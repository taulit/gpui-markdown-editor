//! Completions for wiki links and tags.

use std::sync::{Arc, Mutex};

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

use super::*;
use crate::options::{Completion, CompletionKind};

fn with_completions(calls: Arc<Mutex<Vec<(CompletionKind, String)>>>) -> EditorOptions {
    EditorOptions {
        completions: Some(Arc::new(move |kind, query: &str| {
            calls.lock().unwrap().push((kind, query.to_string()));
            let all: &[&str] = match kind {
                CompletionKind::WikiLink => &["Packing list", "Plan", "Recipes"],
                _ => &["travel", "trip", "work"],
            };
            all.iter()
                .filter(|s| s.to_lowercase().starts_with(&query.to_lowercase()))
                .map(|s| Completion::new(*s))
                .collect()
        })),
        ..options()
    }
}

fn items(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    editor.read_with(cx, |e, _| {
        e.completion
            .as_ref()
            .map(|m| m.items.iter().map(|i| i.label.to_string()).collect())
    })
}

#[gpui::test]
fn wiki_link_completion(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (editor, cx) = open_with(cx, "", with_completions(calls.clone()));
    type_text(cx, "See [[");
    assert_eq!(items(&editor, cx).unwrap().len(), 3);
    type_text(cx, "p");
    assert_eq!(items(&editor, cx).unwrap(), ["Packing list", "Plan"]);
    assert_eq!(
        calls.lock().unwrap().last().cloned(),
        Some((CompletionKind::WikiLink, "p".to_string()))
    );
    press(cx, "down enter");
    assert_eq!(items(&editor, cx), None);
    assert_eq!(caret(&editor, cx), "See Plan‸");
    type_text(cx, " now");
    assert_eq!(md(&editor, cx), "See [[Plan]] now\n");
}

#[gpui::test]
fn tag_completion_click_and_escape(cx: &mut TestAppContext) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (editor, cx) = open_with(cx, "", with_completions(calls));
    type_text(cx, "a#");
    assert_eq!(items(&editor, cx), None, "not inside a word");
    type_text(cx, " #t");
    assert_eq!(items(&editor, cx).unwrap(), ["travel", "trip"]);
    let second = cx.debug_bounds("completion-1").expect("entry");
    cx.simulate_click(second.center(), Modifiers::none());
    assert_eq!(caret(&editor, cx), "a# #trip‸");
    assert_eq!(md(&editor, cx), "a# #trip\n");

    type_text(cx, " #w");
    press(cx, "escape");
    assert_eq!(items(&editor, cx), None);
    // A space ends the list too.
    type_text(cx, "ork #x ");
    assert_eq!(items(&editor, cx), None);
}
