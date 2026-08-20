# `main.rs` Refactoring Implementation Plan

## Purpose

This plan turns the findings in `MAIN_RS_REFACTORING.md` into small, sequential changes that can each be completed in one implementation window. Every task is intended to start and finish with a compiling, tested application. No task should require holding the whole 3,825-line `main.rs` in context.

The plan is based on commit `1839d59` and the current codebase:

- `src/main.rs`: 3,825 lines, including 941 lines of root tests
- `Editor`: 16 fields
- root `Message`: 48 variants
- 94 tests currently pass
- `cargo clippy --all-targets -- -D warnings` currently passes
- Help is the existing example of feature-local state, messages, update logic, view, keyboard policy, and tests

## End state

```text
src/
  main.rs                     # process entry point only
  lib.rs                      # module declarations and public run API
  cli.rs
  watch.rs                    # shared filesystem event predicate

  app/
    mod.rs                    # composition and cross-feature coordination
    shell.rs                  # root layout/layer composition

  document/
    mod.rs                    # State, Message, update, Event
    io.rs                     # open/save dialogs and writes
    watch.rs                  # opened-file subscription
    unsaved_view.rs

  preview/
    mod.rs                    # State, Message, update
    model.rs                  # ElementMap, Caret, motions
    viewer.rs
    scroll.rs
    decorations.rs

  comments/
    mod.rs                    # State, Message, update, Event
    model.rs                  # threads, anchors, cards, store
    sidebar.rs
    composer.rs

  find/
    mod.rs                    # State, Message, update, Event
    model.rs                  # query/current-match model
    view.rs

  input/
    mod.rs
    command.rs                # semantic commands, no app dependency
    keymap.rs
    guard.rs

  ui/
    mod.rs
    icons.rs
    modal.rs
    toolbar.rs

  editing.rs
  help.rs
  highlight.rs
  interactive_text.rs
  theme.rs
```

Final structural targets:

- `src/main.rs` is under 50 lines.
- `src/app/mod.rs` is under 500 lines.
- Root state contains composed feature states instead of document/comment/preview/find fields individually.
- Root messages are nested by feature.
- No feature or input module imports `app::App` or `app::Message`.
- The app interprets semantic feature events for cross-feature behavior.
- Existing behavior and keyboard shortcuts remain unchanged.
- All existing tests remain, but unit tests live with their owning feature and only cross-feature tests remain under `app`.

## Architectural rules for every task

1. **Keep the tree green.** Run the task-specific tests, then run:

   ```sh
   cargo fmt --check
   cargo test
   cargo clippy --all-targets -- -D warnings
   ```

2. **Separate moves from redesign where practical.** A package move should preserve behavior; a following task can change ownership.
3. **No reverse dependency.** Lower-level modules may expose `Message`, `Event`, context, and view functions, but may not import root app state/messages.
4. **Use semantic events across feature seams.** Examples are `NavigateTo(CaretPosition)`, `SourceReplaced`, `CloseWindow`, and `PublishRequested`; do not return root messages.
5. **Map Iced tasks at the boundary.** A feature returns `Task<feature::Message>` and the app uses `.map(Message::Feature)`.
6. **Do not add a generic `Component` trait.** Concrete state/message/update/view APIs are easier to use with Iced lifetimes.
7. **Do not change behavior during extraction.** New features, including comment publishing and stable comment anchors, remain out of scope.
8. **Move tests with ownership.** Do not postpone all test movement until the final task.
9. **Preserve modal ordering.** Help remains above unsaved changes, which remains above find, note, and the editing surface.
10. **Preserve focus restoration.** Closing Help, Find, Note, and unsaved dialogs must restore or retain the same focus behavior as today.

## Feature seam conventions

Use this shape where it fits; not every feature needs every item:

```rust
pub struct State { /* feature-owned data */ }

#[derive(Debug, Clone)]
pub enum Message { /* local widget and async messages */ }

#[derive(Debug, Clone)]
pub enum Event { /* semantic requests for the app */ }

pub struct Update {
    pub task: Task<Message>,
    pub event: Option<Event>,
}

pub fn update(state: &mut State, message: Message, context: Context<'_>) -> Update;
pub fn view(state: &State, context: ViewContext<'_>) -> Element<'_, Message>;
```

