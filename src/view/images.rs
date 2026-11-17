//! Loading of image blocks: resolving the source, decoding and downscaling
//! off the UI thread, and the per-source state layout and painting read.
//!
//! Only images of blocks around the viewport are requested (see
//! [`DocumentView::load_images`](super::DocumentView::load_images)), so a
//! long document with many images decodes only what is near the screen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, Asset, RenderImage, SharedString, SharedUri, Window};

use crate::model::{Block, BlockKind};

/// Images wider than this (in pixels) are scaled down after decoding.
const MAX_DECODED_WIDTH: u32 = 1600;
/// The largest image file or download read, in bytes.
pub(crate) const MAX_IMAGE_BYTES: u64 = 64 << 20;
/// The largest width or height a decoder accepts, in pixels.
const MAX_DIMENSION: u32 = 16_384;
/// The most memory a decoder may allocate, in bytes.
const MAX_DECODE_ALLOC: u64 = 256 << 20;

/// Reads a regular file of at most `cap` bytes. Anything else (a
/// directory, FIFO or device such as `/dev/zero`) fails without being
/// opened, and a file that grows past `cap` while being read fails too.
pub(crate) fn read_capped(path: &Path, cap: u64) -> std::io::Result<Vec<u8>> {
    use std::io::{Error, ErrorKind, Read as _};
    let meta = std::fs::metadata(path)?;
    if !meta.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
    }
    if meta.len() > cap {
        return Err(Error::new(ErrorKind::InvalidData, "file too large"));
    }
    let file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(Error::new(ErrorKind::InvalidInput, "not a regular file"));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(cap + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > cap {
        return Err(Error::new(ErrorKind::InvalidData, "file too large"));
    }
    Ok(bytes)
}

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

    #[allow(clippy::manual_async_fn)] // the future must be `Send + 'static`
    fn load(
        source: Self::Source,
        _cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        async move {
            let bytes =
                read_capped(&source, MAX_IMAGE_BYTES).map_err(|e| Arc::from(e.to_string()))?;
            decode(&bytes).map_err(Arc::from)
        }
    }
}

/// Loads an image over HTTP(S) with the app's HTTP client: the body is
/// read up to [`MAX_IMAGE_BYTES`], decoded with the same limits as local
/// files (SVG through GPUI's renderer) on the background executor.
/// Animated images show their first frame.
pub(crate) enum UrlImageLoader {}

impl Asset for UrlImageLoader {
    type Source = SharedUri;
    type Output = Result<Decoded, Arc<str>>;

    #[allow(clippy::manual_async_fn)] // the future must be `Send + 'static`
    fn load(
        source: Self::Source,
        cx: &mut App,
    ) -> impl Future<Output = Self::Output> + Send + 'static {
        let client = cx.http_client();
        let svg = cx.svg_renderer();
        async move {
            use futures::AsyncReadExt as _;
            let error = |e: &dyn std::fmt::Display| Arc::<str>::from(e.to_string());
            let mut response = client
                .get(source.as_ref(), ().into(), true)
                .await
                .map_err(|e| error(&e))?;
            if !response.status().is_success() {
                return Err(error(&response.status()));
            }
            let mut bytes = Vec::new();
            response
                .body_mut()
                .take(MAX_IMAGE_BYTES + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|e| error(&e))?;
            if bytes.len() as u64 > MAX_IMAGE_BYTES {
                return Err("image too large".into());
            }
            if image::guess_format(&bytes).is_ok() {
                return decode(&bytes).map_err(Arc::from);
            }
            let image = svg
                .render_single_frame(&bytes, 1.0)
                .map_err(|e| error(&e))?;
            let size = image.size(0);
            let (width, height) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
            if width == 0 || height == 0 {
                return Err("empty image".into());
            }
            Ok(Decoded {
                image,
                width,
                height,
            })
        }
    }
}

/// Decodes image bytes into a GPU-ready image (BGRA), at most
/// [`MAX_DECODED_WIDTH`] wide.
/// The decoder refuses images larger than [`MAX_DIMENSION`] on a side or
/// needing more than [`MAX_DECODE_ALLOC`] bytes.
pub(crate) fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
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
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let frame = image::Frame::new(rgba);
    Ok(Decoded {
        image: Arc::new(RenderImage::new(smallvec::smallvec![frame])),
        width,
        height,
    })
}

/// The MIME type of an image file, by extension.
pub(crate) fn image_mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "tif" | "tiff" => "image/tiff",
        _ => return None,
    })
}

/// The size in bytes of the local file `src` names (through the resolver,
/// or an absolute path without one), if it is a regular file.
pub(crate) fn file_size(src: &str, resolver: Option<&Resolve>) -> Option<u64> {
    match resolve(src, resolver)? {
        Source::File(path) => std::fs::metadata(&path)
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len()),
        Source::Url(_) => None,
    }
}

/// A borrowed [`SrcResolver`](crate::options::SrcResolver).
pub(crate) type Resolve = dyn Fn(&str) -> Option<PathBuf>;

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
fn resolve(src: &str, resolver: Option<&Resolve>) -> Option<Source> {
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
    resolver: Option<&Resolve>,
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
        Some(Source::Url(uri)) => match window.use_asset::<UrlImageLoader>(&uri, cx) {
            None => ImageInfo::Loading,
            Some(Ok(d)) => ImageInfo::Ready {
                image: d.image,
                width: d.width,
                height: d.height,
            },
            Some(Err(_)) => ImageInfo::Failed,
        },
    }
}

/// Removes `src` from the asset cache, so that it is decoded again when it
/// comes back into view.
pub(crate) fn forget(src: &str, resolver: Option<&Resolve>, cx: &mut App) {
    match resolve(src, resolver) {
        Some(Source::File(path)) => cx.remove_asset::<FileImageLoader>(&path),
        Some(Source::Url(uri)) => cx.remove_asset::<UrlImageLoader>(&uri),
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
    fn decoder_limits_refuse_huge_images() {
        let mut png = Vec::new();
        let img = image::GrayImage::new(MAX_DIMENSION + 1, 1);
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(decode(&png).is_err());
    }

    #[test]
    fn reads_only_regular_files_up_to_the_cap() {
        let dir = std::env::temp_dir().join(format!("gme-readcap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.bin");
        std::fs::write(&file, b"12345").unwrap();
        assert_eq!(read_capped(&file, 5).unwrap(), b"12345");
        assert!(read_capped(&file, 4).is_err());
        // A sparse file larger than the cap is refused from its metadata.
        let big = dir.join("big.bin");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES + 1)
            .unwrap();
        assert!(read_capped(&big, MAX_IMAGE_BYTES).is_err());
        assert!(read_capped(&dir, MAX_IMAGE_BYTES).is_err(), "a directory");
        #[cfg(unix)]
        {
            let start = std::time::Instant::now();
            assert!(read_capped(Path::new("/dev/zero"), MAX_IMAGE_BYTES).is_err());
            let fifo = dir.join("fifo");
            let made = std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .is_ok_and(|s| s.success());
            if made {
                // Opening a FIFO without a writer would block.
                assert!(read_capped(&fifo, MAX_IMAGE_BYTES).is_err());
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(1));
        }
        std::fs::remove_dir_all(&dir).unwrap();
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
