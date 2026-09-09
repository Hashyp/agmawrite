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
- **Mirror** — the rendered↔source alignment that carries the caret
  between surfaces: entering the preview places the caret on the element
  the write cursor rests in, at the rendered column its byte offset
  aligns to; leaving it lands the write cursor on the source character
  that column stands before (`align_boundaries`). Markdown markup is
  skipped as markup, a soft break matches the space it renders as, and
  the preview ruler reports the mirrored position exactly.
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
- **Outline** — the bordered rectangle framing commented text in a
  preview element: exactly the selected span for selection anchors, the
  whole element for spot anchors — a border with no fill, amber when
  merely commented and cyan when active, painted on top of the unchanged
  tints. (`Outline`, via `Comments::outlines_for`)
- **Active comment** — the comment highlighted in the preview and the
  sidebar. `Ctrl+N` cycles it forward through the anchored comments,
  wrapping around; clicking a card activates that comment and scrolls
  its anchor into view — without placing the caret on it.
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
- **Help window** — the shortcuts overlay `Ctrl + ?` toggles (`Ctrl + /`
  is the same chord on US layouts): a modal over the current mode whose
  search field filters the shortcut list live, case-insensitively, over
  both the key and its description. Escape (or the chord again) closes
  it, clearing the query so the next search starts fresh; the mode,
  cursor, and popups underneath resume untouched.
- **Element map** — the numbered preview elements, owned by the preview
  module (`ElementMap`) together with the claim protocol the Markdown
  viewer uses per rendered item — the viewer claims, it never counts.
- **Thread** — a comment and its replies: notes saved on the same anchor
  (a covered spot or the same selection) grow one thread, the tree the
  sidebar renders — root first, replies indented by depth (`Comments`).
- **Resolve** — closes a thread: resolved threads are history — the
  sidebar lists them under RESOLVED, they stop marking elements, and
  cycling skips them; reopening restores everything (`Comments::resolve`).
- **History** — the previous texts of a comment, kept when it is edited
  (`Entry::history`); the edit popup shows them and the card counts them.
- **Count** — the digits pending before a preview motion, like vim's `3`
  of `3j`; it lives in the keymap, repeats the next motion (or page), and
  shows beside the mode badge while pending (`Keymap::pending_count`).
- **Yank** — visual mode's copy verb, vim's `y`: the rendered selection
  (the same slices the selection paints, one line per element) copies to
  the system clipboard and visual mode ends. The span stays behind as a
  brief IncSearch-style flash (`preview::State::yank_flash`, cleared by
  the app's ticker) while the clipboard write and the report cross to the
  application boundary (`preview::Event::Yanked`).
- **Yank report** — the transient message naming what was yanked — Vim's
  own wording, `N characters yanked` over graphemes — carried by the app
  (`App::report`) and shown beside the mode badge until the next
  interaction clears it, like Neovim's cmdline message; mouse scrolling,
  theme changes, and the flash ticker leave it alone.
- **Unsaved dialog** — the modal asking what to do with unsaved changes
  before opening another file or closing the window: Cancel, Save
  (save, then proceed), Discard. The guarded action waits in
  `Editor::pending_unsaved` until the save lands. Keyboard-driven:
  `Esc` cancels back to the editor, `Enter` saves, `j`/`k` move the
  focused button (the accent-outlined one); the focus lives in the
  document (`State::unsaved_focus`) and resets to Save whenever the
  prompt is requested.
- **Marker** — a byte range the write-mode highlighter dims: Markdown's
  block prefixes and inline punctuation, colored as the theme's
  foreground eased toward its background (`highlight::Marker`). Fenced
  code is content — inside a fence only the closing fence marker and
  find matches highlight. The highlighter tracks how far the editor has
  fed it and which fence is open, rewinding on edits
  (`highlight::MarkdownMarkers`).
- **Paragraph gap** — shared write/preview typography (`typography`):
  20px body text, 1.8× line height (36px), and a 1.5× separator (54px).
  Write mode applies it to blank source lines (empty or spaces/tabs)
  outside fenced code through Iced's opt-in line-height hook; shaped heights
  drive caret, selection, and visibility geometry (`vendor/README.md`).
  Preview applies the same content-to-content gap between rendered blocks,
  counting decoration insets inside the gap, not on top. Tight lists and
  code interiors do not receive paragraph gaps; heading sizes remain semantic.
  Preview follows Markdown's blank-line collapsing, while consecutive,
  leading, and trailing source blanks keep individual heights in write mode.
  The source text is unchanged.
- **Palette** — the Omarchy color scheme the interface paints with,
  loaded from the current theme and reloaded when it changes
  (`theme::Palette`).
- **Watch** — the opened file's directory is watched; external changes
  reload the document immediately in any mode. The Omarchy theme state is
  watched the same way, re-painting the interface on theme switches.
