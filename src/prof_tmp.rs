#[test]
#[ignore]
fn prof_open() {
    use std::time::Instant;
    let md = include_str!("/tmp/claude-1000/-home-user-development-gpui-markdown-editor/8f9ad15c-b459-4ab5-b781-6be471a11a2d/scratchpad/1mb.md");
    let ext = crate::Extensions::from(vec![Box::new(crate::extensions::WikiLink) as Box<dyn crate::Extension>, Box::new(crate::extensions::Tag), Box::new(crate::extensions::Highlight)]);
    for _ in 0..6 {
        let t = Instant::now();
        let doc = crate::Document::parse_with(md, ext.clone());
        let a = t.elapsed();
        let t = Instant::now();
        let st = crate::edit::EditorState::new(doc);
        let b = t.elapsed();
        let t = Instant::now();
        drop(st);
        let c = t.elapsed();
        let t = Instant::now();
        let doc = crate::Document::parse(md);
        let d = t.elapsed();
        drop(doc);
        let t = Instant::now();
        let n = pulldown_cmark::Parser::new_ext(md, pulldown_cmark::Options::ENABLE_TABLES | pulldown_cmark::Options::ENABLE_STRIKETHROUGH | pulldown_cmark::Options::ENABLE_TASKLISTS | pulldown_cmark::Options::ENABLE_FOOTNOTES | pulldown_cmark::Options::ENABLE_MATH | pulldown_cmark::Options::ENABLE_DEFINITION_LIST).into_offset_iter().count();
        let e = t.elapsed();
        println!("pulldown {e:?} ({n} events)");
        println!("parse_with {a:?} state::new {b:?} drop {c:?} parse(no ext) {d:?}");
    }
}
