# Iced line-height patches

`iced_core` and `iced_graphics` are vendored from the crates.io **0.14.0**
releases (upstream: https://github.com/iced-rs/iced/tree/0.14.0).
`../Cargo.toml` patches both packages; all other Iced packages remain upstream.
The upstream MIT notice is in `LICENSE-iced`.

## Patch surface

Only these source files differ from the released crates:

- `iced_core/src/text/highlighter.rs`: adds the default
  `Highlighter::line_height_scale(&self, line: &str) -> f32` hook.
  It returns `1.0` unless an application opts in, and is queried immediately
  before the same line is fed to `highlight_line`. It must not advance state.
  Scales are relative to the editor's ordinary line height, not font size.
- `iced_graphics/src/text/editor.rs`: applies the scale to the line's default
  attributes and highlighted spans, rejects invalid/overflowing heights, and
  derives selection/caret/viewport geometry from actual shaped heights.
  Metric changes invalidate the line formats; reflow invalidates cached
  selection geometry. Highlighting repeats for newly exposed lines and keeps
  a previously visible cursor visible without jumping back to an offscreen
  cursor during scrolling.

This reuses Iced's existing ordered highlighter traversal to classify lines,
including empty ones; it does not encode Markdown in the renderer or change
`Format`/widget APIs. It is still an extension of the highlight/reflow path,
not a new independent rich-text document model. No source characters are added.

## Application policy

`../src/typography.rs` owns the shared write/preview line-height and
paragraph-gap scales; `../src/highlight.rs` owns source-line classification
and fence tracking. Preview uses ordinary widget layout with the same gap,
not these editor patches. Blank source lines (empty or containing only ASCII spaces/tabs) outside fenced
code get extra height; code blanks do not. This is source-line spacing, not
semantic block margins: consecutive, leading, and trailing blanks each retain
their own line height. Other highlighters, including the note editors, retain
normal spacing unless they override the hook. The widget retains its ordinary
caret height on blank lines.

## Maintenance and validation

On an Iced upgrade, compare the two files above against the matching upstream
release, reapply only these changes, and run:

```sh
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
```

`../src/highlight/layout_tests.rs` exercises the renderer/editor boundary:
clicking, selections/copy, scrolling, wrapping, metric changes, edits, cached
geometry, viewport limits, default opt-out, and fence context on unvisited lines.
Vendored Rust files follow upstream's rustfmt settings (`edition = "2024"`,
`max_width = 80`). Avoid reformatting unrelated upstream files.
