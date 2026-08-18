# Context

Domain language for agmawrite — a minimal Markdown editor with a vim-style
preview and anchored comments.

## Terms

- **Preview element** — a Markdown block the preview numbers as a caret
  position: headings, paragraphs, quotes, list items, and non-empty table
  cells. Code blocks, images, and rules produce no element.
  (`PreviewElement`)
- **Caret position** — an `(element, column)` position in the preview: the
  element index plus a grapheme column within its rendered text. Where the
  preview caret sits. (`CaretPosition`)
- **Comment** — a saved note: its text plus the anchor it was written for.
- **Anchor** — the caret position a comment is attached to. Its stored
  representation is private to the comments module; it crosses the module
  seam only as a caret position, so re-anchoring (element indices rot when
  the preview re-parses after an edit) can change inside the module without
  touching the editor.
- **Mark** — whether a preview element carries a comment: `None`,
  `Commented`, or `Active` — the element the currently active comment is
  anchored to. (`Mark`, via `Comments::mark_for`)
- **Active comment** — the comment highlighted in the preview and the
  sidebar. `Ctrl+N` cycles it forward, wrapping around.
- **Note popup** — the modal text field over the preview whose text becomes
  a comment on save.
- **Publish draft** — the text of the sidebar publish field, owned by the
  comments module (`Comments::draft`).
