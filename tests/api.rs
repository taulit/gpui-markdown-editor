//! Tests of the public `MarkdownEditor` API against a test window.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{Entity, Focusable as _, TestAppContext, VisualTestContext};
use gpui_markdown_editor::{
    Block, BlockId, Command, DocMeta, EditorEvent, EditorOptions, MarkdownEditor,
};

fn open_editor(cx: &mut TestAppContext) -> (Entity<MarkdownEditor>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        gpui_markdown_editor::init(cx);
    });
    let (editor, cx) =
        cx.add_window_view(|window, cx| MarkdownEditor::new(window, cx, EditorOptions::default()));
    cx.update(|window, cx| editor.focus_handle(cx).focus(window, cx));
    (editor, cx)
}

fn set_markdown(editor: &Entity<MarkdownEditor>, cx: &mut VisualTestContext, md: &str) {
    editor.update(cx, |editor, cx| editor.set_markdown(md, cx));
    cx.run_until_parked();
}

fn record_events(
    editor: &Entity<MarkdownEditor>,
    cx: &mut VisualTestContext,
) -> Rc<RefCell<Vec<EditorEvent>>> {
    let events = Rc::new(RefCell::new(Vec::new()));
    cx.update(|_, cx| {
        let events = events.clone();
        cx.subscribe(editor, move |_, event: &EditorEvent, _| {
            events.borrow_mut().push(event.clone());
        })
        .detach();
    });
    events
}

fn changes(events: &RefCell<Vec<EditorEvent>>) -> usize {
    events
        .borrow()
        .iter()
        .filter(|event| matches!(event, EditorEvent::Changed))
        .count()
}

#[gpui::test]
fn markdown_round_trips_byte_for_byte(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    assert_eq!(editor.read_with(cx, |editor, _| editor.markdown()), "");

    for source in [
        "# Title\n\nText\n",
        "no trailing newline",
        "windows\r\nline endings\r\n",
        "mixed\r\nendings\n\n\n",
        "\n\n  leading blank lines and trailing spaces  \n\t\n",
        "| a | b |\n|---|---|\n| 1 | 2 |",
        "",
    ] {
        editor.update(cx, |editor, cx| editor.set_markdown(source, cx));
        // Readable before the next frame.
        assert_eq!(editor.read_with(cx, |editor, _| editor.markdown()), source);

        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.markdown(), source);
            assert_eq!(editor.document().source(), source);
            assert!(!editor.is_dirty());
        });
    }
}

#[gpui::test]
fn typing_edits_marks_dirty_and_emits_changed(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    let events = record_events(&editor, cx);

    set_markdown(&editor, cx, "line one\r\nline two");
    assert_eq!(changes(&events), 0, "set_markdown must not emit Changed");

    // The cursor is at the start after loading. The edited paragraph is
    // written normalized; its line break stays one (`LineBreaks::Keep`, the
    // default), in the line ending of the source.
    cx.simulate_input("ab");
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.markdown(), "abline one\r\nline two");
        assert!(editor.is_dirty());
    });
    assert_eq!(changes(&events), 2);
    assert!(events.borrow().contains(&EditorEvent::SelectionChanged));

    editor.update(cx, |editor, _| editor.mark_saved());
    assert!(!editor.read_with(cx, |editor, _| editor.is_dirty()));

    cx.simulate_input("c");
    assert!(editor.read_with(cx, |editor, _| editor.is_dirty()));

    set_markdown(&editor, cx, "fresh");
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.markdown(), "fresh");
        assert!(!editor.is_dirty());
    });
    assert_eq!(changes(&events), 3);
}

#[gpui::test]
fn text_commands_edit_the_content(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    let events = record_events(&editor, cx);
    set_markdown(&editor, cx, "World");

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            // Works without waiting for a frame after `set_markdown`.
            editor.set_markdown("World", cx);
            editor.command(Command::InsertText("Hello ".into()), window, cx);
            assert_eq!(editor.markdown(), "Hello World");
            assert!(editor.is_dirty());

            editor.command(Command::SelectAll, window, cx);
            editor.command(Command::InsertMarkdown("# New\n".into()), window, cx);
            assert_eq!(editor.markdown(), "# New");

            // Commands that do not apply change nothing; view commands of
            // the next phase are accepted.
            editor.command(Command::ToggleTask, window, cx);
            editor.command(Command::OpenFind, window, cx);
            editor.command(Command::FoldHeading, window, cx);
            assert_eq!(editor.markdown(), "# New");

            // A rule is followed by an empty paragraph for the caret, which
            // does not change the Markdown while it is empty.
            editor.command(Command::InsertRule, window, cx);
            assert_eq!(editor.markdown(), "# New\n\n---");
        });
    });
    cx.run_until_parked();
    assert_eq!(changes(&events), 3);
}

