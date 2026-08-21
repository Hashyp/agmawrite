# Keymap Refactor Implementation Plan

## Status

Proposed. No production code has been changed.

Baseline validation on the current tree:

- `cargo test --quiet`: passing
- Unit tests: 135 passed
- Documentation tests: 4 passed

## Objective

Replace the boolean-heavy state machine in `src/input/keymap.rs` with nested Rust enums that model the legal interaction states directly. Split UI state, key-sequence parsing, key binding resolution, and the Iced event adapter into separate units.

The refactor should make these states structurally impossible:

- Write mode in a preview-only session.
- Visual mode outside Preview.
- A note composer over Write.
- Duplicate or incorrectly ordered overlays.
- Simultaneously pending `g` and `z` prefixes.
- A pending count of zero.
- Prefix/count state while Note or Find owns input.
- A visible unsaved prompt without the action it is confirming.

## Scope

### In scope

- `src/input/keymap.rs`
- `src/input/guard.rs`
- `src/input/command.rs`
- `src/input/mod.rs`
- `src/app/mod.rs`
- `src/app/tests.rs`
- `src/ui/toolbar.rs`
- The unsaved-confirmation boundary in `src/document/mod.rs`
- Tests affected by the state and event-routing changes

### Out of scope

- Changing existing keyboard shortcuts.
- Changing preview motions, selection behavior, find matching, or comment storage.
- Redesigning the visual appearance of overlays.
- Making key bindings user-configurable.
- General refactors unrelated to interaction state.

## Locked behavioral decisions

Unless changed before implementation, preserve these existing behaviors:

1. Overlay order remains:

   ```text
   Help
   └── Unsaved confirmation
       └── Find
           └── Note
               └── Write or Preview
   ```

2. Find may be opened over Note. Closing Find returns focus to Note.
3. Help may be opened over any state, including Unsaved, and closing it restores the exact suspended state.
4. Help preserves a pending preview count or prefix beneath it.
5. Opening Note, Find, or Unsaved cancels pending preview input.
6. Preview-only sessions can never transition to Write.
7. A loaded document preserves Write versus Preview, exits Visual to View, and closes document-bound Note/Find state.
8. Ordinary typing continues to pass to the focused Write, Note, Find, or Help text field.
9. Modal Escape priority remains Help, Unsaved, Find, Note, then the underlying surface.
10. The badge reflects the top interaction below Help/Unsaved. In particular, Find over Note displays `FIND`, fixing the current disagreement between rendering order and `Keymap::mode()`.

## Target state model

Use nested discriminated unions rather than a flat enum containing every Cartesian combination.

```rust
use std::num::NonZeroU32;

pub(crate) struct InteractionState {
    root: RootState,
}

enum RootState {
    Active(Workspace),
    Unsaved {
        action: UnsavedAction,
        resume: Workspace,
    },
    Help {
        resume: HelpResume,
    },
}

enum HelpResume {
    Active(Workspace),
    Unsaved {
        action: UnsavedAction,
        resume: Workspace,
    },
}

enum Workspace {
    Editable(EditableWorkspace),
    PreviewOnly(PreviewState),
}

enum EditableWorkspace {
    Write(WriteState),
    Preview(PreviewState),
}

enum WriteState {
    Editor,
    Find,
}

enum PreviewState {
    Canvas {
        mode: PreviewMode,
        pending: Pending,
    },
    Note {
        resume: PreviewMode,
    },
    Find {
        resume: FindResume,
    },
}

enum FindResume {
    Canvas(PreviewMode),
    Note(PreviewMode),
}

enum PreviewMode {
    View,
    Visual,
}

enum Pending {
    Idle,
    Count(Count),
    Prefix {
        prefix: Prefix,
        count: Option<Count>,
    },
}

enum Prefix {
    G,
    Z,
}

struct Count(NonZeroU32);
```

The exact visibility and derives may change during implementation, but enum variants should remain private. Callers must transition state through methods so invalid transitions cannot be constructed externally.

### State tree

