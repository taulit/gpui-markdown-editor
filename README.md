# gpui-markdown-editor

WYSIWYG Markdown editor component for GPUI (Zed's UI framework)

`gpui-markdown-editor` is an editor component that shows Markdown as formatted text while you type. Documents round-trip losslessly: what you load is what gets saved, byte for byte, unless you edited it. It feels like Apple Notes or Notion, with no visible `**` or `#` markers, and it is built for [gpui-component](https://github.com/longbridge/gpui-component).

## Status

Early development. The API is unstable until 0.1. Changes between tags are listed in [CHANGELOG.md](CHANGELOG.md); current progress is in [docs/STATUS.md](docs/STATUS.md).

## Usage

```toml
[dependencies]
gpui-markdown-editor = { git = "https://github.com/taulit/gpui-markdown-editor", tag = "v0.0.3" }
```

This crate pins `gpui-pre =0.3.12` and `gpui-component =0.10.0`. Host applications must use the same versions.

## Building

```sh
cargo run --example basic
cargo test
```

## License

Apache-2.0, see [LICENSE](LICENSE).
