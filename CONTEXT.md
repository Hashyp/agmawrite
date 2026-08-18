# Context

Domain language for agmawrite — a minimal Markdown editor with a vim-style
preview and anchored comments.

## Terms

- **Preview element** — a Markdown block the preview numbers as a caret
  position: headings, paragraphs, quotes, list items, non-empty table
  cells, and fenced code blocks. Images and rules produce no element.
  (`PreviewElement`)
- **Caret position** — an `(element, column)` position in the preview: the
  element index plus a grapheme column within its rendered text. Where the
  preview caret sits. (`CaretPosition`)
- **Comment** — a saved note: its text plus the anchor it was written for.
- **Anchor** — what a comment is attached to: a caret position, or nothing
  for a **global comment** (labeled "Global", skipped by `Ctrl+N`). The
  stored representation is private to the comments module; it crosses the
  module seam only as a caret position, so re-anchoring (element indices
  rot when the preview re-parses after an edit) can change inside the
  module without touching the editor.
- **Mark** — whether a preview element carries a comment: `None`,
  `Commented`, or `Active` — the element the currently active comment is
  anchored to. (`Mark`, via `Comments::mark_for`)
- **Active comment** — the comment highlighted in the preview and the
  sidebar. `Ctrl+N` cycles it forward through the anchored comments,
  wrapping around; clicking a card activates that comment and moves the
  cursor to its anchor.
- **Note popup** — the modal text field over the preview whose text becomes
  a comment on save. Dismissing it (Escape, the Close button, a backdrop
  click) discards the draft, so the next note starts fresh.
- **Find popup** — the small search field in the top right corner
  (`Ctrl+F`). The query matches case-insensitively; each keystroke
  restarts at the first match, and the counter reads `n/m` (or "no
  match"). Enter, `Ctrl+G`, and the ▼ button step to the next match,
  Shift+Enter and ▲ to the previous one, wrapping around. Every match
  highlights amber with the current one brighter; in write mode the
  current match is the editor's selection and the others tint amber
  through the syntax highlighter. The query and the current-match index
  live in the find module (`Find`).
- **Publish draft** — the text of the sidebar publish field, owned by the
  comments module (`Comments::draft`). The Add button files it as a global
  comment.
- **Keymap** — the input mode stack (write, view, visual, note, find) plus
  the pending `g` sequence, owned by the keymap module. Key handling and
  the mode badge read it, so they agree by construction.
- **Element map** — the numbered preview elements, owned by the preview
  module (`ElementMap`) together with the claim protocol the Markdown
  viewer uses per rendered item — the viewer claims, it never counts.
- **Watch** — the opened file's directory is watched; external changes
  reload the document immediately in any mode.