```text
InteractionState
└── RootState
    ├── Active(Workspace)
    ├── Unsaved { action, resume: Workspace }
    └── Help { resume: Active | Unsaved }

Workspace
├── Editable
│   ├── Write(Editor | Find)
│   └── Preview(PreviewState)
└── PreviewOnly(PreviewState)

PreviewState
├── Canvas(View | Visual, Pending)
├── Note { resume: View | Visual }
└── Find { resume: Canvas | Note }

Pending
├── Idle
├── Count(NonZeroCount)
└── Prefix(G | Z, optional NonZeroCount)
```

## Presentation model

`Mode` should no longer be stored or treated as the complete interaction state. Retain only a derived toolbar projection, renamed to communicate its purpose:

```rust
pub(crate) enum ModeBadge {
    Write,
    View,
    Visual,
    Note,
    Find,
}
```

`InteractionState` should derive the following read-only projections:

- `badge() -> ModeBadge`
- `surface() -> Surface` where `Surface` is `Write | Preview`
- `preview_mode() -> Option<PreviewMode>`
- `can_toggle_preview() -> bool`
- `pending_count() -> Option<NonZeroU32>`
- An overlay/view projection used by `app::view`

Boolean query methods are acceptable as derived compatibility helpers during migration; boolean fields are not.

The toolbar must receive one coherent presentation value instead of independent `preview`, `preview_only`, and `mode` arguments that can disagree.

## Target input pipeline

Use one pure event router for keyboard and input-method policy:

```rust
pub(crate) enum Decision {
    Pass,
    Capture,
    Dispatch(InputMessage),
}

pub(crate) fn route(
    state: &InteractionState,
    event: &iced::Event,
) -> Decision;
```

`InputMessage` should distinguish semantic application commands from sequence-parser changes:

```rust
pub(crate) enum InputMessage {
    Execute(Command),
    ArmPrefix(Prefix),
    PushCountDigit(u8),
}
```

The Iced root guard should become a thin adapter:

1. Call `route` once.
2. Pass the event to the child for `Decision::Pass`.
3. Capture without publishing for `Decision::Capture`.
4. Publish and capture for `Decision::Dispatch`.

After this is in place, remove the separate `keyboard::listen()` subscription. This removes the duplicated modal policy currently spread across `Keymap::handle`, `guard::action`, and `help::keyboard_action`.

## Target file layout

```text
src/input/
├── mod.rs
├── command.rs       # Semantic application commands only
├── state.rs         # InteractionState and UI transition methods
├── pending.rs       # Count, Prefix, Pending, and sequence operations
├── bindings.rs      # Pure event-to-decision routing
└── guard.rs         # Thin Iced Widget adapter
```

If `state.rs` grows beyond roughly 300 production lines, split modal/root transitions into `state/root.rs` and workspace transitions into `state/workspace.rs`. Do not split solely to meet a line-count target.

`src/input/keymap.rs` should be deleted after callers and tests migrate. Do not leave it as a second state representation.

## Transition rules

Implement transitions as exhaustive matches that consume and reconstruct the enum. Unsupported transitions should either be idempotent by explicit policy or return a typed error in tests/debug builds; they must not be silently encoded through unrelated boolean assignments.

### Preview switching

| Current state | Action | Result |
|---|---|---|
| Editable Write(Editor) | Toggle Preview | Editable Preview(Canvas View, Idle) |
| Editable Preview(Canvas/Note) | Toggle Preview | Editable Write(Editor) |
| Editable Write(Find) | Toggle Preview | Editable Preview(Find over Canvas View) |
| Editable Preview(Find) | Toggle Preview | Editable Write(Find) |
| PreviewOnly(any) | Toggle Preview | Unchanged |

Toggling from Preview while Note is present closes Note, matching the current behavior. If Find is above Note, preserve Find while dropping the Note beneath it.

### Visual mode

| Current state | Action | Result |
|---|---|---|
| Preview Canvas(View, pending) | Toggle Visual | Canvas(Visual, Idle) |
| Preview Canvas(Visual, pending) | Toggle Visual | Canvas(View, Idle) |
| Preview Note/Find | Toggle Visual | Update only the suspended preview mode if the action can legitimately originate there; otherwise reject explicitly |
| Write | Toggle Visual | Rejected/unchanged |

### Note

