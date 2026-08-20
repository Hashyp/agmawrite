# `main.rs` Refactoring Findings

## Assessment

At commit `1839d59` (`Extract Help window into its own module`), `src/main.rs` is **3,825 lines**, split exactly at `#[cfg(test)]` into:

- **2,884 production lines** (`src/main.rs:1-2884`)
- **941 test-section lines** (`src/main.rs:2885-3825`), containing 23 tests

The complete `src/` tree is **9,526 lines**: 6,348 production-section lines and 3,178 test-section lines. `Editor` still has 16 fields and the flat root `Message` has 48 variants.

`main.rs` currently serves as:

- Application entry point and CLI parser
- Root state container
- Global message vocabulary
- Update/controller for the document, preview, comments, and find features
- Root coordination for the extracted Help feature
- Document persistence and file watching
- Unsaved-changes workflow
- Keyboard and modal routing
- Source editor behavior
- Preview rendering and scrolling
- Find UI and controller
- Comments UI and controller
- Toolbar, icons, dialogs, and styling
- Integration-test container

The main architectural problem is not only file size. Adding a feature currently tends to modify `Editor`, `Message`, `update`, `view`, `boot`, `keymap`, subscriptions, and test fixtures. This is the kind of shotgun surgery the Open/Closed Principle should prevent.

## Completed refactoring: Help feature extraction

The Help functionality has already been extracted successfully:

- Before extraction, `src/main.rs` was **4,103 lines**.
- It is now **3,825 lines**, a net reduction of **278 lines**.
- `src/help.rs` is **407 lines**: 336 production lines and a 71-line test section with four tests.
- Help now owns its query state and reducer (`src/help.rs:17-43`), local message type (`src/help.rs:46-51`), keyboard policy (`src/help.rs:54-119`), shortcut vocabulary and filtering (`src/help.rs:124-235`), complete view (`src/help.rs:238-334`), and unit tests (`src/help.rs:337-407`).

This is the strongest existing example of the desired feature shape. The root now retains only legitimate integration responsibilities plus some remaining coupling:

- Help state in `Editor`: `src/main.rs:75`
- Root wrapper messages: `src/main.rs:137-138`
- Focus restoration and delegation: `src/main.rs:928-947`
- Root keyboard-guard integration: `src/main.rs:1107-1314`
- Overlay composition: `src/main.rs:2015-2017`
- Cross-feature integration tests: `src/main.rs:2927-3055`

The extraction is therefore complete at the feature level, but `keymap.rs` still depends on both `help` and the root `Message`. The later keymap recommendation remains relevant.

## Highest-value refactorings

### 1. Introduce feature-level state, messages, updates, and views

The flat `Editor` and `Message` definitions at `src/main.rs:49-149` are the main coupling point.

Use nested feature messages:

```rust
enum Message {
    Document(document::Message),
    Preview(preview::Message),
    Comments(comments::Message),
    Find(find::Message),
    Help(help::Message),
    Shell(shell::Message),
}
```

Each feature should conventionally expose:

```rust
pub struct State { /* ... */ }
pub enum Message { /* ... */ }

pub fn update(state: &mut State, message: Message) -> Update;
pub fn view(state: &State, context: ViewContext<'_>) -> Element<'_, Message>;
```

The root `update` should only delegate messages and interpret cross-feature effects.

A generic `Component` trait is not necessary initially. Iced's element lifetimes and task mapping would add complexity without much benefit.

### 2. Extract the document lifecycle as one cohesive feature

Relevant code is currently scattered across:

- File dialogs and saving: `src/main.rs:387-420`
- Root subscription assembly: `src/main.rs:422-435`
- File watching: `src/main.rs:498-586`
- Source replacement and dirty tracking: `src/main.rs:590-651`
- Source editing, document, and unsaved-workflow `update` branches: `src/main.rs:661-767`
- Unsaved dialog: `src/main.rs:2644-2713`
- Document/file integration tests: approximately `src/main.rs:3267-3666`

A `document` feature should own:

```rust
struct State {
    content: text_editor::Content,
    path: Option<PathBuf>,
    saved_contents: String,
    pending_unsaved: Option<UnsavedAction>,
}
```

And messages such as:

```rust
enum Message {
    Edit(text_editor::Action),
    OpenRequested,
    Loaded(/* ... */),
    SaveRequested,
    SavePathChosen(/* ... */),
    Saved(/* ... */),
    ChangedExternally,
    CloseRequested(/* ... */),
    UnsavedAnswered(UnsavedAnswer),
}
```

