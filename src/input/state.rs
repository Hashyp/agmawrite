//! Legal interaction states and their transitions.

use std::num::NonZeroU32;

use super::pending::{Pending, Prefix};
use crate::document::UnsavedAction;

mod root;
mod workspace;

/// The complete interaction state. Its variants are private so callers can
/// only move between legal states through transition methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct InteractionState {
    root: RootState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HelpResume {
    Active(Workspace),
    Unsaved {
        action: UnsavedAction,
        resume: Workspace,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Workspace {
    Editable(EditableWorkspace),
    PreviewOnly(PreviewState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EditableWorkspace {
    Write(WriteState),
    Preview(PreviewState),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WriteState {
    Editor,
    Find,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PreviewState {
    Canvas { mode: PreviewMode, pending: Pending },
    Note { resume: PreviewMode },
    Find { resume: FindResume },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FindResume {
    Canvas(PreviewMode),
    Note(PreviewMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum PreviewMode {
    View,
    Visual,
}

/// The mode shown in the toolbar badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ModeBadge {
    Write,
    View,
    Visual,
    Note,
    Find,
}

/// The document surface rendered below any overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Surface {
    Write,
    Preview,
}

/// An overlay in bottom-to-top rendering order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Overlay {
    Note,
    Find,
    Unsaved(UnsavedAction),
    Help,
}

/// The widget that should own keyboard focus in the current hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FocusTarget {
    SourceEditor,
    NoteComposer,
    FindInput,
    HelpInput,
}

/// A resolved unsaved prompt and the focus encoded by its resumed state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnsavedResolution {
    action: UnsavedAction,
    focus: Option<FocusTarget>,
}

/// A coherent, derived view of the interaction hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewProjection {
    surface: Surface,
    badge: ModeBadge,
    preview_mode: Option<PreviewMode>,
    can_toggle_preview: bool,
    pending_count: Option<NonZeroU32>,
    overlays: [Option<Overlay>; 4],
}

/// Why a requested transition cannot originate in the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransitionError {
    RequiresPreview,
    OverlayOwnsInput,
}

impl InteractionState {
    /// Starts an editable session on the source editor.
    pub(crate) fn editable() -> Self {
        Self::from_workspace(Workspace::Editable(EditableWorkspace::Write(
            WriteState::Editor,
        )))
    }

    /// Starts a preview-only session on the preview canvas.
    pub(crate) fn preview_only() -> Self {
        Self::from_workspace(Workspace::PreviewOnly(PreviewState::canvas(
            PreviewMode::View,
        )))
    }

    fn from_workspace(workspace: Workspace) -> Self {
        Self {
            root: RootState::Active(workspace),
        }
    }

    /// Switches between editable Write and Preview. Preview-only sessions
    /// are unchanged.
    pub(crate) fn toggle_preview(&mut self) -> bool {
        if !self.view().can_toggle_preview() {
            return false;
        }

        self.map_workspace(Workspace::toggle_preview);
        true
    }

    /// Toggles Visual only on an exposed preview canvas.
    pub(crate) fn toggle_visual(&mut self) -> Result<(), TransitionError> {
        self.try_map_workspace(Workspace::toggle_visual)
    }

    /// Returns Visual to View and consumes pending preview input.
    pub(crate) fn cancel_preview(&mut self) -> Result<(), TransitionError> {
        if !matches!(self.view().surface(), Surface::Preview) {
            return Err(TransitionError::RequiresPreview);
        }

        self.map_workspace(Workspace::cancel_preview);
        Ok(())
    }

    /// Opens Note on Preview, preserving the suspended preview mode.
    pub(crate) fn open_note(&mut self) -> Result<(), TransitionError> {
        self.try_map_workspace(Workspace::open_note)
    }

    /// Closes Note wherever it appears, including below Find.
    pub(crate) fn close_note(&mut self) -> Option<FocusTarget> {
        let was_open = self.view().contains(Overlay::Note);
        self.map_workspace(Workspace::close_note);
        was_open.then(|| self.focus_target()).flatten()
    }

    /// Opens Find over the current surface and any Note overlay.
    pub(crate) fn open_find(&mut self) {
        self.map_workspace(Workspace::open_find);
    }

    /// Closes Find and restores its exact suspended interaction.
    pub(crate) fn close_find(&mut self) -> Option<FocusTarget> {
        let was_open = self.view().contains(Overlay::Find);
        self.map_workspace(Workspace::close_find);
        was_open.then(|| self.focus_target()).flatten()
    }

    /// Normalizes document-bound state after loading a new document while
    /// retaining Write versus Preview and any root overlays.
    pub(crate) fn document_loaded(&mut self) {
        self.map_workspace(Workspace::document_loaded);
    }

    /// Consumes pending input after an unrelated or completed command.
    pub(crate) fn activity(&mut self) {
        self.map_workspace(Workspace::clear_pending);
    }

    pub(super) fn arm_prefix(&mut self, prefix: Prefix) {
        self.map_workspace(|workspace| workspace.update_pending(|pending| pending.arm(prefix)));
    }

    pub(crate) fn arm_g(&mut self) {
        self.arm_prefix(Prefix::G);
    }

    pub(crate) fn arm_z(&mut self) {
        self.arm_prefix(Prefix::Z);
    }

    pub(crate) fn push_count_digit(&mut self, digit: u32) {
        self.map_workspace(|workspace| {
            workspace.update_pending(|pending| pending.push_digit(digit))
        });
    }

    pub(super) fn has_count(self) -> bool {
        self.pending().has_count()
    }

    pub(super) fn has_prefix(self, prefix: Prefix) -> bool {
        self.pending().has_prefix(prefix)
    }

    pub(super) fn motion_count(self) -> usize {
        self.pending().motion_count()
    }

    pub(super) fn jump_count(self) -> usize {
        self.pending().jump_count()
    }

    fn pending(self) -> Pending {
        match self.root {
            RootState::Active(workspace)
            | RootState::Unsaved {
                resume: workspace, ..
            } => workspace.pending(),
            RootState::Help {
                resume:
                    HelpResume::Active(workspace)
                    | HelpResume::Unsaved {
                        resume: workspace, ..
                    },
            } => workspace.pending(),
        }
    }

    /// Projects all state needed by input and presentation callers.
    pub(crate) fn view(self) -> ViewProjection {
        let (workspace, unsaved, help) = match self.root {
            RootState::Active(workspace) => (workspace, None, false),
            RootState::Unsaved { action, resume } => (resume, Some(action), false),
            RootState::Help {
                resume: HelpResume::Active(workspace),
            } => (workspace, None, true),
            RootState::Help {
                resume: HelpResume::Unsaved { action, resume },
            } => (resume, Some(action), true),
        };

        let mut projection = workspace.view();
        if let Some(action) = unsaved {
            projection.push_overlay(Overlay::Unsaved(action));
        }
        if help {
            projection.push_overlay(Overlay::Help);
        }
        projection
    }

    /// Derives focus directly from the top interaction payload.
    pub(crate) fn focus_target(self) -> Option<FocusTarget> {
        match self.root {
            RootState::Help { .. } => Some(FocusTarget::HelpInput),
            RootState::Unsaved { .. } => None,
            RootState::Active(workspace) => workspace.focus_target(),
        }
    }

    fn map_workspace(&mut self, transition: impl FnOnce(Workspace) -> Workspace + Copy) {
        self.root = self.root.map_workspace(transition);
    }

    fn try_map_workspace(
        &mut self,
        transition: impl FnOnce(Workspace) -> Result<Workspace, TransitionError> + Copy,
    ) -> Result<(), TransitionError> {
        self.root = self.root.try_map_workspace(transition)?;
        Ok(())
    }
}

impl UnsavedResolution {
    pub(crate) fn action(self) -> UnsavedAction {
        self.action
    }

    pub(crate) fn focus(self) -> Option<FocusTarget> {
        self.focus
    }
}

impl ViewProjection {
    fn write(write: WriteState) -> Self {
        let mut projection = Self {
            surface: Surface::Write,
            badge: match write {
                WriteState::Editor => ModeBadge::Write,
                WriteState::Find => ModeBadge::Find,
            },
            preview_mode: None,
            can_toggle_preview: true,
            pending_count: None,
            overlays: [None; 4],
        };
        if matches!(write, WriteState::Find) {
            projection.push_overlay(Overlay::Find);
        }
        projection
    }

    fn preview(preview: PreviewState, can_toggle_preview: bool) -> Self {
        let mode = preview.mode();
        let pending_count = match preview {
            PreviewState::Canvas { pending, .. } => NonZeroU32::new(pending.pending_count()),
            PreviewState::Note { .. } | PreviewState::Find { .. } => None,
        };
        let mut projection = Self {
            surface: Surface::Preview,
            badge: match preview {
                PreviewState::Canvas { .. } => match mode {
                    PreviewMode::View => ModeBadge::View,
                    PreviewMode::Visual => ModeBadge::Visual,
                },
                PreviewState::Note { .. } => ModeBadge::Note,
                PreviewState::Find { .. } => ModeBadge::Find,
            },
            preview_mode: Some(mode),
            can_toggle_preview,
            pending_count,
            overlays: [None; 4],
        };

        match preview {
            PreviewState::Note { .. } => projection.push_overlay(Overlay::Note),
            PreviewState::Find {
                resume: FindResume::Canvas(_),
            } => projection.push_overlay(Overlay::Find),
            PreviewState::Find {
                resume: FindResume::Note(_),
            } => {
                projection.push_overlay(Overlay::Note);
                projection.push_overlay(Overlay::Find);
            }
            PreviewState::Canvas { .. } => {}
        }
        projection
    }

    fn push_overlay(&mut self, overlay: Overlay) {
        let slot = self
            .overlays
            .iter_mut()
            .find(|entry| entry.is_none())
            .expect("the legal hierarchy has at most four overlays");
        *slot = Some(overlay);
    }

    pub(crate) fn surface(self) -> Surface {
        self.surface
    }

    pub(crate) fn badge(self) -> ModeBadge {
        self.badge
    }

    pub(crate) fn can_toggle_preview(self) -> bool {
        self.can_toggle_preview
    }

    pub(crate) fn pending_count(self) -> Option<NonZeroU32> {
        self.pending_count
    }

    pub(super) fn preview_mode(self) -> Option<PreviewMode> {
        self.preview_mode
    }

    pub(crate) fn visual(self) -> bool {
        matches!(self.preview_mode(), Some(PreviewMode::Visual))
    }

    pub(crate) fn overlay(self) -> Option<Overlay> {
        self.overlays.into_iter().flatten().last()
    }

    pub(crate) fn contains(self, expected: Overlay) -> bool {
        self.overlays
            .into_iter()
            .flatten()
            .any(|item| item == expected)
    }

    pub(crate) fn unsaved_action(self) -> Option<UnsavedAction> {
        self.overlays
            .into_iter()
            .flatten()
            .find_map(|item| match item {
                Overlay::Unsaved(action) => Some(action),
                Overlay::Note | Overlay::Find | Overlay::Help => None,
            })
    }

    #[cfg(test)]
    fn overlays(self) -> Vec<Overlay> {
        self.overlays.into_iter().flatten().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(workspace: Workspace) -> InteractionState {
        InteractionState::from_workspace(workspace)
    }

    fn editable_preview(preview: PreviewState) -> Workspace {
        Workspace::Editable(EditableWorkspace::Preview(preview))
    }

    fn canvas(mode: PreviewMode) -> PreviewState {
        PreviewState::canvas(mode)
    }

    fn counted_canvas(mode: PreviewMode, digit: u32) -> InteractionState {
        let mut state = active(editable_preview(canvas(mode)));
        state.push_count_digit(digit);
        state
    }

    fn workspace(state: InteractionState) -> Workspace {
        match state.root {
            RootState::Active(workspace)
            | RootState::Unsaved {
                resume: workspace, ..
            } => workspace,
            RootState::Help {
                resume:
                    HelpResume::Active(workspace)
                    | HelpResume::Unsaved {
                        resume: workspace, ..
                    },
            } => workspace,
        }
    }

    #[test]
    fn constructors_create_editable_write_and_preview_only_view() {
        let editable = InteractionState::editable().view();
        assert_eq!(editable.surface(), Surface::Write);
        assert_eq!(editable.badge(), ModeBadge::Write);
        assert!(editable.can_toggle_preview());
        assert_eq!(editable.overlay(), None);

        let preview_only = InteractionState::preview_only().view();
        assert_eq!(preview_only.surface(), Surface::Preview);
        assert_eq!(preview_only.badge(), ModeBadge::View);
        assert!(!preview_only.can_toggle_preview());
        assert!(!preview_only.visual());
    }

    #[test]
    fn preview_toggle_covers_every_editable_variant_and_preview_only_is_unchanged() {
        let preview_variants = [
            canvas(PreviewMode::View),
            canvas(PreviewMode::Visual),
            PreviewState::Note {
                resume: PreviewMode::View,
            },
            PreviewState::Note {
                resume: PreviewMode::Visual,
            },
            PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::View),
            },
            PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::Visual),
            },
            PreviewState::Find {
                resume: FindResume::Note(PreviewMode::View),
            },
            PreviewState::Find {
                resume: FindResume::Note(PreviewMode::Visual),
            },
        ];

        for write in [WriteState::Editor, WriteState::Find] {
            let mut state = active(Workspace::Editable(EditableWorkspace::Write(write)));
            state.toggle_preview();
            let expected = match write {
                WriteState::Editor => canvas(PreviewMode::View),
                WriteState::Find => PreviewState::Find {
                    resume: FindResume::Canvas(PreviewMode::View),
                },
            };
            assert_eq!(workspace(state), editable_preview(expected));
        }

        for preview in preview_variants {
            let mut state = active(editable_preview(preview));
            state.toggle_preview();
            let expected = if matches!(preview, PreviewState::Find { .. }) {
                WriteState::Find
            } else {
                WriteState::Editor
            };
            assert_eq!(
                workspace(state),
                Workspace::Editable(EditableWorkspace::Write(expected))
            );

            let mut preview_only = active(Workspace::PreviewOnly(preview));
            let before = preview_only;
            preview_only.toggle_preview();
            assert_eq!(preview_only, before);
        }
    }

    #[test]
    fn visual_toggle_is_exhaustive_and_rejects_hidden_or_write_surfaces() {
        for (mode, expected) in [
            (PreviewMode::View, PreviewMode::Visual),
            (PreviewMode::Visual, PreviewMode::View),
        ] {
            let mut editable = counted_canvas(mode, 3);
            assert_eq!(editable.toggle_visual(), Ok(()));
            assert_eq!(workspace(editable), editable_preview(canvas(expected)));

            let mut preview_only = active(Workspace::PreviewOnly(canvas(mode)));
            assert_eq!(preview_only.toggle_visual(), Ok(()));
            assert_eq!(
                workspace(preview_only),
                Workspace::PreviewOnly(canvas(expected))
            );
        }

        for write in [WriteState::Editor, WriteState::Find] {
            let mut state = active(Workspace::Editable(EditableWorkspace::Write(write)));
            let before = state;
            assert_eq!(state.toggle_visual(), Err(TransitionError::RequiresPreview));
            assert_eq!(state, before);
        }

        for preview in [
            PreviewState::Note {
                resume: PreviewMode::View,
            },
            PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::Visual),
            },
            PreviewState::Find {
                resume: FindResume::Note(PreviewMode::View),
            },
        ] {
            for wrap in [
                editable_preview as fn(PreviewState) -> Workspace,
                Workspace::PreviewOnly,
            ] {
                let mut state = active(wrap(preview));
                let before = state;
                assert_eq!(
                    state.toggle_visual(),
                    Err(TransitionError::OverlayOwnsInput)
                );
                assert_eq!(state, before);
            }
        }
    }

    #[test]
    fn preview_cancel_clears_pending_and_normalizes_every_suspended_visual_mode() {
        let variants = [
            canvas(PreviewMode::Visual),
            PreviewState::Note {
                resume: PreviewMode::Visual,
            },
            PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::Visual),
            },
            PreviewState::Find {
                resume: FindResume::Note(PreviewMode::Visual),
            },
        ];
        let expected = [
            canvas(PreviewMode::View),
            PreviewState::Note {
                resume: PreviewMode::View,
            },
            PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::View),
            },
            PreviewState::Find {
                resume: FindResume::Note(PreviewMode::View),
            },
        ];

        for (variant, expected) in variants.into_iter().zip(expected) {
            for wrap in [
                editable_preview as fn(PreviewState) -> Workspace,
                Workspace::PreviewOnly,
            ] {
                let mut state = active(wrap(variant));
                assert_eq!(state.cancel_preview(), Ok(()));
                assert_eq!(workspace(state), wrap(expected));
            }
        }

        let mut pending = counted_canvas(PreviewMode::View, 4);
        assert_eq!(pending.cancel_preview(), Ok(()));
        assert_eq!(pending.view().pending_count(), None);

        let mut write = InteractionState::editable();
        let before = write;
        assert_eq!(
            write.cancel_preview(),
            Err(TransitionError::RequiresPreview)
        );
        assert_eq!(write, before);
    }

    #[test]
    fn note_transitions_cover_rejection_idempotence_and_find_restoration() {
        for write in [WriteState::Editor, WriteState::Find] {
            let mut state = active(Workspace::Editable(EditableWorkspace::Write(write)));
            let before = state;
            assert_eq!(state.open_note(), Err(TransitionError::RequiresPreview));
            assert_eq!(state, before);
        }

        for mode in [PreviewMode::View, PreviewMode::Visual] {
            for wrap in [
                editable_preview as fn(PreviewState) -> Workspace,
                Workspace::PreviewOnly,
            ] {
                let mut canvas_state = active(wrap(canvas(mode)));
                assert_eq!(canvas_state.open_note(), Ok(()));
                let note = wrap(PreviewState::Note { resume: mode });
                assert_eq!(workspace(canvas_state), note);
                assert_eq!(canvas_state.open_note(), Ok(()));
                assert_eq!(workspace(canvas_state), note);
                let _ = canvas_state.close_note();
                assert_eq!(workspace(canvas_state), wrap(canvas(mode)));
                let closed = canvas_state;
                let _ = canvas_state.close_note();
                assert_eq!(canvas_state, closed);

                let mut find = active(wrap(PreviewState::Find {
                    resume: FindResume::Canvas(mode),
                }));
                assert_eq!(find.open_note(), Ok(()));
                assert_eq!(
                    workspace(find),
                    wrap(PreviewState::Find {
                        resume: FindResume::Note(mode),
                    })
                );
                assert_eq!(find.open_note(), Ok(()));
                let _ = find.close_note();
                assert_eq!(
                    workspace(find),
                    wrap(PreviewState::Find {
                        resume: FindResume::Canvas(mode),
                    })
                );
            }
        }
    }

    #[test]
    fn find_transitions_restore_every_surface_and_are_idempotent() {
        let cases = [
            Workspace::Editable(EditableWorkspace::Write(WriteState::Editor)),
            editable_preview(canvas(PreviewMode::View)),
            editable_preview(canvas(PreviewMode::Visual)),
            editable_preview(PreviewState::Note {
                resume: PreviewMode::View,
            }),
            editable_preview(PreviewState::Note {
                resume: PreviewMode::Visual,
            }),
            Workspace::PreviewOnly(canvas(PreviewMode::View)),
            Workspace::PreviewOnly(canvas(PreviewMode::Visual)),
            Workspace::PreviewOnly(PreviewState::Note {
                resume: PreviewMode::View,
            }),
            Workspace::PreviewOnly(PreviewState::Note {
                resume: PreviewMode::Visual,
            }),
        ];

        for original in cases {
            let mut state = active(original);
            state.open_find();
            let opened = state;
            state.open_find();
            assert_eq!(state, opened);
            let _ = state.close_find();
            assert_eq!(workspace(state), original);
            let closed = state;
            let _ = state.close_find();
            assert_eq!(state, closed);
        }
    }

    #[test]
    fn help_and_unsaved_wrap_and_restore_every_root_path() {
        let action = UnsavedAction::OpenFile;
        let other = UnsavedAction::CloseWindow(iced::window::Id::unique());
        let mut state = counted_canvas(PreviewMode::Visual, 3);
        state.arm_prefix(Prefix::G);
        let active_root = state.root;

        state.open_help();
        let help_over_active = state.root;
        state.open_help();
        assert_eq!(state.root, help_over_active);
        assert_eq!(state.view().pending_count().map(NonZeroU32::get), Some(3));
        assert!(state.has_prefix(Prefix::G));
        assert_eq!(state.close_help(), None);
        assert_eq!(state.root, active_root);
        assert_eq!(state.close_help(), None);
        assert_eq!(state.root, active_root);

        state.open_unsaved(action);
        let unsaved_root = state.root;
        assert_eq!(state.view().pending_count(), None);
        assert!(!state.has_prefix(Prefix::G));
        state.open_unsaved(other);
        assert_eq!(
            state.root, unsaved_root,
            "duplicate retains the first action"
        );
        state.open_help();
        let help_over_unsaved = state.root;
        state.open_help();
        assert_eq!(state.root, help_over_unsaved);
        assert_eq!(
            state.view().overlays(),
            vec![Overlay::Unsaved(action), Overlay::Help]
        );
        assert_eq!(state.close_help(), None);
        assert_eq!(state.root, unsaved_root);
        let resolved = state.resolve_unsaved().expect("unsaved action");
        assert_eq!(resolved.action(), action);
        assert_eq!(resolved.focus(), None);
        assert!(matches!(state.root, RootState::Active(_)));

        let mut help_first = counted_canvas(PreviewMode::View, 5);
        help_first.open_help();
        help_first.open_unsaved(action);
        assert_eq!(
            help_first.view().overlays(),
            vec![Overlay::Unsaved(action), Overlay::Help]
        );
        assert_eq!(help_first.view().pending_count(), None);
        let nested = help_first.root;
        help_first.open_unsaved(other);
        assert_eq!(help_first.root, nested);
        let resolved = help_first.resolve_unsaved().expect("unsaved below Help");
        assert_eq!(resolved.action(), action);
        assert_eq!(resolved.focus(), Some(FocusTarget::HelpInput));
        assert_eq!(help_first.view().overlays(), vec![Overlay::Help]);
        assert_eq!(help_first.resolve_unsaved(), None);
        assert_eq!(help_first.view().overlays(), vec![Overlay::Help]);
        let _ = help_first.close_help();
        assert!(matches!(help_first.root, RootState::Active(_)));
    }

    #[test]
    fn closing_overlays_restores_focus_from_the_suspended_payload() {
        let mut write = InteractionState::editable();
        write.open_help();
        assert_eq!(write.close_help(), Some(FocusTarget::SourceEditor));

        let mut note = active(editable_preview(canvas(PreviewMode::View)));
        note.open_note().unwrap();
        note.open_help();
        assert_eq!(note.close_help(), Some(FocusTarget::NoteComposer));

        note.open_find();
        note.open_help();
        assert_eq!(note.close_help(), Some(FocusTarget::FindInput));
        assert_eq!(note.close_find(), Some(FocusTarget::NoteComposer));
        assert_eq!(note.close_note(), None);

        let mut unsaved = InteractionState::editable();
        unsaved.open_unsaved(UnsavedAction::OpenFile);
        let resolution = unsaved.resolve_unsaved().expect("visible prompt");
        assert_eq!(resolution.focus(), Some(FocusTarget::SourceEditor));

        let mut nested = active(editable_preview(PreviewState::Note {
            resume: PreviewMode::Visual,
        }));
        nested.open_unsaved(UnsavedAction::OpenFile);
        nested.open_help();
        let resolution = nested.resolve_unsaved().expect("prompt below Help");
        assert_eq!(resolution.focus(), Some(FocusTarget::HelpInput));
        assert_eq!(nested.close_help(), Some(FocusTarget::NoteComposer));
    }

    #[test]
    fn projections_cover_every_workspace_variant_and_overlay_order() {
        let cases = [
            (
                Workspace::Editable(EditableWorkspace::Write(WriteState::Editor)),
                Surface::Write,
                ModeBadge::Write,
                false,
                vec![],
            ),
            (
                Workspace::Editable(EditableWorkspace::Write(WriteState::Find)),
                Surface::Write,
                ModeBadge::Find,
                false,
                vec![Overlay::Find],
            ),
            (
                editable_preview(canvas(PreviewMode::View)),
                Surface::Preview,
                ModeBadge::View,
                false,
                vec![],
            ),
            (
                editable_preview(canvas(PreviewMode::Visual)),
                Surface::Preview,
                ModeBadge::Visual,
                true,
                vec![],
            ),
            (
                editable_preview(PreviewState::Note {
                    resume: PreviewMode::Visual,
                }),
                Surface::Preview,
                ModeBadge::Note,
                true,
                vec![Overlay::Note],
            ),
            (
                editable_preview(PreviewState::Find {
                    resume: FindResume::Canvas(PreviewMode::View),
                }),
                Surface::Preview,
                ModeBadge::Find,
                false,
                vec![Overlay::Find],
            ),
            (
                editable_preview(PreviewState::Find {
                    resume: FindResume::Note(PreviewMode::Visual),
                }),
                Surface::Preview,
                ModeBadge::Find,
                true,
                vec![Overlay::Note, Overlay::Find],
            ),
        ];

        for (workspace, surface, badge, visual, overlays) in cases {
            let projection = active(workspace).view();
            assert_eq!(projection.surface(), surface);
            assert_eq!(projection.badge(), badge);
            assert_eq!(projection.visual(), visual);
            assert_eq!(projection.overlays(), overlays);
        }

        for preview in [
            canvas(PreviewMode::View),
            canvas(PreviewMode::Visual),
            PreviewState::Note {
                resume: PreviewMode::View,
            },
            PreviewState::Note {
                resume: PreviewMode::Visual,
            },
            PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::View),
            },
            PreviewState::Find {
                resume: FindResume::Note(PreviewMode::Visual),
            },
        ] {
            let editable = active(editable_preview(preview)).view();
            let preview_only = active(Workspace::PreviewOnly(preview)).view();
            assert_eq!(preview_only.surface(), editable.surface());
            assert_eq!(preview_only.badge(), editable.badge());
            assert_eq!(preview_only.visual(), editable.visual());
            assert_eq!(preview_only.overlays(), editable.overlays());
            assert!(editable.can_toggle_preview());
            assert!(!preview_only.can_toggle_preview());
        }

        let mut nested = active(editable_preview(PreviewState::Find {
            resume: FindResume::Note(PreviewMode::Visual),
        }));
        nested.open_unsaved(UnsavedAction::OpenFile);
        nested.open_help();
        let projection = nested.view();
        assert_eq!(projection.badge(), ModeBadge::Find);
        assert_eq!(projection.overlay(), Some(Overlay::Help));
        assert_eq!(
            projection.overlays(),
            vec![
                Overlay::Note,
                Overlay::Find,
                Overlay::Unsaved(UnsavedAction::OpenFile),
                Overlay::Help,
            ]
        );
    }

    #[test]
    fn document_loaded_normalizes_every_workspace_variant_under_every_root() {
        let variants = [
            Workspace::Editable(EditableWorkspace::Write(WriteState::Editor)),
            Workspace::Editable(EditableWorkspace::Write(WriteState::Find)),
            editable_preview(canvas(PreviewMode::View)),
            editable_preview(canvas(PreviewMode::Visual)),
            editable_preview(PreviewState::Note {
                resume: PreviewMode::Visual,
            }),
            editable_preview(PreviewState::Find {
                resume: FindResume::Canvas(PreviewMode::Visual),
            }),
            editable_preview(PreviewState::Find {
                resume: FindResume::Note(PreviewMode::Visual),
            }),
            Workspace::PreviewOnly(canvas(PreviewMode::Visual)),
            Workspace::PreviewOnly(PreviewState::Note {
                resume: PreviewMode::View,
            }),
            Workspace::PreviewOnly(PreviewState::Find {
                resume: FindResume::Note(PreviewMode::Visual),
            }),
        ];

        for variant in variants {
            let expected = match variant {
                Workspace::Editable(EditableWorkspace::Write(_)) => {
                    Workspace::Editable(EditableWorkspace::Write(WriteState::Editor))
                }
                Workspace::Editable(EditableWorkspace::Preview(_)) => {
                    editable_preview(canvas(PreviewMode::View))
                }
                Workspace::PreviewOnly(_) => Workspace::PreviewOnly(canvas(PreviewMode::View)),
            };

            let roots = [
                RootState::Active(variant),
                RootState::Unsaved {
                    action: UnsavedAction::OpenFile,
                    resume: variant,
                },
                RootState::Help {
                    resume: HelpResume::Active(variant),
                },
                RootState::Help {
                    resume: HelpResume::Unsaved {
                        action: UnsavedAction::OpenFile,
                        resume: variant,
                    },
                },
            ];

            for root in roots {
                let mut state = InteractionState { root };
                state.document_loaded();
                assert_eq!(workspace(state), expected);
                assert_eq!(
                    state.view().overlays(),
                    match root {
                        RootState::Active(_) => vec![],
                        RootState::Unsaved { action, .. } => vec![Overlay::Unsaved(action)],
                        RootState::Help {
                            resume: HelpResume::Active(_),
                        } => vec![Overlay::Help],
                        RootState::Help {
                            resume: HelpResume::Unsaved { action, .. },
                        } => vec![Overlay::Unsaved(action), Overlay::Help],
                    }
                );
            }
        }
    }
}
