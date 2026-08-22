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

Without `--preview`, `Ctrl + p` toggles between writing and the rendered preview (headings, emphasis, tables, lists, quotes, code blocks, rules). The toggle button carries two icons: an eye while writing (switch to the preview) and a pencil while previewing (switch back to writing) — same shortcut, one button.

A badge in the bottom bar, next to the icons, names the current mode: `WRITE` while editing, `VIEW` while preview-navigating, and `VISUAL` (highlighted in blue) while a selection is anchored.

The whole interface follows the current Omarchy color scheme and re-paints live when `omarchy theme set` changes it.

Opening another file (`Ctrl + O`) or closing the window over unsaved changes asks first: a dialog offers Cancel, Save (write the document, then proceed), and Discard; `Esc` cancels.

In preview, a caret marks the current rendered text element. Navigation supports the arrow keys and vim motions: `h`/`l` (←/→) move the caret one character, crossing to the neighbouring element at its edges; `w`/`b` jump between word starts and `e`/`ge` between word ends; `j`/`k` (↓/↑) move between elements, keeping vim's sticky column; `0` returns to the element's start; `gg`/`G` jump to the first/last element. A count repeats any motion like vim — `3j`, `10k`, `2h`, `3l`, `3w`, `5G`, `3gg` — and shows beside the mode badge while pending. `Ctrl + D`/`Ctrl + U` scroll half a page down/up and PageDown/PageUp a full page; `zz`/`zt`/`zb` scroll the caret to the viewport's middle, top, or bottom. Fenced code blocks are elements like any other: the caret moves through their code and comments can anchor on them. `v` toggles visual mode: while it is active every motion extends the selection from the anchor to the caret, highlighted with the theme's own selection color — the same role write-mode selections use — so it follows the current omarchy theme (text stays readable); `v` or `Esc` leaves it. Inside a table, `j`/`k` move between rows of the same column and exit the table at its top and bottom edges. Press `c` to open a note popup with a text area; save the note as a comment with `Ctrl + S` (or the Save button), close it with `Esc`, the Close button, or a click on the backdrop — closing discards the draft, so the next note starts fresh. In visual mode, `c` comments exactly the selected text: the anchor is the selection, the preview highlights the selected span, and the sidebar card quotes it. Saved comments frame the text they were written for with a subtle amber rectangle — a border only, with no fill of its own, so every background underneath stays as it was; the currently active comment is framed in cyan instead, plus a faint tint over its element, and its card in the sidebar is highlighted the same way. `Ctrl + N` cycles to the next comment, scrolling it into view.

Comments grow threads: another note on the same anchor becomes a reply, indented under the thread's root in the sidebar. Pressing `Enter` over the active comment opens it for editing in the note popup — saving keeps the previous text as history (shown in the popup and counted on the card), and the popup's Delete button removes the comment. Every card also carries a Resolve toggle (✓) and a Delete button (×); resolved threads move to a dimmed RESOLVED history section at the bottom of the sidebar and stop marking elements — reopening restores them.

The sidebar sits flush against the window's right edge, leaving the editor's margins symmetric; when it is hidden (`Ctrl + B`), a slim rail with a comment icon and the comment count marks where it collapsed to — click it to expand. A free text field with Add (`Ctrl + Enter`) and Publish (`Ctrl + Shift + Enter`) buttons sits at the bottom of the panel. Clicking a card activates its comment and brings the commented text into view — the preview scrolls to its rectangle without moving the caret; in write mode the source cursor lands on the element's source. The sidebar appears once a comment exists and is cleared when a file is loaded.

`Ctrl + F` opens a find popup in the top right corner, in any mode. The query matches case-insensitively and selects matches live: in write mode each match becomes the editor's selection and all matches tint amber; in the preview the caret jumps to the match and matches paint as amber highlights with the current one in a brighter shade. A counter shows the current match as `n/m` (or "no match"). Enter and `Ctrl + G` step to the next match, Shift + Enter to the previous one, wrapping around; ▲/▼ buttons do the same by mouse. Escape closes the popup.

Press `Ctrl + ?` at any time to open a centered keyboard shortcuts help window with incremental search: typing filters the list live to the shortcuts and descriptions matching the query (case-insensitive), and the query clears when the window closes so every search starts fresh. It captures input without changing the editor underneath; `Esc` closes it — as does `Ctrl + ?` again — and restores the previous mode, cursor, selection, and popup state.

Built on iced 0.14 and its `markdown` widget. The iA Writer Mono S font is bundled under the SIL Open Font License; see `fonts/OFL.txt`.
