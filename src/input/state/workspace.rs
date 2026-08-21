//! Write/Preview workspace transitions.

use super::*;

impl Workspace {
    pub(super) fn toggle_preview(self) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Write(WriteState::Editor)) => Self::Editable(
                EditableWorkspace::Preview(PreviewState::canvas(PreviewMode::View)),
            ),
            Self::Editable(EditableWorkspace::Write(WriteState::Find)) => {
                Self::Editable(EditableWorkspace::Preview(PreviewState::Find {
                    resume: FindResume::Canvas(PreviewMode::View),
                }))
            }
            Self::Editable(EditableWorkspace::Preview(PreviewState::Find { .. })) => {
                Self::Editable(EditableWorkspace::Write(WriteState::Find))
            }
            Self::Editable(EditableWorkspace::Preview(
                PreviewState::Canvas { .. } | PreviewState::Note { .. },
            )) => Self::Editable(EditableWorkspace::Write(WriteState::Editor)),
            preview_only @ Self::PreviewOnly(_) => preview_only,
        }
    }

    pub(super) fn toggle_visual(self) -> Result<Self, TransitionError> {
        match self {
            Self::Editable(EditableWorkspace::Write(_)) => Err(TransitionError::RequiresPreview),
            Self::Editable(EditableWorkspace::Preview(preview)) => preview
                .toggle_visual()
                .map(|preview| Self::Editable(EditableWorkspace::Preview(preview))),
            Self::PreviewOnly(preview) => preview.toggle_visual().map(Self::PreviewOnly),
        }
    }

    pub(super) fn cancel_preview(self) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Write(_)) => self,
            Self::Editable(EditableWorkspace::Preview(preview)) => {
                Self::Editable(EditableWorkspace::Preview(preview.cancel_preview()))
            }
            Self::PreviewOnly(preview) => Self::PreviewOnly(preview.cancel_preview()),
        }
    }

    pub(super) fn open_note(self) -> Result<Self, TransitionError> {
        match self {
            Self::Editable(EditableWorkspace::Write(_)) => Err(TransitionError::RequiresPreview),
            Self::Editable(EditableWorkspace::Preview(preview)) => Ok(Self::Editable(
                EditableWorkspace::Preview(preview.open_note()),
            )),
            Self::PreviewOnly(preview) => Ok(Self::PreviewOnly(preview.open_note())),
        }
    }

    pub(super) fn close_note(self) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Write(_)) => self,
            Self::Editable(EditableWorkspace::Preview(preview)) => {
                Self::Editable(EditableWorkspace::Preview(preview.close_note()))
            }
            Self::PreviewOnly(preview) => Self::PreviewOnly(preview.close_note()),
        }
    }

    pub(super) fn open_find(self) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Write(WriteState::Editor)) => {
                Self::Editable(EditableWorkspace::Write(WriteState::Find))
            }
            Self::Editable(EditableWorkspace::Write(WriteState::Find)) => self,
            Self::Editable(EditableWorkspace::Preview(preview)) => {
                Self::Editable(EditableWorkspace::Preview(preview.open_find()))
            }
            Self::PreviewOnly(preview) => Self::PreviewOnly(preview.open_find()),
        }
    }

    pub(super) fn close_find(self) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Write(WriteState::Find)) => {
                Self::Editable(EditableWorkspace::Write(WriteState::Editor))
            }
            Self::Editable(EditableWorkspace::Write(WriteState::Editor)) => self,
            Self::Editable(EditableWorkspace::Preview(preview)) => {
                Self::Editable(EditableWorkspace::Preview(preview.close_find()))
            }
            Self::PreviewOnly(preview) => Self::PreviewOnly(preview.close_find()),
        }
    }

    pub(super) fn document_loaded(self) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Write(_)) => {
                Self::Editable(EditableWorkspace::Write(WriteState::Editor))
            }
            Self::Editable(EditableWorkspace::Preview(_)) => Self::Editable(
                EditableWorkspace::Preview(PreviewState::canvas(PreviewMode::View)),
            ),
            Self::PreviewOnly(_) => Self::PreviewOnly(PreviewState::canvas(PreviewMode::View)),
        }
    }

    pub(super) fn clear_pending(self) -> Self {
        self.update_pending(Pending::consume)
    }

    pub(super) fn update_pending(self, update: impl FnOnce(&mut Pending)) -> Self {
        match self {
            Self::Editable(EditableWorkspace::Preview(preview)) => {
                Self::Editable(EditableWorkspace::Preview(preview.update_pending(update)))
            }
            Self::PreviewOnly(preview) => Self::PreviewOnly(preview.update_pending(update)),
            Self::Editable(EditableWorkspace::Write(_)) => self,
        }
    }

    pub(super) fn pending(self) -> Pending {
        match self {
            Self::Editable(EditableWorkspace::Preview(PreviewState::Canvas {
                pending, ..
            }))
            | Self::PreviewOnly(PreviewState::Canvas { pending, .. }) => pending,
            Self::Editable(
                EditableWorkspace::Write(_)
                | EditableWorkspace::Preview(PreviewState::Note { .. } | PreviewState::Find { .. }),
            )
            | Self::PreviewOnly(PreviewState::Note { .. } | PreviewState::Find { .. }) => {
                Pending::default()
            }
        }
    }

    pub(super) fn view(self) -> ViewProjection {
        match self {
            Self::Editable(EditableWorkspace::Write(write)) => ViewProjection::write(write),
            Self::Editable(EditableWorkspace::Preview(preview)) => {
                ViewProjection::preview(preview, true)
            }
            Self::PreviewOnly(preview) => ViewProjection::preview(preview, false),
        }
    }

    pub(super) fn focus_target(self) -> Option<FocusTarget> {
        match self {
            Self::Editable(EditableWorkspace::Write(WriteState::Editor)) => {
                Some(FocusTarget::SourceEditor)
            }
            Self::Editable(EditableWorkspace::Write(WriteState::Find))
            | Self::Editable(EditableWorkspace::Preview(PreviewState::Find { .. }))
            | Self::PreviewOnly(PreviewState::Find { .. }) => Some(FocusTarget::FindInput),
            Self::Editable(EditableWorkspace::Preview(PreviewState::Note { .. }))
            | Self::PreviewOnly(PreviewState::Note { .. }) => Some(FocusTarget::NoteComposer),
            Self::Editable(EditableWorkspace::Preview(PreviewState::Canvas { .. }))
            | Self::PreviewOnly(PreviewState::Canvas { .. }) => None,
        }
    }
}

