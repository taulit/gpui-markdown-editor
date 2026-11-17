//! Loading of image blocks: resolving the source, decoding and downscaling
//! off the UI thread, and the per-source state layout and painting read.
//!
//! Only images of blocks around the viewport are requested (see
//! [`DocumentView::load_images`](super::DocumentView::load_images)), so a
//! long document with many images decodes only what is near the screen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, Asset, ImageAssetLoader, RenderImage, Resource, SharedString, SharedUri, Window};

use crate::model::{Block, BlockKind};

/// Images wider than this (in pixels) are scaled down after decoding.
const MAX_DECODED_WIDTH: u32 = 1600;

/// The load state of one image source.
#[derive(Clone)]
pub(crate) enum ImageInfo {
    Loading,
    /// Decoded, with the size of the original image in pixels.
    Ready {
        image: Arc<RenderImage>,
        width: u32,
        height: u32,
    },
    /// Missing, unreadable or not an image.
    Failed,
}

impl ImageInfo {
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Loading, Self::Loading) | (Self::Failed, Self::Failed) => true,
            (Self::Ready { image: a, .. }, Self::Ready { image: b, .. }) => a.id == b.id,
            _ => false,
        }
    }
}

impl std::fmt::Debug for ImageInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Loading => write!(f, "Loading"),
            Self::Ready { width, height, .. } => write!(f, "Ready({width}x{height})"),
            Self::Failed => write!(f, "Failed"),
        }
    }
}

/// Image states by source as written in the Markdown.
pub(crate) type ImageStates = HashMap<SharedString, ImageInfo>;

/// A decoded local image with the size of the original.
#[derive(Clone)]
pub(crate) struct Decoded {
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
}

/// Loads a local image file: reads, decodes and downscales it on the
/// background executor.
pub(crate) enum FileImageLoader {}

impl Asset for FileImageLoader {
    type Source = Arc<Path>;
    type Output = Result<Decoded, Arc<str>>;

    fn load(
        source: Self::Source,
        _cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            let bytes = std::fs::read(&source).map_err(|e| Arc::from(e.to_string()))?;
            decode(&bytes).map_err(Arc::from)
        }
    }
}

/// Decodes image bytes into a GPU-ready image (BGRA), at most
/// [`MAX_DECODED_WIDTH`] wide.
pub(crate) fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let image = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());
    if width == 0 || height == 0 {
        return Err("empty image".into());
    }
    let image = if width > MAX_DECODED_WIDTH {
        let h = ((height as u64 * MAX_DECODED_WIDTH as u64) / width as u64).max(1) as u32;
        image.resize_exact(MAX_DECODED_WIDTH, h, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut rgba = image.into_rgba8();
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let frame = image::Frame::new(rgba);
    Ok(Decoded {
        image: Arc::new(RenderImage::new(smallvec::smallvec![frame])),
        width,
        height,
    })
}

/// Where an image comes from.
enum Source {
    File(Arc<Path>),
    Url(SharedUri),
}

fn is_url(src: &str) -> bool {
    src.starts_with("https://") || src.starts_with("http://")
}

/// Resolves `src` with the host's resolver: local files through it, URLs
/// as they are, absolute paths without a resolver.
fn resolve(src: &str, resolver: Option<&dyn Fn(&str) -> Option<PathBuf>>) -> Option<Source> {
    if let Some(resolver) = resolver
        && let Some(path) = resolver(src)
    {
        return Some(Source::File(path.into()));
    }
    if is_url(src) {
        return Some(Source::Url(SharedUri::from(src.to_string())));
    }
    if resolver.is_none() && Path::new(src).is_absolute() {
        return Some(Source::File(PathBuf::from(src).into()));
    }
    None
}

/// Requests `src` from the asset system (which notifies the current view
/// when it is loaded) and returns its state now.
pub(crate) fn request(
    src: &str,
    resolver: Option<&dyn Fn(&str) -> Option<PathBuf>>,
    window: &mut Window,
    cx: &mut App,
) -> ImageInfo {
    match resolve(src, resolver) {
        None => ImageInfo::Failed,
        Some(Source::File(path)) => match window.use_asset::<FileImageLoader>(&path, cx) {
            None => ImageInfo::Loading,
            Some(Ok(d)) => ImageInfo::Ready {
                image: d.image,
                width: d.width,
                height: d.height,
            },
            Some(Err(_)) => ImageInfo::Failed,
        },
        Some(Source::Url(uri)) => {
            match window.use_asset::<ImageAssetLoader>(&Resource::Uri(uri), cx) {
                None => ImageInfo::Loading,
                Some(Ok(image)) => {
                    let size = image.size(0);
                    let (width, height) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
                    if width == 0 || height == 0 {
                        ImageInfo::Failed
                    } else {
                        ImageInfo::Ready {
                            image,
                            width,
                            height,
                        }
                    }
                }
                Some(Err(_)) => ImageInfo::Failed,
            }
        }
    }
}

/// Removes `src` from the asset cache, so that it is decoded again when it
/// comes back into view.
pub(crate) fn forget(src: &str, resolver: Option<&dyn Fn(&str) -> Option<PathBuf>>, cx: &mut App) {
    match resolve(src, resolver) {
        Some(Source::File(path)) => cx.remove_asset::<FileImageLoader>(&path),
        Some(Source::Url(uri)) => cx.remove_asset::<ImageAssetLoader>(&Resource::Uri(uri)),
        None => {}
    }
}

/// Updates `states` for `src`; `true` if the state changed.
pub(crate) fn update(states: &mut ImageStates, src: &SharedString, info: ImageInfo) -> bool {
    if states.get(src).is_some_and(|old| old.same(&info)) {
        return false;
    }
    states.insert(src.clone(), info);
    true
}

/// The image sources in `block` and its descendants.
pub(crate) fn sources(block: &Block, out: &mut Vec<SharedString>) {
    if let BlockKind::Image(image) = &block.kind
        && !image.src.is_empty()
    {
        out.push(image.src.clone());
    }
    for child in &block.children {
        sources(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_downscales() {
        let mut png = Vec::new();
        let img = image::RgbaImage::from_pixel(3200, 100, image::Rgba([10, 20, 30, 255]));
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let d = decode(&png).unwrap();
        assert_eq!((d.width, d.height), (3200, 100));
        let size = d.image.size(0);
        assert_eq!((size.width.0, size.height.0), (1600, 50));
        // BGRA.
        assert_eq!(&d.image.as_bytes(0).unwrap()[..4], &[30, 20, 10, 255]);
    }

    #[test]
    fn rejects_garbage() {
        assert!(decode(b"not an image").is_err());
        assert!(decode(&[]).is_err());
    }

    #[test]
    fn resolves_sources() {
        assert!(matches!(
            resolve("https://x.org/a.png", None),
            Some(Source::Url(_))
        ));
        assert!(resolve("relative.png", None).is_none());
        let abs = if cfg!(windows) { "C:\\a.png" } else { "/a.png" };
        assert!(matches!(resolve(abs, None), Some(Source::File(_))));
        let resolver = |s: &str| (s == "rel.png").then(|| PathBuf::from("/base/rel.png"));
        assert!(matches!(
            resolve("rel.png", Some(&resolver)),
            Some(Source::File(_))
        ));
        assert!(resolve("missing.png", Some(&resolver)).is_none());
    }
}