| Current state | Action | Result |
|---|---|---|
| Preview Canvas(mode, pending) | Open Note | Note(resume: mode) |
| Preview Find(Canvas(mode)) | Open Note | Find(Note(mode)) |
| Preview Note or Find(Note) | Open Note | Idempotent |
| Write | Open Note | Rejected |
| Preview Note(mode) | Close Note | Canvas(mode, Idle) |
| Preview Find(Note(mode)) | Close Note | Find(Canvas(mode)) |

### Find

| Current state | Action | Result |
|---|---|---|
| Write Editor | Open Find | Write Find |
| Preview Canvas(mode, pending) | Open Find | Preview Find(Canvas(mode)) |
| Preview Note(mode) | Open Find | Preview Find(Note(mode)) |
| Any Find state | Open Find | Idempotent |
| Write Find | Close Find | Write Editor |
| Preview Find(Canvas(mode)) | Close Find | Canvas(mode, Idle) |
| Preview Find(Note(mode)) | Close Find | Note(mode) |

### Help and Unsaved

- Opening Help wraps `Active` or `Unsaved` in `RootState::Help`.
- Closing Help unwraps exactly one Help layer.
- Opening Unsaved from Active stores both the `UnsavedAction` and suspended workspace.
- If Unsaved is requested while Help is open, update `HelpResume::Active` to `HelpResume::Unsaved`; Help remains topmost.
- Closing/answering Unsaved changes either `RootState::Unsaved` to Active or `HelpResume::Unsaved` to Active.
- Duplicate Help or Unsaved open requests are idempotent.

### Pending input

- `Idle + 1..=9` becomes `Count`.
- A lone `0` remains the preview start motion; it does not construct a count.
- `Count + 0..=9` appends with saturation at `MAX_COUNT`.
- `Idle/Count + g|z` becomes `Prefix`, preserving an existing count.
- Arming one prefix replaces the other.
- A digit after a prefix cancels the prefix and continues/starts the count.
- A completed command consumes the prefix/count.
- Help activity preserves pending input.
- Note, Find, Unsaved, document load, preview toggle, and unrelated activity cancel it according to the existing behavior.

## Unsaved workflow ownership

The current `keymap.unsaved_open` and `document.pending_action()` can disagree. Remove the visibility boolean and carry the confirmed action in `RootState::Unsaved`.

Refactor the document boundary as follows:

1. Replace `document::Event::UnsavedVisibilityChanged(bool)` with `UnsavedConfirmationRequested(UnsavedAction)`.
2. When a modified document needs confirmation, emit the request without separately opening a keymap flag.
3. `InteractionState` stores the action while the prompt is visible.
4. Cancel drops the action and resumes the workspace.
5. Discard passes the extracted action back to the document reducer for execution.
6. Save closes the prompt and gives the action to an explicit document continuation such as `SaveThen(UnsavedAction)`.
7. The document model may retain an `after_save: Option<UnsavedAction>` while asynchronous saving is in progress; this is workflow continuation, not prompt visibility.

This ensures the visible prompt always has an action while allowing the action to continue after the prompt closes for asynchronous save.

## Implementation phases

### [x] Phase 0 — Characterize behavior

Files:

- `src/input/keymap.rs`
- `src/input/guard.rs`
- `src/app/tests.rs`

Tasks:

1. Retain the current passing suite as the baseline.
2. Add characterization tests for:
   - Note → Find → close Find → Note.
   - Help over Unsaved over Find over Note.
   - Preview toggle while Find is open.
   - Pending count/prefix preservation under Help.
   - Pending cancellation under Note, Find, and Unsaved.
3. Add a test specifying that the badge is `FIND` when Find is above Note. This test should initially fail and represents the intentional priority correction.
4. Document any discovered behavior not covered by this plan before modifying production code.

Gate:

- All characterization tests except the deliberately corrected Find badge expectation pass against the old implementation.

### [x] Phase 1 — Replace prefix/count fields

Files:

- Add `src/input/pending.rs`
- Modify `src/input/keymap.rs`
- Modify `src/input/mod.rs`

Tasks:

1. Implement private `Count`, `Prefix`, and `Pending` types.
2. Move saturation, motion count, jump count, digit accumulation, prefix replacement, and consumption into `pending.rs`.
3. Replace `pending_g`, `pending_z`, and `pending_count` with one `Pending` value.
4. Keep the existing external `Keymap` API temporarily to constrain the migration.
5. Convert sequence tests to table-driven tests in `pending.rs`; retain end-to-end binding tests in `keymap.rs`.

