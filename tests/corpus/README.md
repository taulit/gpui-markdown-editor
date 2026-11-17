# Test corpus

Input for `tests/roundtrip.rs`: every file must come out of
`Document::parse(x).to_markdown()` byte for byte, and must keep its meaning
when all of its blocks are written again in the normalized style.

The bytes of these files matter (CRLF, a byte order mark, a missing final
newline). `.gitattributes` keeps git from converting line endings here.

## `spec/`

Examples of the Markdown specifications, reduced to the Markdown input
(`example`, `section`, `markdown`). The expected HTML is not needed here.

| File | Source | Examples |
| --- | --- | ---: |
| `commonmark-0.31.2.json` | <https://spec.commonmark.org/0.31.2/spec.json> | 652 |
| `gfm-0.29.json` | <https://github.com/github/cmark-gfm/blob/master/test/spec.txt> | 672 |
| `gfm-extensions.json` | <https://github.com/github/cmark-gfm/blob/master/test/extensions.txt> | 30 |

The CommonMark specification is written by John MacFarlane, the GitHub
Flavored Markdown specification by GitHub, Inc. on top of it. Both are
licensed under [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/),
and so are the files in this directory. `extensions.txt` is part of
cmark-gfm's test suite and is used here under the same terms. In the two
files taken from cmark-gfm the `→` of the specification text is replaced by
the tab it stands for.

The crate excludes `tests/corpus/` from the published package.

## `notes/`

Notes written for this crate, in German and English: checklists, meeting
notes, recipes, tables, images, links, tags and wiki links, nested lists,
code blocks, Obsidian-style files (front matter, callouts, embeds, footnotes,
highlights) and edge cases (`edge-*.md`: CRLF and CR line endings, mixed line
endings, a byte order mark, tabs, no final newline, whitespace only, unclosed
constructs). They are released under the license of the crate.
