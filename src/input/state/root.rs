//! Help and unsaved root-overlay transitions.

use super::*;

impl InteractionState {
    /// Wraps Active or Unsaved with Help. Duplicate opens are idempotent.
    pub(in crate::input) fn open_help(&mut self) {
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
    pub(in crate::input) fn close_help(&mut self) {
        self.root = match self.root {
            RootState::Help {
                resume: HelpResume::Active(workspace),
            } => RootState::Active(workspace),
            RootState::Help {
                resume: HelpResume::Unsaved { action, resume },
            } => RootState::Unsaved { action, resume },
            root @ (RootState::Active(_) | RootState::Unsaved { .. }) => root,
        };
    }

    /// Opens Unsaved below Help when necessary and consumes pending input.
    /// A duplicate request retains the first action being confirmed.
    pub(in crate::input) fn open_unsaved(&mut self, action: UnsavedAction) {
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

    /// Closes or answers Unsaved without disturbing a Help layer above it.
    pub(in crate::input) fn close_unsaved(&mut self) {
        self.root = match self.root {
            RootState::Unsaved { resume, .. } => RootState::Active(resume),
            RootState::Help {
                resume: HelpResume::Unsaved { resume, .. },
            } => RootState::Help {
                resume: HelpResume::Active(resume),
            },
            root @ (RootState::Active(_)
            | RootState::Help {
                resume: HelpResume::Active(_),
            }) => root,
        };
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
