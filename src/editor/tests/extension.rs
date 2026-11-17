//! Extension hooks: input rules, click events, slash entries, mark styles.

use gpui::{Modifiers, TestAppContext};

use super::*;
use crate::extension::{Extension, InlineMatch, MarkColor, MarkStyle, SlashEntry};
use crate::model::{CustomMarkId, Mark};

/// `@name` mentions, as a host would write them.
struct Mention;

impl Mention {
    const MARK: CustomMarkId = CustomMarkId("mention");
}

impl Extension for Mention {
    fn name(&self) -> &'static str {
        "mention"
    }

    fn parse_inline(&self, text: &str) -> Vec<InlineMatch> {
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(at) = text[from..].find('@') {
            let start = from + at;
            let len = text[start + 1..]
                .find(|c: char| !c.is_alphanumeric())
                .unwrap_or(text.len() - start - 1);
            if len > 0 {
                out.push(InlineMatch {
                    range: start..start + 1 + len,
                    text: text[start..start + 1 + len].to_string(),
                    mark: Mark::Custom {
                        id: Self::MARK,
                        attr: None,
                    },
                });
            }
            from = start + 1 + len;
        }
        out
    }

    fn serialize_mark(&self, mark: &Mark, text: &str) -> Option<String> {
        matches!(mark, Mark::Custom { id, .. } if *id == Self::MARK).then(|| text.to_string())
    }

    fn mark_style(&self, id: CustomMarkId) -> Option<MarkStyle> {
        (id == Self::MARK).then(|| {
            MarkStyle::new()
                .color(MarkColor::Fixed(gpui::red()))
                .italic()
        })
    }

    fn input_rule(&self, before: &str) -> Option<InlineMatch> {
        let body = before.strip_suffix(' ')?;
        self.parse_inline(body)
            .pop()
            .filter(|m| m.range.end == body.len())
    }

    fn click(&self, mark: &Mark, text: &str) -> Option<EditorEvent> {
        matches!(mark, Mark::Custom { id, .. } if *id == Self::MARK)
            .then(|| EditorEvent::LinkClicked(format!("mention:{}", &text[1..])))
    }

    fn slash_items(&self) -> Vec<SlashEntry> {
        vec![SlashEntry {
            id: "mention",
            label: "Mention",
            keywords: &["person", "user"],
            insert: "@",
        }]
    }
}

fn marks(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
) -> Vec<(String, Mark)> {
    editor.read_with(cx, |e, _| {
        let content = e.document().blocks()[0].content();
        content
            .marks()
            .iter()
            .map(|m| (content.text()[m.range.clone()].to_string(), m.mark.clone()))
            .collect()
    })
}

#[gpui::test]
fn bundled_input_rules_mark_wiki_links_and_tags(cx: &mut TestAppContext) {
    let (editor, cx) = open(cx, "");
    type_text(cx, "See [[Plan|the plan]] for #work today");
    let found = marks(&editor, cx);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].0, "the plan");
    assert_eq!(found[1].0, "#work");
    assert_eq!(md(&editor, cx), "See [[Plan|the plan]] for #work today\n");
    assert_eq!(caret(&editor, cx), "See the plan for #work today‸");
    // The rule is one undo step: undo gives back the typed syntax.
    let (editor, cx) = (editor, cx);
    press(cx, "secondary-z");
    assert!(md(&editor, cx).starts_with("See [[Plan|the plan]] for #work"));
}

#[gpui::test]
fn custom_extension_hooks(cx: &mut TestAppContext) {
    let options = EditorOptions {
        extensions: vec![Box::new(Mention)],
        ..Default::default()
    };
    let (editor, cx) = open_with(cx, "", options);
    let events = record_events(&editor, cx);

    // Slash entry, then the input rule on the space after the name.
    type_text(cx, "/ment");
    press(cx, "enter");
    type_text(cx, "ann ");
    assert_eq!(caret(&editor, cx), "@ann ‸");
    let found = marks(&editor, cx);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, "@ann");

    // Ctrl/Cmd+click reports what the extension says.
    let b = block(&editor, cx, 0);
    let p = point_of(
        &editor,
        cx,
        TextPos {
            block: b,
            offset: 2,
        },
    );
    let ctrl = if cfg!(target_os = "macos") {
        Modifiers::command()
    } else {
        Modifiers::control()
    };
    cx.simulate_click(p, ctrl);
    assert_eq!(
        count(&events, &EditorEvent::LinkClicked("mention:ann".into())),
        1
    );

    // The style is the extension's.
    let style = editor.read_with(cx, |e, _| {
        e.document()
            .extensions
            .iter()
            .find_map(|x| x.mark_style(Mention::MARK))
    });
    assert_eq!(
        style.and_then(|s| s.color),
        Some(MarkColor::Fixed(gpui::red()))
    );
}

/// Rewrites a typed `/` into `÷` and `#` into `♯` (both multi-byte).
struct Rewrite;

impl Extension for Rewrite {
    fn name(&self) -> &'static str {
        "rewrite"
    }

    fn input_rule(&self, before: &str) -> Option<InlineMatch> {
        let (from, to) = [("/", "÷"), ("#", "♯")]
            .into_iter()
            .find(|(from, _)| before.ends_with(from))?;
        Some(InlineMatch {
            range: before.len() - from.len()..before.len(),
            text: to.to_string(),
            mark: Mark::Custom {
                id: CustomMarkId("rewrite"),
                attr: None,
            },
        })
    }
}

#[gpui::test]
fn triggers_rewritten_by_an_input_rule_do_not_panic(cx: &mut TestAppContext) {
    let options = EditorOptions {
        extensions: vec![Box::new(Rewrite), Box::new(crate::extensions::Tag)],
        completions: Some(std::sync::Arc::new(|_, _: &str| Vec::new())),
        ..Default::default()
    };
    let (editor, cx) = open_with(cx, "", options);
    type_text(cx, "a /");
    type_text(cx, " #");
    type_text(cx, "#");
    let text = editor.read_with(cx, |e, _| e.document().blocks()[0].text().to_string());
    assert!(text.contains('÷') && text.contains('♯'), "{text:?}");
    assert!(editor.read_with(cx, |e, _| e.slash.is_none() && e.completion.is_none()));
}
