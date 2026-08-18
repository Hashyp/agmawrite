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

In preview, a white caret marks the current rendered text element. Navigation supports the arrow keys and vim motions: `h`/`l` (←/→) move the caret one character, crossing to the neighbouring element at its edges; `w`/`b` jump between word starts and `e`/`ge` between word ends; `j`/`k` (↓/↑) move between elements, keeping vim's sticky column; `gg`/`G` jump to the first/last element. Fenced code blocks are elements like any other: the caret moves through their code and comments can anchor on them. `v` toggles visual mode: while it is active every motion extends the selection from the anchor to the caret, highlighted in translucent blue (text stays readable); `v` or `Esc` leaves it. Inside a table, `j`/`k` move between rows of the same column and exit the table at its top and bottom edges. Press `c` to open a note popup with a text area; save the note as a comment with `Ctrl + S` (or the Save button), close it with `Esc`, the Close button, or a click on the backdrop — closing discards the draft, so the next note starts fresh. Saved comments mark the element they were written for with a subtle amber bar at its left edge; the currently active comment is marked in cyan instead (bar plus a faint tint), and its card in the sidebar is highlighted the same way. `Ctrl + N` cycles to the next comment, jumping the caret there. Saved comments appear in a full-height sidebar on the right: each card quotes the Markdown source of the element the comment was written for (collapsed to one line and trimmed) above the comment text, and a free text field with a Publish button sits at the bottom of the panel. Clicking a card activates its comment and moves the cursor to the element it was written for — the preview caret jumps there; in write mode the source cursor lands on the element's source. The sidebar appears once a comment exists and is cleared when a file is loaded.

`Ctrl + F` opens a find popup in the top right corner, in any mode. The query matches case-insensitively and selects matches live: in write mode each match becomes the editor's selection and all matches tint amber; in the preview the caret jumps to the match and matches paint as amber highlights with the current one in a brighter shade. A counter shows the current match as `n/m` (or "no match"). Enter and `Ctrl + G` step to the next match, Shift + Enter to the previous one, wrapping around; ▲/▼ buttons do the same by mouse. Escape closes the popup.

Built on iced 0.14 and its `markdown` widget. The iA Writer Mono S font is bundled under the SIL Open Font License; see `fonts/OFL.txt`.