Gate:

- No `pending_g` or `pending_z` fields remain.
- A zero `Count` cannot be constructed through the public/module API.
- Existing keyboard behavior tests pass.

### [x] Phase 2 — Introduce the interaction-state enums

Files:

- Add `src/input/state.rs`
- Modify `src/input/keymap.rs`
- Modify `src/input/mod.rs`

Tasks:

1. Implement `RootState`, `HelpResume`, `Workspace`, `EditableWorkspace`, `WriteState`, `PreviewState`, and `FindResume`.
2. Implement constructors for editable and preview-only sessions.
3. Implement exhaustive transition methods for preview, visual, note, find, help, unsaved, and document-loaded events.
4. Implement derived surface, overlay, badge, visual, toggle-capability, and pending-count projections.
5. Temporarily wrap `InteractionState` inside `Keymap` or alias the façade so callers can migrate incrementally.
6. Replace stored `Layer`, modal booleans, and `preview_only` with the enum hierarchy.

Gate:

- `Layer` is removed.
- No `note_open`, `find_open`, `help_open`, `unsaved_open`, or `preview_only` fields remain.
- The Find-over-Note badge test passes.
- State transition tests cover every variant and restoration path.

### [x] Phase 3 — Migrate application ownership

Files:

- `src/app/mod.rs`
- `src/app/tests.rs`
- `src/input/state.rs`
- `src/document/mod.rs`

Tasks:

1. Rename `App.keymap` to `App.interaction`.
2. Replace independent query calls in `view` with one derived interaction/view projection.
3. Replace `input_transition(&Message)` and `Keymap::note` with explicit state transitions at successful feature boundaries.
4. Ensure transitions occur only when the associated feature operation is accepted.
5. Implement the unsaved workflow ownership changes described above.
6. Restore focus from enum payloads rather than a priority chain of `find_open`, `note_open`, and `preview` checks.
7. Update app tests to construct legal state through transition methods, not by setting multiple flags.

Gate:

- `input_transition` and `Transition` are deleted.
- The unsaved view cannot be rendered without an `UnsavedAction` payload.
- No feature visibility is mirrored into a second boolean representation.

### [x] Phase 4 — Separate bindings from state

Files:

- Add `src/input/bindings.rs`
- Modify `src/input/command.rs`
- Modify `src/input/keymap.rs`
- Modify `src/input/mod.rs`

Tasks:

1. Move keyboard matching from `Keymap::handle` into a pure router organized by interaction variant.
2. Split routing into focused helpers such as:
   - `route_help`
   - `route_unsaved`
   - `route_note`
   - `route_find`
   - `route_preview`
   - `route_global`
3. Preserve strict top-down routing through exhaustive `RootState`/workspace matches rather than boolean priority checks.
4. Replace `PreviewCommand::ArmG`, `ArmZ`, and `Count` with internal `InputMessage` sequence events.
5. Remove `preview::Message::AcknowledgeG`, `AcknowledgeZ`, and `AcknowledgeCount` after all callers migrate.
6. Keep `Command` limited to semantic application intent.

Gate:

- No sequence-parser command crosses into the preview feature.
- Bindings are pure functions of state plus event.
- Each interaction variant has focused routing tests.

### [x] Phase 5 — Consolidate event interception

Files:

- `src/input/guard.rs`
- `src/input/bindings.rs`
- `src/app/mod.rs`
- `src/help.rs`

Tasks:

1. Change the root guard to call the single `route` function.
2. Preserve pass-through for focused text widgets and input-method events.
3. Publish routed input messages and capture dispatched shortcuts.
4. Remove `GuardAction` and `message_for_guard_action`.
5. Remove the `keyboard::listen()` subscription and its `Keymap` snapshot.
6. Reduce `help::keyboard_action` to a reusable binding helper or absorb it into the router; do not retain two modal decisions for the same event.
7. Verify held-key behavior and one-shot repeat suppression after moving interception.

Gate:

- Every keyboard event has one routing decision.
- `guard.rs` contains widget adaptation, not modal business rules.
- No duplicate Help/Unsaved/Find/Note priority chain remains.

### [x] Phase 6 — Migrate presentation and remove compatibility code

Files:

