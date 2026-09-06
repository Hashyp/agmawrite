# Preview status bar prototype

Branch: `prototype/neovim-preview-statusbar`

## Try it

```sh
cargo run -- sample.md
# Ctrl+P enters preview; click Write (or Ctrl+P) to return.

cargo run -- sample.md --preview
# Preview-only sessions intentionally have no Write control.
```

The status bar appears **only in preview**, including preview-only sessions.
Write mode retains its original toolbar and collapsed comments rail. In preview,
Open / Save / Write replace the toolbar icons, and Comments replaces the sidebar
rail icon. The existing reducers, dialogs, shortcuts and save safeguards remain
in use. The bar spans the whole window, below the comments sidebar.

## Visual reference

Based on this computer's installed LazyVim Lualine configuration:

- `~/.local/share/nvim/lazy/LazyVim/lua/lazyvim/plugins/ui.lua`
- Lualine's default Powerline section separators.
- `nightfox.nvim/lua/nightfox/util/lualine.lua` (current Neovim: nordfox).
- The installed `JetBrainsMono Nerd Font`, at 12 logical pixels.

A 26px strip uses a bold mode block, branch, Markdown file name, a flexible
middle, text actions, progress/location, and a local 24-hour clock. Mode names
are VIEW / VISUAL rather than pretending the application is in Vim NORMAL mode.
Blue and magenta come from the live Omarchy palette; secondary sections blend
30% of the mode color into the dark background, matching Nightfox's Lualine
structure. Geometric Powerline wedges avoid font-dependent separator gaps.
There are no fabricated LSP diagnostics or Git diff counts.

Colors reuse the application's existing Omarchy subscription. No Neovim or
Omarchy configuration changes, hooks, or restarts are required. Each repaint
uses the latest palette, including the wedges, tooltips and action hover colors.

## Prototype boundaries

- The ruler approximates source position using the preview block's source range
  and rendered grapheme column. Markdown delimiters, soft breaks, and fenced
  blocks mean it is not an exact source cursor mapping.
- Progress follows Lualine's **caret-line** convention: Top / percentage / Bot,
  not viewport scroll percentage. Mouse scrolling does not move the caret.
- Git HEAD and local time refresh off the UI thread every two seconds while
  preview is visible. Ordinary repositories, detached HEADs and worktree
  `.git` pointers are supported. Missing/unreadable Git metadata is hidden.
- Long labels truncate; narrower windows hide branch, file, clock and ruler
  segments before the action controls. Full file paths have a tooltip when
  the file segment is visible.
- The local Nerd Font is not bundled. Other machines need that font for the
  exact typography and decorative branch/file/clock glyphs.
- Find and note overlays retain their existing behavior and placement. The bar
  continues naming the underlying preview mode while overlays are open.
- This deliberately follows the new prototype request rather than the older
  `STATUS_BAR_REQUIREMENTS.md` draft (which keeps icons and adds a write-mode bar).

## Validation

```sh
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt --all -- --check
```

Tests cover visibility, mode projection under overlays, toolbar action reuse,
preview-only protection, palette colors, Unicode truncation/ruler behavior,
Git/worktree metadata, and metadata refresh preserving pending motion counts.

Rendered UI was inspected at desktop and narrow-window widths. A running app
with an isolated temporary HOME observed a simulated Omarchy state update and
recolored its bar from Nord blue to amber, then back, without restarting or
changing the real desktop theme.
