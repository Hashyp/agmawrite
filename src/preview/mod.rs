//! Preview model and caret navigation.

mod model;

pub(crate) use model::{
    element_selection, Caret, CaretPosition, Claims, ElementMap, Jump, Motion, Page, Placement,
    PreviewElement, WordMotion,
};