This gives one location for invariants such as:

- A saved snapshot, rather than current live text, becomes the baseline.
- Saving while typing does not discard newer edits.
- Loading and external replacement preserve valid cursor positions.
- Opening and closing are guarded by the same unsaved workflow.

It also removes concrete filesystem and dialog concerns from the root.

A related issue is repeated projection rebuilding. `markdown`, `preview_elements`, and source content are manually synchronized in several `update` branches. Introduce operations such as:

```rust
document.load(path, contents);
preview.replace_source(contents);
preview.refresh_from_source(contents);
```

This prevents a future derived representation from requiring modifications in every loading path.

### 3. Complete the comments feature extraction

`src/comments.rs` already contains a strong domain model, but the feature remains split across `main.rs`.

Comment-specific root state includes:

- `comments`
- `note_text`
- `editing_comment`
- `sidebar_override`

Comment behavior remains in:

- Comment and note `update` branches: `src/main.rs:854-909` and `src/main.rs:947-973`
- Note persistence helper: `src/main.rs:1369-1386`

Comment UI remains in:

- Sidebar and rail: `src/main.rs:2029-2211`
- Cards and styles: `src/main.rs:2213-2478`
- Note popup: `src/main.rs:2539-2642`

Recommended structure:

```text
src/comments/
  mod.rs
  model.rs       # Current Comments, Thread, Anchor, CommentCard
  feature.rs     # State, Message, update
  sidebar.rs     # Sidebar and cards
  composer.rs    # Note popup and editing workflow
```

For cross-feature actions, return semantic effects instead of importing the root message:

```rust
pub enum Event {
    NavigateTo(CaretPosition),
    Reveal(CaretPosition),
    PublishRequested(CommentBatch),
}
```

The app decides whether `NavigateTo` moves the preview caret or source cursor.

This is especially useful because `PublishPressed` is currently a TODO. A publisher can later be installed without rewriting comment storage and UI.

### 4. Complete the preview feature extraction

`src/preview.rs` owns parsing and caret movement, but `main.rs` still owns most of the actual preview feature:

- Preview update branches: `src/main.rs:768-853`
- Scrolling operations: `src/main.rs:1394-1541`
- `PreviewViewer` and decoration calculation: `src/main.rs:1601-1787`
- Preview portion of root view: approximately `src/main.rs:1789-1846`

Recommended structure:

```text
src/preview/
  mod.rs
  model.rs         # ElementMap, Caret, motions
  feature.rs       # State, Message, update
  viewer.rs        # PreviewViewer
  scroll.rs        # RevealCaret, PageScroll
  decorations.rs
```

Preview state should group:

```rust
struct State {
    markdown: markdown::Content,
    elements: ElementMap,
    caret: Caret,
    visual_anchor: Option<CaretPosition>,
}
```

Adding a preview command would then modify the preview feature instead of root `Editor`, root `Message`, root `update`, and root view separately.

### 5. Replace hard-coded preview decoration parameters with a decoration model

This is a second important Open/Closed axis.

`PreviewViewer::decorations` at `src/main.rs:1695-1732` hard-codes:

- Visual selection
- Caret
- Comment marks
- Comment span
- Find matches

Meanwhile, `interactive_text::paragraph` and `code` have enough positional arguments to require `#[allow(clippy::too_many_arguments)]` at `src/interactive_text.rs:54` and `src/interactive_text.rs:91`.

Replace those arguments with a model such as:

```rust
pub struct TextDecorations {
    pub caret: Option<CaretDecoration>,
    pub regions: Vec<RegionDecoration>,
    pub gutters: Vec<GutterDecoration>,
    pub widget_id: Option<Id>,
}
```

Possible producers include:

- `SelectionDecorator`
- `CommentDecorator`
- `FindDecorator`
- A future spell-check or diagnostics decorator

A new visual annotation could then extend the decoration pipeline instead of modifying `PreviewViewer`, `Decorations`, `InteractiveText`, `paragraph`, `code`, and all their call sites.

### 6. Complete the find feature extraction

`src/find.rs` owns only search state and match enumeration. Root still owns:

- Find update branches and focus behavior: `src/main.rs:910-925`
- Selecting matches: `src/main.rs:984-1024`
- Counting matches: `src/main.rs:1028-1038`
- Popup rendering: `src/main.rs:1043-1104`

Move those responsibilities into `find::feature` and `find::view`.

The find feature can emit:

```rust
enum Event {
    SelectSource(SourceMatch),
    SelectPreview {
        element: usize,
        range: Range<usize>,
    },
}
```

