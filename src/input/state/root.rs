//! Help and unsaved root-overlay transitions.

use super::*;

impl InteractionState {
    /// Wraps Active or Unsaved with Help. Duplicate opens are idempotent.
    pub(crate) fn open_help(&mut self) {
        self.root = match self.root {
            RootState::Active(workspace) => RootState::Help {
                resume: HelpResume::Active(workspace),
            },
            RootState::Unsaved { action, resume } => RootState::Help {
                resume: HelpResume::Unsaved { action, resume },
            },
            help @ RootState::Help { .. } => help,
        };
    }

    /// Removes exactly one Help layer and restores the suspended root.
    pub(crate) fn close_help(&mut self) -> Option<FocusTarget> {
        let was_open = matches!(self.root, RootState::Help { .. });
        self.root = match self.root {
            RootState::Help {
                resume: HelpResume::Active(workspace),
            } => RootState::Active(workspace),
            RootState::Help {
                resume: HelpResume::Unsaved { action, resume },
            } => RootState::Unsaved { action, resume },
            root @ (RootState::Active(_) | RootState::Unsaved { .. }) => root,
        };
        was_open.then(|| self.focus_target()).flatten()
    }

    /// Opens Unsaved below Help when necessary and consumes pending input.
    /// A duplicate request retains the first action being confirmed.
    pub(crate) fn open_unsaved(&mut self, action: UnsavedAction) {
        self.root = match self.root {
            RootState::Active(workspace) => RootState::Unsaved {
                action,
                resume: workspace.clear_pending(),
            },
            unsaved @ RootState::Unsaved { .. } => unsaved,
            RootState::Help {
                resume: HelpResume::Active(workspace),
            } => RootState::Help {
                resume: HelpResume::Unsaved {
                    action,
                    resume: workspace.clear_pending(),
                },
            },
            help @ RootState::Help {
                resume: HelpResume::Unsaved { .. },
            } => help,
        };
    }

    /// Extracts the action being answered without disturbing Help above it.
    pub(crate) fn resolve_unsaved(&mut self) -> Option<UnsavedResolution> {
        let (root, action) = match self.root {
            RootState::Unsaved { action, resume } => (RootState::Active(resume), Some(action)),
            RootState::Help {
                resume: HelpResume::Unsaved { action, resume },
            } => (
                RootState::Help {
                    resume: HelpResume::Active(resume),
                },
                Some(action),
            ),
            root @ (RootState::Active(_)
            | RootState::Help {
                resume: HelpResume::Active(_),
            }) => (root, None),
        };
        self.root = root;

        action.map(|action| UnsavedResolution {
            action,
            focus: self.focus_target(),
        })
    }
}

impl RootState {
    pub(super) fn map_workspace(
        self,
        transition: impl FnOnce(Workspace) -> Workspace + Copy,
    ) -> Self {
        match self {
            Self::Active(workspace) => Self::Active(transition(workspace)),
            Self::Unsaved { action, resume } => Self::Unsaved {
                action,
                resume: transition(resume),
            },
            Self::Help {
                resume: HelpResume::Active(workspace),
            } => Self::Help {
                resume: HelpResume::Active(transition(workspace)),
            },
            Self::Help {
                resume: HelpResume::Unsaved { action, resume },
            } => Self::Help {
                resume: HelpResume::Unsaved {
                    action,
                    resume: transition(resume),
                },
            },
        }
    }

    pub(super) fn try_map_workspace(
        self,
        transition: impl FnOnce(Workspace) -> Result<Workspace, TransitionError> + Copy,
    ) -> Result<Self, TransitionError> {
        Ok(match self {
            Self::Active(workspace) => Self::Active(transition(workspace)?),
            Self::Unsaved { action, resume } => Self::Unsaved {
                action,
                resume: transition(resume)?,
            },
            Self::Help {
                resume: HelpResume::Active(workspace),
            } => Self::Help {
                resume: HelpResume::Active(transition(workspace)?),
            },
            Self::Help {
                resume: HelpResume::Unsaved { action, resume },
            } => Self::Help {
                resume: HelpResume::Unsaved {
                    action,
                    resume: transition(resume)?,
                },
            },
        })
    }
}