Use a simpler return type for features that do not need both tasks and events.

---

# Phase 1: Establish safe boundaries

## Task 1 — Decouple the keymap from root `Message` ✅ Completed

**Goal:** remove the current `use crate::Message` dependency from `src/keymap.rs` without changing key behavior.

**Changes:**

- Add a shared semantic command vocabulary. Initially this may be `src/command.rs`; it will move to `src/input/command.rs` when the input package is assembled.
- Group commands by domain rather than reproducing one flat app enum:

  ```rust
  pub enum Command {
      Document(DocumentCommand),
      Preview(PreviewCommand),
      Comments(CommentsCommand),
      Find(FindCommand),
      Help(HelpCommand),
  }
  ```

- Include all keyboard-produced intents currently returned by `Keymap::handle`, including prefix/count commands.
- Change `Keymap::handle` to return `Option<Command>`.
- Replace `Keymap::note(&Message)` with an input-local transition/event type. It must represent:
  - ordinary unrelated activity, which clears pending `g`, `z`, and counts;
  - `g`, `z`, and count arming;
  - preview, visual, note, find, Help, and unsaved modal transitions;
  - document load resetting transient modes.
- Add one root conversion function from `Command` to the current root `Message`.
- Update the keyboard subscription to map commands at the app boundary.
- Convert keymap tests to assert `Command` values and input-local transitions instead of importing root `Message`.

**Keep in root for now:** the existing flat root `Message` and update branches.

**Acceptance:**

- `rg 'crate::Message' src/keymap.rs` returns no matches.
- Keymap tests still cover all current shortcuts, modal capture, counts, and prefixes.
- Opening/closing Help does not clear a pending preview count or prefix; unrelated commands still do.

## Task 2 — Extract CLI parsing ✅ Completed

**Goal:** isolate argument vocabulary and validation before introducing a library boundary.

**Changes:**

- Add `src/cli.rs` containing `Args`, usage text, parsing, and a parse outcome for run/help.
- Keep parsing pure: no `process::exit` inside the parser.
- Move or add tests for:
  - no arguments;
  - one file;
  - file plus `--preview` in either order;
  - `--preview` without a file;
  - duplicate file arguments;
  - unknown flags;
  - `-h` and `--help`.
- Keep booting and Iced application construction where they are for this task.

**Acceptance:** root no longer defines `Args`, `USAGE`, or `parse_args`.

## Task 3 — Introduce the library/binary boundary ✅ Completed

**Goal:** make the binary a real entry point while preserving the current application as an intermediate state.

**Changes:**

- Move the current application implementation from `src/main.rs` to `src/lib.rs`.
- Replace the old private `main` function with a public `run` API that receives command-line arguments.
- Create a tiny `src/main.rs` that passes `std::env::args().skip(1)` to `agmawrite::run` and preserves current error/help behavior.
- Adjust bundled font paths after the move if required.
- Keep the large application implementation in `lib.rs` only temporarily. This task is packaging, not the architectural completion warned about in the findings.

**Acceptance:**

- `src/main.rs` is already under 50 lines.
- Both `cargo run -- --help` and `cargo run -- sample.md --preview` still take the same paths.
- The 94-test baseline remains green.

---

# Phase 2: Extract the document lifecycle

## Task 4 — Introduce `document::State` ✅ Completed

**Goal:** group document invariants without moving async I/O or update routing yet.

**Changes:**

- Add `src/document/mod.rs` with a `State` owning:
  - source `text_editor::Content`;
  - current path;
  - saved snapshot;
  - pending unsaved action.
- Move `UnsavedAction`, `is_modified`, cursor-preserving external replacement, and cursor clamping into the document module.
- Add narrow accessors required by rendering and cross-feature coordination: `content`, `text`, `path`, `is_modified`, and pending-action inspection.
- Keep mutation methods internal or `pub(crate)`; avoid exposing all fields.
- Replace the four corresponding root fields with `document: document::State`.
- Move these tests from the root into `document`:
  - external replacement preserves/clamps the cursor;
  - modified tracking follows load/save/edit;
  - plain-save snapshot behavior;
  - edits during a save remain modified where this can be tested at the state layer.

**Keep in root for now:** root document messages, async I/O, watcher, and unsaved dialog routing.

