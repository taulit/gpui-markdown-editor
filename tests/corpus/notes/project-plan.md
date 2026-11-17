# Project plan

## Milestones

1. Parser
   - block structure
   - inline marks
     - bold, italic, strike
     - links and images
   - extensions
2. Serializer
   1. normalized style
   2. lossless copy
3. Editor

   The editor is built on top of both.

   ```rust
   let doc = Document::parse(source);
   assert_eq!(doc.to_markdown(), source);
   ```

## Risks

* escaping is hard
* tables with `|` in cells
+ a different list

---

Status: ==green==, see [[Roadmap|the roadmap]] and #project/editor.