This keeps the feature independent of root application state.

### 7. Decouple `keymap.rs` from the root `Message`

This is the most obvious dependency-direction violation:

```rust
use crate::Message;
```

at `src/keymap.rs:16`.

`Keymap::handle` constructs application messages, while `Keymap::note` inspects root messages to infer transitions. Consequently, adding an unrelated feature or modal can require modifying `keymap.rs`.

Instead, introduce semantic input commands:

```rust
pub enum Command {
    Document(DocumentCommand),
    Preview(PreviewCommand),
    Comments(CommentsCommand),
    Find(FindCommand),
    Help(HelpCommand),
}
```

Then:

```rust
pub fn handle(&self, event: keyboard::Event) -> Option<Command>;
```

The root maps commands into feature messages. Mode transitions should be explicit commands or methods rather than inferred by inspecting arbitrary application messages.

The dependency direction should be:

```text
main/app -> keymap
main/app -> features
keymap    -> shared command vocabulary
```

Never:

```text
keymap -> app::Message
```

The extracted `help.rs` is the existing example of the desired feature shape: it owns its state, message, update, keyboard policy, focus task, view, and local unit tests. However, `keymap.rs:14-16`, `Keymap::handle` at `src/keymap.rs:172-373`, and `Keymap::note` at `src/keymap.rs:378-469` still tie input routing to root application messages.

## Lower-risk extractions

These make `main.rs` smaller but provide less architectural benefit.

### UI icons

The canvas icons occupy `src/main.rs:151-385`. Move them to:

```text
src/ui/icons.rs
```

### Keyboard guard

Move `KeyboardGuardAction`, `keyboard_guard_action`, and the custom widget from `src/main.rs:1107-1314` into:

```text
src/input/guard.rs
```

### Theme subscription

Palette parsing is in `theme.rs`, but palette watching is in `src/main.rs:439-498`. `theme::subscription()` should return a feature-local event that the app maps to its message.

### Toolbar and shell layout

The root `view` at `src/main.rs:1789-2021` should compose smaller views:

```rust
shell::view(
    editor_surface,
    toolbar::view(/* ... */),
    comments::view(/* ... */),
    overlays,
)
```

Keep styles beside the component that uses them rather than creating one large `styles.rs` dumping ground.

### CLI and binary entry point

Move `Args`, parsing, usage, and boot configuration from `src/main.rs:2758-2847` into `cli.rs`.

Introduce `src/lib.rs`, then reduce `main.rs` to approximately:

```rust
fn main() -> iced::Result {
    agmawrite::run(std::env::args().skip(1))
}
```

This makes `main.rs` a true composition entry point rather than the application implementation.

## Suggested target layout

```text
src/
  main.rs
  lib.rs
  cli.rs

  app/
    mod.rs             # App composition and cross-feature coordination
    shell.rs

  document/
    mod.rs
    io.rs
    watch.rs
    unsaved_view.rs

  preview/
    mod.rs
    model.rs
    feature.rs
    viewer.rs
    scroll.rs
    decorations.rs

  comments/
    mod.rs
    model.rs
    feature.rs
    sidebar.rs
    composer.rs

  find/
    mod.rs
    feature.rs
    view.rs

  input/
    mod.rs
    keymap.rs
    guard.rs

  ui/
    icons.rs
    modal.rs
    toolbar.rs

  editing.rs
  help.rs
  highlight.rs
  interactive_text.rs
  theme.rs
```

The resulting `app/mod.rs` should contain only:

- Composed `App` state
- Nested top-level message routing
- Cross-feature event handling
- Root layer composition
- Application builder

A reasonable target is **under 500 lines for `app/mod.rs` and under 50 lines for `main.rs`**.

## Recommended migration order

1. Decouple `keymap` from root `Message`.
2. Introduce `lib.rs` and move CLI/application boot out of `main.rs`.
3. Extract document lifecycle and the unsaved workflow.
4. Extract comments state, controller, and views.
5. Extract preview controller, view, and scrolling.
6. Extract find controller and view.
7. Introduce the extensible decoration model.
8. Move icons, toolbar, modal primitives, and keyboard guard.
9. Move the remaining 941-line test section beside its owning features, following Help's example, while retaining only true cross-feature tests under `app`.

## Important constraint

Simply renaming `main.rs` to `app.rs` would reduce the visible problem but not the responsibility or Open/Closed problem.

The important change is ensuring that lower-level features no longer import or inspect the root application's state and messages. The composition root may change when a new feature is installed, but existing feature implementations should remain unchanged.
