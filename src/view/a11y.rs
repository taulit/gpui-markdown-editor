//! The accessibility tree of the document: one node per visible block.
//!
//! GPUI lets a custom element add synthetic *leaf* nodes below its own
//! node. The document element is a node with role `Document`; each visible
//! block (nested blocks too) becomes a leaf with its role (heading with
//! level, paragraph, list item, checkbox with its state, quote, code,
//! table, image, rule), its text as label and its bounds. The block with
//! the caret is marked selected. Checkboxes take the `Click` action.
//!
//! Not expressible with leaf nodes: text runs inside blocks, so the caret
//! position and the selected text are not reported as text positions.

use gpui::{Bounds, Pixels, Role};

use crate::model::{Block, BlockId, BlockKind};

use super::LayoutInput;
use super::state::ViewState;

/// One synthetic node.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct A11yBlock {
    pub id: BlockId,
    pub role: Role,
    pub label: String,
    pub level: Option<usize>,
    pub toggled: Option<bool>,
    pub selected: bool,
    pub bounds: Bounds<Pixels>,
}

/// Longest label passed on, in chars.
const MAX_LABEL: usize = 2000;

fn role_of(block: &Block) -> (Role, Option<usize>, Option<bool>) {
    match &block.kind {
        BlockKind::Heading(level) => (Role::Heading, Some(usize::from(*level)), None),
        BlockKind::BulletItem | BlockKind::OrderedItem { .. } => (Role::ListItem, None, None),
        BlockKind::TaskItem { checked } => (Role::CheckBox, None, Some(*checked)),
        BlockKind::Quote => (Role::Blockquote, None, None),
        BlockKind::CodeBlock { .. } => (Role::Code, None, None),
        BlockKind::Table(_) => (Role::Table, None, None),
        BlockKind::Image(_) => (Role::Image, None, None),
        BlockKind::Rule => (Role::Splitter, None, None),
        _ => (Role::Paragraph, None, None),
    }
}

fn label_of(block: &Block, divider: &str) -> String {
    let text = match &block.kind {
        BlockKind::Image(image) => image.alt.to_string(),
        BlockKind::Table(table) => table
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|c| c.content().text())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .collect::<Vec<_>>()
            .join("; "),
        BlockKind::Rule => divider.to_string(),
        _ => block.content.text.clone(),
    };
    text.chars().take(MAX_LABEL).collect()
}

/// The nodes of the blocks laid out in the last frame.
pub(crate) fn nodes(
    state: &mut ViewState,
    input: &LayoutInput<'_>,
    strings: &crate::strings::EditorStrings,
    caret: Option<BlockId>,
    ts: &gpui::WindowTextSystem,
) -> Vec<A11yBlock> {
    let divider = strings.get("a11y.divider");
    let mut out = Vec::new();
    for ix in state.visible.clone() {
        let Some(block) = input.document.blocks().get(ix) else {
            continue;
        };
        if input.hidden.contains(&block.id) {
            continue;
        }
        let Some(layout) = state.layout(input, ix, ts) else {
            continue;
        };
        let mut push = |b: &Block, r: Bounds<Pixels>, state: &mut ViewState| {
            let (role, level, toggled) = role_of(b);
            out.push(A11yBlock {
                id: b.id,
                role,
                label: label_of(b, &divider),
                level,
                toggled,
                selected: caret == Some(b.id),
                bounds: state.window_rect(ix, r),
            });
        };
        fn walk(
            b: &Block,
            rects: &[(BlockId, super::layout::RectKind, Bounds<Pixels>)],
            state: &mut ViewState,
            push: &mut impl FnMut(&Block, Bounds<Pixels>, &mut ViewState),
        ) {
            for child in &b.children {
                if let Some((_, _, r)) = rects.iter().find(|(id, _, _)| *id == child.id) {
                    push(child, *r, state);
                }
                walk(child, rects, state, push);
            }
        }
        let own = layout
            .rects
            .iter()
            .find(|(id, _, _)| *id == block.id)
            .map(|(_, _, r)| *r)
            .unwrap_or_else(|| {
                Bounds::new(
                    gpui::point(Pixels::ZERO, Pixels::ZERO),
                    gpui::size(state.width, layout.height),
                )
            });
        push(block, own, state);
        walk(block, &layout.rects, state, &mut push);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Document;

    #[test]
    fn roles_levels_states_and_labels() {
        let doc = Document::parse(
            "## Plan\n\n- [x] done\n\n1. one\n\n> q\n\n---\n\n![Lake](l.png)\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        );
        let got: Vec<_> = doc
            .blocks()
            .iter()
            .map(|b| {
                let (role, level, toggled) = role_of(b);
                (role, level, toggled, label_of(b, "Divider"))
            })
            .collect();
        assert_eq!(got[0], (Role::Heading, Some(2), None, "Plan".into()));
        assert_eq!(got[1], (Role::CheckBox, None, Some(true), "done".into()));
        assert_eq!(got[2].0, Role::ListItem);
        assert_eq!(got[3].0, Role::Blockquote);
        assert_eq!(got[4], (Role::Splitter, None, None, "Divider".into()));
        assert_eq!(got[5], (Role::Image, None, None, "Lake".into()));
        assert_eq!(got[6], (Role::Table, None, None, "a, b; 1, 2".into()));
    }
}