**Acceptance:** no root code compares live text to `saved_contents` directly.

## Task 5 — Centralize source replacement and preview projection refresh ✅ Completed

**Goal:** remove the repeated manual synchronization of source, Markdown content, element map, and caret before moving the document reducer.

**Changes:**

- Add one app-level synchronization helper for the temporary architecture.
- Give it explicit reasons/policies, for example:
  - initial/file load: rebuild preview, reset caret/selection, clear document-bound comments;
  - external replacement: rebuild preview, reset/reveal caret, preserve source cursor;
  - entering preview: rebuild preview and place the preview caret from the source cursor.
- Replace all direct repeated assignments to `markdown`, `preview_elements`, and `caret` in load, external-change, toggle-preview, and boot paths.
- Add focused tests proving each policy retains current behavior.

**Note:** this helper is intentionally temporary. Later `preview::State` methods will absorb it. Creating the seam first keeps the document reducer task bounded.

**Acceptance:** search results show one coordinated projection path rather than several assignment clusters.

## Task 6 — Extract document I/O and file watching ✅ Completed

**Goal:** move filesystem/dialog mechanics out of the app without yet moving the lifecycle reducer.

**Changes:**

- Add `src/document/io.rs` for open dialog, save-path dialog, and file writes.
- Add `src/document/watch.rs` for the opened-file subscription and watcher thread.
- Add `src/watch.rs` for the shared `may_change_file` event predicate so theme watching does not need to depend on the document feature.
- Make the watcher emit a document-local event/message; map it at the app boundary.
- Move watcher tests into `document::watch`:
  - access events are ignored;
  - a full one-item channel coalesces rather than disconnecting.

**Acceptance:** root/app code contains no `rfd`, `notify`, `std::fs::write`, or watcher-thread implementation.

## Task 7 — Add the document reducer and nested document message ✅ Completed

**Goal:** make the document feature own edit/open/save/load/external-change/unsaved workflow transitions.

**Changes:**

- Add `document::Message` covering the existing document and unsaved variants:
  - edit;
  - open requested/loaded;
  - save requested/path chosen/saved;
  - external change;
  - close requested;
  - unsaved cancel/save/discard/card press.
- Add `document::Event` for app-owned consequences. Keep it semantic, for example:
  - `SourceReplaced { reason }`;
  - `CloseWindow(id)`;
  - `UnsavedVisibilityChanged(bool)`.
- Add `document::update` returning a local task plus at most the semantic events needed by the app.
- Preserve the exact-snapshot save invariant: a save completion baselines what was written, not the latest live text.
- Preserve guarded-open/guarded-close behavior when edits occur during an in-flight save.
- Replace root document variants with `Message::Document(document::Message)` and delegate in root update.
- Map document tasks and subscriptions with `Message::Document`.
- Move document lifecycle tests from root to `document`; retain only projection/reset effects as app integration tests.

**Acceptance:** root update has one document branch plus semantic event handling, not individual file/save/unsaved branches.

## Task 8 — Extract the unsaved dialog view ✅ Completed

**Goal:** complete document ownership of its modal UI.

**Changes:**

- Add `src/document/unsaved_view.rs`.
- Render warning text from the pending `UnsavedAction`.
- Emit only `document::Message` values.
- Keep reusable backdrop/card/button styling in place temporarily; it will move to `ui::modal` later.
- Replace the root `unsaved_dialog` function with `document::unsaved_view::view(...).map(Message::Document)`.
- Move view-independent keyboard/modal assertions to input tests; keep one app integration test proving the modal sits above the editing surface and below Help.

**Acceptance:** root contains no unsaved-dialog button construction or warning text.

---

# Phase 3: Complete comments extraction

## Task 9 — Turn `comments.rs` into a package without behavior changes ✅ Completed

**Goal:** separate the already-strong domain model from the upcoming controller and views.

**Changes:**

- Move the production model from `src/comments.rs` to `src/comments/model.rs`.
- Add `src/comments/mod.rs` that re-exports only the model types needed outside the feature.
- Move all existing comments model tests with the model.
- Keep names and public behavior stable in this task.

**Acceptance:** this is a move-only change apart from module paths; all comments tests remain unchanged in meaning.

