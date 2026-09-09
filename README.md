# agmawrite

A deliberately minimal Markdown editor: black canvas, blinking cursor, a writing area with roughly 10% top and side margins, generous line spacing, and `Ctrl + O` for opening Markdown files. Text uses the bundled iA Writer Mono S font.

While writing, the Markdown syntax itself stays quiet: every marker — heading hashes, list bullets and checkboxes, ordered numbers, blockquote bars, code fences and thematic breaks, but also the inline punctuation of emphasis (`*`/`_`), strikethroughs (`~~`), code spans, and links or images — renders dimmer than the prose, its color the theme's foreground eased toward the background, so the words stand out and the syntax fades. Both write and preview use the same generous line spacing and paragraph gap: at the 20px body size, prose lines are 36px tall and paragraph separators occupy 54px. Write mode expands blank source lines outside fenced code (including spaces/tabs); preview applies that gap between rendered blocks while retaining heading sizes and keeping tight lists and code interiors compact. Preview follows Markdown's normal blank-line collapsing; consecutive source blanks retain their individual height in write mode. No extra newlines are inserted into the document. Code inside fenced blocks is left alone — it is content, not syntax.

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

Without `--preview`, `Ctrl + p` toggles between writing and the rendered preview (headings, emphasis, tables, lists, quotes, code blocks, rules).

A text-only status bar spans the bottom of the window on both surfaces, LazyVim-style: a mode badge — `WRITE` (the theme's red) while editing, `VIEW` (blue) while preview-navigating, and `VISUAL` (magenta) while a selection is anchored — then the git branch and file name (with a `[+]` marker while modified), and on the right the `Preview`/`Write` surface switch (`Ctrl + P`) beside a ruler (`Top`/`n%`/`Bot` progress plus `row:column`) that follows the source cursor in write mode and the caret's source position in preview. While writing, the bar ends at the ruler: comments are a preview feature there. In preview it also carries the `Comments` toggle for the sidebar.

In editable sessions, two document controls float as icons in the window's top-left corner: open (a folder) and save (a floppy disk). Save is disabled — dimmed and clickless — while the document has no unsaved changes. When the app is run in preview mode (`--preview`) the corner controls are absent. The `Ctrl + O`/`Ctrl + S` shortcuts keep working everywhere.

The whole interface follows the current Omarchy color scheme and re-paints live when `omarchy theme set` changes it.

Opening another file (`Ctrl + O`) or closing the window over unsaved changes asks first: a dialog offers Cancel, Save (write the document, then proceed), and Discard. The dialog is fully keyboard-driven: `Esc` returns to the editor, `Enter` saves, and `j`/`k` move the accent-outlined focus between the buttons (which stay clickable).

In preview, a caret marks the current rendered text element. Navigation supports the arrow keys and vim motions: `h`/`l` (←/→) move the caret one character, crossing to the neighbouring element at its edges; `w`/`b` jump between word starts and `e`/`ge` between word ends; `j`/`k` (↓/↑) move between elements, keeping vim's sticky column; `0` returns to the element's start; `gg`/`G` jump to the first/last element. A count repeats any motion like vim — `3j`, `10k`, `2h`, `3l`, `3w`, `5G`, `3gg` — and shows beside the mode badge while pending. `Ctrl + D`/`Ctrl + U` scroll half a page down/up and PageDown/PageUp a full page; `zz`/`zt`/`zb` scroll the caret to the viewport's middle, top, or bottom. Fenced code blocks are elements like any other: the caret moves through their code and comments can anchor on them. `v` toggles visual mode: while it is active every motion extends the selection from the anchor to the caret, highlighted with the theme's own selection color — the same role write-mode selections use — so it follows the current omarchy theme (text stays readable); `v` or `Esc` leaves it. Press `y` to yank the selection, like vim: the rendered text copies to the system clipboard, visual mode ends, the yanked span flashes briefly in an IncSearch-style highlight — the idea behind Neovim's `vim.hl.on_yank` — and the status bar reports the count (`3 characters yanked`, Vim's own wording) until the next key press, like Neovim's message area. Inside a table, `j`/`k` move between rows of the same column and exit the table at its top and bottom edges. Press `c` to open a note popup with a text area; save the note as a comment with `Ctrl + S` (or the Save button), close it with `Esc`, the Close button, or a click on the backdrop — closing discards the draft, so the next note starts fresh. In visual mode, `c` comments exactly the selected text: the anchor is the selection, the preview highlights the selected span, and the sidebar card quotes it. Saved comments frame the text they were written for with a subtle amber rectangle — a border only, with no fill of its own, so every background underneath stays as it was; the currently active comment is framed in cyan instead, plus a faint tint over its element, and its card in the sidebar is highlighted the same way. `Ctrl + N` cycles to the next comment, scrolling it into view.

Comments grow threads: another note on the same anchor becomes a reply, indented under the thread's root in the sidebar. Pressing `Enter` over the active comment opens it for editing in the note popup — saving keeps the previous text as history (shown in the popup and counted on the card), and the popup's Delete button removes the comment. Every card also carries a Resolve toggle (✓) and a Delete button (×); resolved threads move to a dimmed RESOLVED history section at the bottom of the sidebar and stop marking elements — reopening restores them.

The sidebar sits flush against the window's right edge, leaving the editor's margins symmetric. It is a preview-surface feature: while hidden (`Ctrl + B`), a slim rail with a comment icon and the comment count marks where it collapsed to on the preview — click it to expand. Write mode keeps its right edge clean (`Ctrl + B` still toggles the sidebar from either surface). A free text field with Add (`Ctrl + Enter`) and Publish (`Ctrl + Shift + Enter`) buttons sits at the bottom of the panel. Clicking a card activates its comment and brings the commented text into view — the preview scrolls to its rectangle without moving the caret; in write mode the source cursor lands on the element's source. The sidebar appears once a comment exists and is cleared when a file is loaded.

`Ctrl + F` opens a find popup in the top right corner, in any mode. In preview (view or visual mode), `/` opens the same search; in text fields it remains a normal character. The query matches case-insensitively and selects matches live: in write mode each match becomes the editor's selection and all matches tint amber; in the preview the caret jumps to the match and matches paint as amber highlights with the current one in a brighter shade. A counter shows the current match as `n/m` (or "no match"). Enter and `Ctrl + G` step to the next match, Shift + Enter to the previous one, wrapping around; ▲/▼ buttons do the same by mouse. Escape closes the popup.

Press `Ctrl + ?` at any time to open a centered keyboard shortcuts help window with incremental search: typing filters the list live to the shortcuts and descriptions matching the query (case-insensitive), and the query clears when the window closes so every search starts fresh. It captures input without changing the editor underneath; `Esc` closes it — as does `Ctrl + ?` again — and restores the previous mode, cursor, selection, and popup state.

Built on iced 0.14 and its `markdown` widget. The iA Writer Mono S font is bundled under the SIL Open Font License; see `fonts/OFL.txt`. Focused patches to `iced_core` and `iced_graphics` 0.14.0 live under `vendor/`, pinned with `[patch.crates-io]`: an opt-in line-height hook and matching caret/selection geometry. Markdown spacing policy stays in the app, not the renderer. See [vendor/README.md](vendor/README.md) for the patch surface and upgrade checks.