impl PreviewState {
    pub(super) fn canvas(mode: PreviewMode) -> Self {
        Self::Canvas {
            mode,
            pending: Pending::default(),
        }
    }

    fn toggle_visual(self) -> Result<Self, TransitionError> {
        match self {
            Self::Canvas { mode, .. } => Ok(Self::canvas(mode.toggled())),
            Self::Note { .. } | Self::Find { .. } => Err(TransitionError::OverlayOwnsInput),
        }
    }

    fn cancel_preview(self) -> Self {
        match self {
            Self::Canvas { mode, .. } => Self::canvas(mode.cancel_visual()),
            Self::Note { resume } => Self::Note {
                resume: resume.cancel_visual(),
            },
            Self::Find {
                resume: FindResume::Canvas(mode),
            } => Self::Find {
                resume: FindResume::Canvas(mode.cancel_visual()),
            },
            Self::Find {
                resume: FindResume::Note(mode),
            } => Self::Find {
                resume: FindResume::Note(mode.cancel_visual()),
            },
        }
    }

    fn open_note(self) -> Self {
        match self {
            Self::Canvas { mode, .. } => Self::Note { resume: mode },
            note @ Self::Note { .. } => note,
            Self::Find {
                resume: FindResume::Canvas(mode),
            } => Self::Find {
                resume: FindResume::Note(mode),
            },
            find @ Self::Find {
                resume: FindResume::Note(_),
            } => find,
        }
    }

    fn close_note(self) -> Self {
        match self {
            Self::Note { resume } => Self::canvas(resume),
            Self::Find {
                resume: FindResume::Note(mode),
            } => Self::Find {
                resume: FindResume::Canvas(mode),
            },
            state @ (Self::Canvas { .. }
            | Self::Find {
                resume: FindResume::Canvas(_),
            }) => state,
        }
    }

    fn open_find(self) -> Self {
        match self {
            Self::Canvas { mode, .. } => Self::Find {
                resume: FindResume::Canvas(mode),
            },
            Self::Note { resume } => Self::Find {
                resume: FindResume::Note(resume),
            },
            find @ Self::Find { .. } => find,
        }
    }

    fn close_find(self) -> Self {
        match self {
            Self::Find {
                resume: FindResume::Canvas(mode),
            } => Self::canvas(mode),
            Self::Find {
                resume: FindResume::Note(mode),
            } => Self::Note { resume: mode },
            state @ (Self::Canvas { .. } | Self::Note { .. }) => state,
        }
    }

    fn update_pending(self, update: impl FnOnce(&mut Pending)) -> Self {
        match self {
            Self::Canvas { mode, mut pending } => {
                update(&mut pending);
                Self::Canvas { mode, pending }
            }
            Self::Note { .. } | Self::Find { .. } => self,
        }
    }

    pub(super) fn mode(self) -> PreviewMode {
        match self {
            Self::Canvas { mode, .. } | Self::Note { resume: mode } => mode,
            Self::Find {
                resume: FindResume::Canvas(mode) | FindResume::Note(mode),
            } => mode,
        }
    }
}

impl PreviewMode {
    fn toggled(self) -> Self {
        match self {
            Self::View => Self::Visual,
            Self::Visual => Self::View,
        }
    }

    fn cancel_visual(self) -> Self {
        match self {
            Self::View | Self::Visual => Self::View,
        }
    }
}