## Task 10 — Add comments feature state, messages, reducer, and events ✅ Completed

**Goal:** move comment workflow state and mutations out of root.

**Changes:**

- Add `comments::State` owning:
  - the comments store/model;
  - note composer `text_editor::Content`;
  - editing target;
  - sidebar visibility override.
- Add local `comments::Message` values for open/close/edit/save composer, edit active comment, activate/delete/resolve cards, edit/add/publish draft, cycle, and toggle sidebar.
- Add update context containing the current preview caret and optional visual selection.
- Add semantic events:
  - `NavigateTo(CaretPosition)`;
  - `FocusComposer`;
  - `PublishRequested`.
- The app handles `NavigateTo` differently in source and preview modes, exactly as today.
- The app handles `FocusComposer` as an Iced focus task.
- Keep `PublishRequested` as a no-op/TODO at the app integration boundary.
- Replace root comment/note/sidebar fields with `comments: comments::State` and root variants with `Message::Comments(comments::Message)`.
- Move reducer-level tests from root: save/dismiss/no-op note, selection anchors, editing/history, thread growth/resolve/delete, and global draft add.

**Acceptance:** root update does not directly call comment model mutation methods.

## Task 11 — Extract the note composer view ✅ Completed

**Goal:** move the comment editing popup and its styles beside comments state.

**Changes:**

- Add `src/comments/composer.rs`.
- Move note popup construction, editor ID, history display, save/close/delete buttons, and comment-specific composer styles.
- Emit only `comments::Message`.
- Keep modal backdrop/card/button primitives temporary or consume `ui::modal` if Task 22 has been intentionally pulled forward.
- Expose a comments-local `focus_composer()` task so the app does not know the editor widget ID.

**Acceptance:** app/root code contains no `NOTE_EDITOR_ID`, note popup widget construction, or direct composer history rendering.

## Task 12 — Extract comments sidebar and collapsed rail views

**Goal:** complete comments UI ownership.

**Changes:**

- Add `src/comments/sidebar.rs`.
- Move the full sidebar, collapsed rail, cards, comment count, publish field, and all sidebar/card/publish styles.
- Pass a `ViewContext` containing source text, preview elements, palette, and font/theme values that are genuinely shared.
- Emit only `comments::Message`.
- Keep `CommentCard` as a model-to-view projection; do not expose thread internals.
- Root decides only where the returned sidebar element is placed.

**Acceptance:** root no longer imports `Mark`, `Span`, or `CommentCard` solely to render comments; comment-specific styles live under `comments/`.

---

# Phase 4: Complete preview extraction

## Task 13 — Turn `preview.rs` into a package without behavior changes

**Goal:** isolate the parsed model/caret code before adding feature/controller code.

**Changes:**

- Move existing parsing, `ElementMap`, `PreviewElement`, `Caret`, motion types, and their tests to `src/preview/model.rs`.
- Add `src/preview/mod.rs` with deliberate re-exports.
- Keep all motion, table navigation, claim protocol, source-position, and selection tests unchanged in behavior.

**Acceptance:** this task changes module paths, not preview behavior.

## Task 14 — Add `preview::State` and preview reducer

**Goal:** group preview data and own preview navigation transitions.

**Changes:**

- Add `preview::State` owning:
  - `markdown::Content`;
  - `ElementMap`;
  - `Caret`;
  - visual anchor.
- Add explicit state operations:
  - initialize/replace source;
  - refresh from source and source cursor when entering preview;
  - place caret for comment/find navigation;
  - clear visual selection.
- Add `preview::Message` for motion, words, jumps, count/prefix acknowledgements, visual toggle/cancel, link click, and scrolling commands.
- Move preview update branches from app into `preview::update`.
- Return semantic events only for app-owned work, such as an unimplemented external link open if retained.
- Replace the four root preview fields with `preview: preview::State` and use `Message::Preview(preview::Message)`.
- Replace the temporary projection helper from Task 5 with these state methods.
- Move navigation/visual reducer tests to the preview feature.

**Acceptance:** app does not mutate `Caret`, `ElementMap`, `markdown::Content`, or visual anchor fields directly.

## Task 15 — Extract preview scroll operations

**Goal:** move Iced widget-tree scrolling mechanics out of app.