- `src/ui/toolbar.rs`
- `src/app/mod.rs`
- `src/input/mod.rs`
- Delete `src/input/keymap.rs`

Tasks:

1. Replace `Mode` with `ModeBadge` or a complete toolbar presentation enum.
2. Make toolbar toggle visibility derive from `Workspace`, not independent booleans.
3. Remove temporary compatibility query methods that merely mimic old fields, unless they remain useful domain projections.
4. Move remaining tests from `keymap.rs` to `state.rs`, `pending.rs`, `bindings.rs`, and app integration tests.
5. Delete `keymap.rs` rather than leaving a façade with no clear responsibility.
6. Update module documentation to describe the new ownership boundaries.

Gate:

- No production symbol named `Layer`, `Keymap`, or `Transition` remains.
- Toolbar inputs cannot disagree about preview mode and preview-only capability.
- Each source file has one clear responsibility.

### [x] Phase 7 — Final validation and cleanup

Tasks:

1. Run formatting, tests, and linting:

   ```bash
   cargo fmt --all -- --check
   cargo test --all-targets
   cargo clippy --all-targets -- -D warnings
   ```

2. Search for removed concepts:

   ```bash
   rg "Layer|Keymap|Transition|note_open|find_open|help_open|unsaved_open|pending_g|pending_z|Acknowledge(G|Z|Count)" src
   ```

3. Manually verify:
   - Write typing and editor shortcuts.
   - Preview motions and held-key repeat.
   - Counts, `gg`, `ge`, `zz`, `zt`, and `zb`.
   - Visual mode entry/exit.
   - Note editing and save shortcut ownership.
   - Find navigation in Write and Preview.
   - Find over Note and restoration after Escape.
   - Unsaved Cancel/Save/Discard.
   - Help over every underlying interaction.
   - Preview-only startup and absent preview toggle.
4. Confirm no unrelated behavior or appearance changed.

Gate:

- Formatting, tests, and Clippy pass.
- Manual interaction matrix passes.
- No old state representation remains.

## Test strategy

### State tests

Use transition tables rather than reproducing application-message plumbing. Cover:

- Every valid open/close restoration pair.
- Idempotent duplicate opens/closes.
- Rejected transitions such as Note from Write.
- Editable versus PreviewOnly behavior.
- Find-over-Note and Help-over-Unsaved nesting.
- Document-loaded normalization.

### Pending parser tests

Cover:

- `3j`, `10k`, `3gg`, `5G`.
- Saturation above `MAX_COUNT`.
- Lone `0` versus `10`.
- Prefix replacement (`g` then `z`, `z` then `g`).
- Digit after prefix.
- Consumption and cancellation.
- No zero count construction.

### Binding tests

Use table-driven cases containing:

- Interaction state.
- Key and modifiers.
- Repeat flag.
- Expected `Pass`, `Capture`, semantic command, or sequence event.

Keep focused tests for modal ownership, auto-repeat, global shortcuts, and text-field pass-through.

### App integration tests

Retain tests for:

- Root visual ordering.
- Focus restoration.
- Preview projection refresh.
- Find routing to source versus preview.
- Unsaved asynchronous save continuation.
- Help preserving the suspended state.

Avoid tests that construct invalid internal states solely to verify a boolean priority chain; the new types should make those constructions impossible.

## Commit strategy

Prefer one reviewable commit per phase:

1. Characterization tests.
2. Pending input union.
3. Interaction-state union.
4. App and unsaved ownership migration.
5. Binding extraction.
6. Single root event router.
7. Toolbar migration and old keymap deletion.
8. Final cleanup and documentation.

Each commit after the initial deliberate red test should compile and pass the full test suite. Do not combine the state-model replacement and event-interception replacement into one commit.

## Completion criteria

The refactor is complete when:

- Legal interaction states are represented by nested enums with payloads.
- Invalid structural combinations cannot be constructed through the state API.
- Modal ordering is encoded by enum nesting rather than boolean priority.
- Count and prefix state is one discriminated union.
- The toolbar mode is a derived presentation value.
- There is one keyboard routing path.
- App updates transition interaction state directly; no observer-style `note()` synchronization remains.
- Unsaved prompt visibility and its action are one state.
- `src/input/keymap.rs` is removed.
- The complete automated and manual validation matrix passes.