#[gpui::test]
fn undo_and_redo(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    set_markdown(&editor, cx, "base");

    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.command(Command::InsertText("new ".into()), window, cx);
            assert_eq!(editor.markdown(), "new base");
            editor.command(Command::Undo, window, cx);
            assert_eq!(editor.markdown(), "base");
            editor.command(Command::Redo, window, cx);
            assert_eq!(editor.markdown(), "new base");
        });
    });

    // Loading clears the history: there is nothing to undo back to.
    set_markdown(&editor, cx, "other");
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.command(Command::Undo, window, cx);
            assert_eq!(editor.markdown(), "other");
            assert!(!editor.is_dirty());
        });
    });
}

#[gpui::test]
fn find_returns_ranges_in_document_order(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    set_markdown(
        &editor,
        cx,
        "# Milk run\n\n- [ ] milk\n- [x] Bread\n\nNo MILK left.\n",
    );

    editor.read_with(cx, |editor, _| {
        let hits = editor.find("milk");
        assert_eq!(hits.len(), 3);
        for hit in &hits {
            assert_eq!(hit.start.block, hit.end.block);
            let block = editor.document().block(hit.start.block).unwrap();
            let found = &block.text()[hit.start.offset..hit.end.offset];
            assert_eq!(found.to_lowercase(), "milk");
        }
        // Document order: by block, then by offset within the block.
        let mut order = Vec::new();
        collect_ids(editor.document().blocks(), &mut order);
        let positions: Vec<_> = hits
            .iter()
            .map(|hit| {
                let block = order.iter().position(|id| *id == hit.start.block);
                (block.unwrap(), hit.start.offset)
            })
            .collect();
        assert!(positions.is_sorted());

        assert!(editor.find("").is_empty());
        assert!(editor.find("butter").is_empty());
    });
}

fn collect_ids(blocks: &[Block], out: &mut Vec<BlockId>) {
    for block in blocks {
        out.push(block.id());
        collect_ids(block.children(), out);
    }
}

#[gpui::test]
fn word_count_uses_unicode_words(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    assert_eq!(editor.read_with(cx, |editor, _| editor.word_count()), 0);

    set_markdown(&editor, cx, "Über-coole Äpfel, 3 Stück.\n\nCan't stop.\n");
    assert_eq!(editor.read_with(cx, |editor, _| editor.word_count()), 7);

    cx.simulate_input("new ");
    assert_eq!(editor.read_with(cx, |editor, _| editor.word_count()), 8);
}

#[gpui::test]
fn stored_state_notifies_without_meta_event(cx: &mut TestAppContext) {
    let (editor, cx) = open_editor(cx);
    let events = record_events(&editor, cx);
    set_markdown(&editor, cx, "one two one\n");

    let notified = Rc::new(RefCell::new(0));
    cx.update(|_, cx| {
        let notified = notified.clone();
        cx.observe(&editor, move |_, _| *notified.borrow_mut() += 1)
            .detach();
    });

    let mut meta = DocMeta::default();
    meta.folded_headings.insert(0);
    meta.column_widths.insert(0, vec![120., 200.]);

    editor.update(cx, |editor, cx| {
        let hits = editor.find("one");
        assert_eq!(hits.len(), 2);
        editor.highlight(hits.clone(), cx);
        editor.scroll_to(hits[1], cx);
        assert_eq!(editor.highlights(), hits);

        editor.set_meta(meta.clone(), cx);
        assert_eq!(editor.meta(), &meta);

        let mut style = editor.style().clone();
        style.text_size = gpui::px(18.);
        editor.set_style(style.clone(), cx);
        assert_eq!(editor.style(), &style);
    });
    cx.run_until_parked();

    assert!(*notified.borrow() > 0);
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());

    // Highlights belong to the content they were found in.
    set_markdown(&editor, cx, "something else\n");
    editor.read_with(cx, |editor, _| {
        assert!(editor.highlights().is_empty());
        assert_eq!(editor.meta(), &meta);
    });
}