**Changes:**

- Add `src/preview/scroll.rs`.
- Move preview/caret widget IDs, `RevealCaret`, `CaretScroll`, `PageScroll`, margins, and task constructors.
- Expose semantic helpers such as `reveal_caret`, `scroll_page`, and `place_caret_in_view` returning `Task<preview::Message>`.
- Keep operation output local (`preview::Message::ScrollBy`).
- Add unit tests around pure distance calculation by extracting viewport/caret delta calculations from the Iced `Operation` implementation.

**Acceptance:** app imports no advanced `Operation`, `Scrollable`, `Outcome`, `Rectangle`, or scroll IDs for preview behavior.

## Task 16 — Extract the preview viewer and preview surface

**Goal:** move rendered preview construction out of app while preserving current decorations.

**Changes:**

- Add `src/preview/viewer.rs` containing `PreviewViewer`, current `Decorations`, Markdown style creation, paragraph/code rendering, and preview scrollable construction.
- Pass a `ViewContext` containing comments decoration data, find data, and palette. These are read-only collaborators, not root app state.
- Emit `preview::Message` for links and any preview-local widget events.
- Keep the current positional `interactive_text` calls temporarily; Task 19 replaces them.
- Root chooses between `document` source view and `preview::view`, but does not build Markdown widgets.

**Acceptance:** root/app contains no implementation of `markdown::Viewer` and no direct call to `markdown::view_with`.

---

# Phase 5: Complete find extraction

## Task 17 — Turn find into a complete feature

**Goal:** move find messages, selection decisions, focus, counter, and popup into `find/`.

**Changes:**

- Move the current query/current-index model to `src/find/model.rs` and retain its tests.
- Add `src/find/mod.rs` with `State`, local `Message`, update, and semantic `Event`.
- Let update receive a read-only surface context:

  ```rust
  enum Surface<'a> {
      Source(&'a str),
      Preview(&'a [PreviewElement]),
  }
  ```

- Return exact navigation events:
  - `SelectSource(SourceMatch)`;
  - `SelectPreview { element, range }`.
- The app applies source selections through the document API and preview selections through the preview API.
- Add `src/find/view.rs` for popup rendering, counter, next/previous buttons, widget ID, and focus task.
- Replace root find variants with `Message::Find(find::Message)`.
- Preserve the source highlighter and preview viewer query access through narrow read-only APIs.
- Move the root find end-to-end test into find reducer tests plus one app test proving source and preview events reach the correct feature.

**Acceptance:** root has no `select_find_match`, `find_match_count`, `find_popup`, or `FIND_INPUT_ID`.

---

# Phase 6: Make preview decorations extensible

## Task 18 — Introduce a generic interactive-text decoration model

**Goal:** remove the long positional argument list from `interactive_text::paragraph` and `interactive_text::code` without changing rendering.

**Changes:**

- Add data types representing the primitives the leaf widget can draw, for example:
  - caret: column, color, optional widget ID;
  - ranged background region: grapheme range and color;
  - whole-element background;
  - gutter/bar: width and color.
- Add `TextDecorations` containing ordered vectors/options of those primitives.
- Replace `selection`, `commented`, `active_comment`, `comment_span`, `CommentColors`, `FindHighlights`, caret/color/id positional parameters with one `TextDecorations` value.
- Preserve layer order exactly:
  1. Markdown span backgrounds;
  2. whole-element comment tint and gutter;
  3. selected comment span;
  4. ordinary find matches;
  5. current find match;
  6. visual selection;
  7. caret;
  8. text.
- Remove both `#[allow(clippy::too_many_arguments)]` attributes.
- Add focused tests for decoration order/model construction and retain geometry helper tests if added.

**Acceptance:** `paragraph` and `code` each accept settings/content plus one decoration model, and rendered behavior is unchanged.

## Task 19 — Add composable decoration producers in preview

**Goal:** prevent future annotations from requiring coordinated changes across viewer and leaf widget signatures.

**Changes:**

- Add `src/preview/decorations.rs`.
- Create independent producers/functions for:
  - caret;
  - visual selection;
  - comments;
  - find matches.
