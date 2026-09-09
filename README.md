# agmawrite

A minimal Markdown editor: a black canvas, a blinking cursor, and quiet
syntax. Markdown markers — hashes, bullets, emphasis, links — render dimmer
than the prose while you write; `Ctrl + P` flips to a rendered preview you
navigate with vim motions. The cursor mirrors between the two surfaces: the
caret you leave in the preview is the cursor you land on in the source, and
back. Colors follow the current [Omarchy] theme and repaint live when it
changes.

Built with [iced]. Text is set in the bundled iA Writer Mono S (SIL OFL,
see [`fonts/OFL.txt`](fonts/OFL.txt)).

## Run

```sh
cargo run                        # empty editor
cargo run -- sample.md           # open a file
cargo run -- sample.md --preview # read-only preview
```

| Argument | Description |
|---|---|
| `FILE` | Markdown file to open (otherwise `Ctrl + O`) |
| `--preview` | Preview-only mode (requires `FILE`); editing is disabled |

## Keys

Global:

| Key | Action |
|---|---|
| `Ctrl + P` | Toggle write / preview |
| `Ctrl + O` / `Ctrl + S` | Open / save |
| `Ctrl + F` | Find (in preview, `/` works too) |
| `Ctrl + B` | Toggle the comments sidebar |
| `Ctrl + N` | Jump to the next comment |
| `Ctrl + ?` | Searchable shortcut reference |

Preview:

| Key | Action |
|---|---|
| `h` `j` `k` `l` / arrows | Move the caret |
| `w` `b` `e` / `ge` | Word motions |
| `0` / `gg` / `G` | Element start / first / last element |
| count + motion (`3j`, `5G`) | Repeat, like vim |
| `Ctrl + D` / `Ctrl + U` | Scroll half a page |
| `PageUp` / `PageDown` | Scroll a full page |
| `zz` / `zt` / `zb` | Scroll caret to center / top / bottom |
| `v` … `y` | Visual-select, then yank to the clipboard |
| `c` | Comment the caret (or the visual selection) |
| `Enter` | Edit the active comment |

In tables, `j`/`k` move between rows of the same column.

## Comments

`c` opens a note popup; saving anchors the comment to the caret position —
or, from visual mode, to exactly the selected text. Comments frame their
text in the preview and appear in the sidebar, where they grow threads,
keep edit history, and can be resolved or deleted. Clicking a card scrolls
its text into view; in write mode the cursor lands on its source instead.

## Development

Built on iced 0.14 and its `markdown` widget. Focused patches to
`iced_core` and `iced_graphics` live under [`vendor/`](vendor/), pinned
with `[patch.crates-io]` — see [vendor/README.md](vendor/README.md) for
the patch surface and upgrade checks.

[iced]: https://iced.rs
[Omarchy]: https://omarchy.org
