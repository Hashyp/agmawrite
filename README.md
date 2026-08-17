# agmawrite

A deliberately minimal Markdown editor: black canvas, blinking cursor, a writing area with roughly 10% top and side margins, generous line spacing, and a folder icon for opening Markdown files. Text uses the bundled iA Writer Mono S font.

## Run

```sh
cargo run
```

Or with command-line arguments:

```sh
cargo run -- sample.md --preview
```

| Argument | Description |
|---|---|
| `FILE` | Markdown file to open (optional; otherwise use `Ctrl + o`) |
| `--preview` | Open `FILE` in preview-only mode — editing and switching back to write mode are disabled |

Without `--preview`, `Ctrl + p` or the eye icon toggles between writing and the rendered preview (headings, emphasis, tables, lists, quotes, code blocks, rules).

A badge in the bottom bar, next to the icons, names the current mode: `WRITE` while editing, `VIEW` while preview-navigating, and `VISUAL` (highlighted in selection blue) while a selection is anchored.

In preview, hovering a word shows a navy outline; hovering paragraph whitespace or padding shows a green outline. Click either target to keep a red outline for the rest of the session. A white caret marks the current rendered text element. Navigation supports the arrow keys and vim motions: `h`/`l` (←/→) move the caret one character, crossing to the neighbouring element at its edges; `w`/`b` jump between word starts and `e`/`ge` between word ends; `j`/`k` (↓/↑) move between elements, keeping vim's sticky column; `gg`/`G` jump to the first/last element. `v` toggles visual mode: while it is active every motion extends the selection from the anchor to the caret, highlighted in translucent blue (text stays readable); `v` or `Esc` leaves it. Inside a table, `j`/`k` move between rows of the same column and exit the table at its top and bottom edges. Press `c` to open a note popup with a text area; save the note as a comment with `Ctrl + S` (or the Save button), close it with `Esc`, the Close button, or a click on the backdrop. Saved comments appear in a sidebar on the right: each card quotes the Markdown source of the element the comment was written for (collapsed to one line and trimmed) above the comment text. The sidebar takes over the right margin once a comment exists; it is cleared when a file is loaded.

Built on iced 0.14 and its `markdown` widget. The iA Writer Mono S font is bundled under the SIL Open Font License; see `fonts/OFL.txt`.