- Each producer appends generic primitives to `TextDecorations`; the viewer only assembles the pipeline.
- Move all decoration colors, including current hard-coded find/selection colors, into producer configuration derived from palette/default policy.
- Ensure code blocks and paragraphs use the same producer pipeline.
- Add tests that construct an element with overlapping comment/find/visual state and assert the ordered primitive list.

**Acceptance:** adding another ranged annotation requires a new producer and one pipeline registration, not changes to `interactive_text` or separate paragraph/code signatures.

---

# Phase 7: Extract remaining infrastructure and shell UI

## Task 20 — Move theme watching into `theme`

**Goal:** make palette loading and palette subscription one cohesive module.

**Changes:**

- Add a theme-local subscription event such as `theme::Event::Changed`.
- Move palette subscription and watcher-thread code into `theme.rs` (or `theme/watch.rs` if the file becomes unwieldy).
- Reuse `watch::may_change_file` from Task 6.
- Map the theme event to a shell/app message at the composition boundary.
- Keep `Palette::current` and all existing palette tests.

**Acceptance:** app subscription assembly calls `theme::subscription().map(...)` and contains no Omarchy state paths or notify code.

## Task 21 — Extract icons

**Goal:** remove canvas drawing noise from app.

**Changes:**

- Add `src/ui/icons.rs` and move `OpenFileIcon`, `SaveIcon`, `PreviewIcon`, `WriteIcon`, and `CommentsIcon`.
- Move icon design constants with them.
- Keep icons generic over message type and unaware of app messages.

**Acceptance:** app contains no `canvas::Program` implementation.

## Task 22 — Extract reusable modal primitives

**Goal:** deduplicate neutral modal styling without creating a global style dumping ground.

**Changes:**

- Add `src/ui/modal.rs` containing only shared backdrop, card, and quiet-button styles/helpers used by Help, comments composer, and unsaved dialog.
- Leave comment sidebar, find-specific, toolbar, mode badge, and feature-specific styles beside their features.
- Migrate modal users one at a time and verify layering/click swallowing remains unchanged.

**Acceptance:** shared modal code is neutral and imports no feature state/message.

## Task 23 — Extract the keyboard guard into the input package

**Goal:** move modal event interception out of app without introducing another reverse dependency.

**Changes:**

- Move `src/keymap.rs` to `src/input/keymap.rs` and the shared command vocabulary to `src/input/command.rs`.
- Add `src/input/guard.rs` containing the custom widget and an input-local `GuardAction`.
- Make the guard generic over the app message mapping, or return/map `GuardAction`; it must not import `app::Message`.
- Preserve priority:
  1. Help open/close/capture;
  2. Help field pass-through;
  3. unsaved input-method/key capture;
  4. Find Escape;
  5. Note Escape;
  6. pass to focused widgets.
- Move keyboard guard tests from app to `input::guard`.

**Acceptance:** app wraps its layers with one input guard call and maps actions; `src/input/` contains no app imports.

## Task 24 — Extract toolbar and mode badge

**Goal:** move bottom controls and their styles out of app.

**Changes:**

- Add `src/ui/toolbar.rs`.
- Define a small toolbar-local message (`Open`, `Save`, `TogglePreview`) and map it in app.
- Pass a read-only toolbar model containing preview/preview-only state, current mode, pending count, and palette.
- Move icon button, tooltip, mode badge, and toolbar-specific styles.
- Use icons from Task 21.

**Acceptance:** app does not construct open/save/preview buttons or format the mode badge.

## Task 25 — Extract root shell layout

**Goal:** leave app view responsible for composition decisions, not layout mechanics.

**Changes:**

- Add `src/app/shell.rs`.
- Move symmetric margins, editor area sizing, toolbar placement, sidebar placement, root background, and stable stack construction into shell helpers.
- Keep feature ordering explicit in `app::view`: base surface, note, find, unsaved, Help, then input guard.
- Pass already-built `Element`s into shell functions; shell must not inspect feature state.

**Acceptance:** shell handles geometry; app view reads as a short composition of feature views and overlays.

---

# Phase 8: Final composition cleanup

## Task 26 — Move the composition root to `app/mod.rs` and finish nested messages

**Goal:** reach the target state/message shape after feature ownership is established.

**Changes:**

