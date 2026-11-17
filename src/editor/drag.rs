//! Dragging into and within the document (plan §7): image files dropped
//! onto the editor, and top-level blocks moved by their drag handle.

use std::path::{Path, PathBuf};

use gpui::{Context, Pixels, Point, Window};

use super::MarkdownEditor;
use crate::edit::Selection;
use crate::model::TextPos;
use crate::view::images::{MAX_IMAGE_BYTES, read_capped};

/// A top-level block (with the folded section of a heading) being moved.
#[derive(Clone, Copy, Debug)]
pub(super) struct BlockDrag {
    /// First and last top-level index of the dragged run.
    pub from: usize,
    pub to: usize,
    /// Where it would be dropped, once the mouse moved.
    pub gap: Option<usize>,
}

/// The MIME type of an image file, by extension.
pub(super) fn image_mime(path: &Path) -> Option<&'static str> {
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

impl MarkdownEditor {
    /// Files dropped at `position`: the caret goes to the drop point, and
    /// image files go through
    /// [`EditorOptions::on_image`](crate::EditorOptions::on_image) like
    /// pasted image data and are inserted there as image blocks. The files
    /// are read on the background executor, regular files of at most
    /// [`MAX_IMAGE_BYTES`] only. Other files are ignored.
    pub(super) fn drop_paths(
        &mut self,
        paths: &[PathBuf],
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.options.on_image.is_none() {
            return;
        }
        let images: Vec<(PathBuf, &'static str)> = paths
            .iter()
            .filter_map(|p| Some((p.clone(), image_mime(p)?)))
            .collect();
        if images.is_empty() {
            return;
        }
        let ts = window.text_system().clone();
        let view = self.view.clone();
        let target = view.query(self.layout_input(), &ts).position_at(position);
        if let Some(pos) = target {
            self.state.set_selection(Selection::caret(pos));
        }
        self.after_change(false, window, cx);
        let read = cx.background_executor().spawn(async move {
            images
                .into_iter()
                .filter_map(|(path, mime)| Some((read_capped(&path, MAX_IMAGE_BYTES).ok()?, mime)))
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let files = read.await;
            this.update_in(cx, |this, window, cx| {
                let Some(handler) = this.options.on_image.clone() else {
                    return;
                };
                for (bytes, mime) in files {
                    if let Some(src) = handler(bytes, mime) {
                        this.state.insert_image(&src, "");
                    }
                }
                this.after_change(false, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Starts dragging top-level block `ix` (with its section if it is a
    /// folded heading).
    pub(super) fn start_block_drag(&mut self, ix: usize) {
        let blocks = self.state.doc.blocks();
        let mut to = ix;
        while to + 1 < blocks.len() && self.hidden.contains(&blocks[to + 1].id()) {
            to += 1;
        }
        self.block_drag = Some(BlockDrag {
            from: ix,
            to,
            gap: None,
        });
    }

    /// Drops the dragged block at its gap, or, if the mouse did not move,
    /// puts the caret into it.
    pub(super) fn finish_block_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.block_drag.take() else {
            return;
        };
        match drag.gap {
            Some(gap) => {
                self.state.move_top_blocks(drag.from, drag.to, gap);
            }
            None => {
                let Some(block) = self.state.doc.blocks().get(drag.from).map(|b| b.id()) else {
                    return;
                };
                let selection = match self
                    .state
                    .locate(block)
                    .and_then(|loc| crate::edit::order::stop_at(&self.state.doc, &loc))
                {
                    Some(stop) if stop.text => Selection::caret(TextPos {
                        block: stop.id,
                        offset: 0,
                    }),
                    _ => Selection::Node(block),
                };
                self.state.set_selection(selection);
            }
        }
        self.after_change(false, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::image_mime;
    use std::path::Path;

    #[test]
    fn image_types_by_extension() {
        assert_eq!(image_mime(Path::new("a/b.PNG")), Some("image/png"));
        assert_eq!(image_mime(Path::new("photo.jpeg")), Some("image/jpeg"));
        assert_eq!(image_mime(Path::new("notes.md")), None);
        assert_eq!(image_mime(Path::new("noext")), None);
    }
}