- Add `src/app/mod.rs` and move the remaining application state, update, view, subscription, boot, and theme selection out of `lib.rs`.
- Final state should be close to:

  ```rust
  pub struct App {
      document: document::State,
      preview: preview::State,
      comments: comments::State,
      find: find::State,
      help: help::Help,
      keymap: input::Keymap,
      palette: Palette,
  }
  ```

- Final root messages should be grouped, not a new flat list:

  ```rust
  enum Message {
      Document(document::Message),
      Preview(preview::Message),
      Comments(comments::Message),
      Find(find::Message),
      Help(help::Message),
      Input(input::Command),
      Shell(shell::Message),
      Theme(theme::Event),
  }
  ```

  Exact variants may differ, but asynchronous feature messages must remain nested and lower modules must not import this enum.

- Root update should only:
  - update input transitions;
  - delegate to a feature;
  - map its task;
  - interpret semantic events;
  - coordinate cross-feature resets/navigation/focus.
- Keep `lib.rs` to module declarations, public exports as needed, and `run`/application builder wiring.
- Make all feature modules private by default; expose only what the public library API actually requires.

**Acceptance:**

- `src/app/mod.rs` is under 500 lines.
- Root `Message` has only grouped variants.
- No feature imports root `App` or root `Message`.
- `src/lib.rs` is a small library/composition entry, not a renamed old `main.rs`.

## Task 27 — Finish test ownership and add architecture checks

**Goal:** leave tests and module boundaries documenting the new design.

**Move remaining tests:**

| Current root test concern | Final owner |
|---|---|
| file watcher and replacement | `document` |
| list continuation through editor action | `document` integration/unit tests using `editing` |
| modified/save/open/close/unsaved workflow | `document` |
| note save/dismiss/edit/thread/global behavior | `comments` |
| comment model anchoring and cards | `comments::model` |
| preview motion/selection/scroll calculation | `preview` |
| find query/step/counter | `find` |
| key routing/modal capture | `input` |
| Help filtering and keyboard policy | `help` |
| cross-mode comment-card navigation | `app` |
| document load clearing comments/resetting preview | `app` |
| find selection reaching source vs preview | `app` |
| Help preserving underlying source/preview state | `app` |
| overlay ordering/focus restoration | `app` |

**Add architecture checks:**

- A lightweight test or CI shell check rejecting imports of `crate::app` from feature/input modules.
- A check for the agreed line limits on `src/main.rs` and `src/app/mod.rs`.
- A check that both library and binary targets build.

**Final acceptance commands:**

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
rg -n 'crate::app(::|\{).*Message|use crate::app::Message' \
  src/document src/preview src/comments src/find src/input src/help.rs
wc -l src/main.rs src/app/mod.rs
```

Expected final result:

- all behavior tests pass;
- no forbidden dependency matches;
- `main.rs < 50` lines;
- `app/mod.rs < 500` lines.

---

# Dependency/order summary

Tasks should normally be executed in order. The important dependencies are:

```text
1 keymap command seam
  └─> 23 input package/guard

2 CLI
  └─> 3 library boundary
      └─> 26 final app module

4 document state
  └─> 5 projection seam
      └─> 6 document I/O/watch
          └─> 7 document reducer
              └─> 8 unsaved view

9 comments package
  └─> 10 comments reducer
      ├─> 11 composer
      └─> 12 sidebar

13 preview package
  └─> 14 preview state/reducer
      └─> 15 scrolling
          └─> 16 viewer
              └─> 18 generic decoration model
                  └─> 19 decoration producers

17 find feature depends on document and preview state APIs

20–25 depend on the feature messages/views they compose
26 depends on all feature extractions
27 is the final ownership and architecture audit
```

Tasks 20, 21, and 22 are mechanically independent after the library boundary and may be moved earlier if a particular implementation window needs lower-risk work. Do not move Task 26 earlier: relocating the monolith before feature extraction would only rename the architectural problem.

# Explicitly out of scope

- Implementing comment publishing
- Opening Markdown links in the system browser
- Re-anchoring comments to stable source offsets
- Changing find matching semantics
- Changing keyboard shortcuts or mode behavior
- Replacing Iced or introducing a UI framework abstraction
- Introducing a generic component trait
- Visual redesign

Those changes can be implemented after the ownership boundaries in this plan are stable.
