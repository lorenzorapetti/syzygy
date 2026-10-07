//! Some of a long list.

/// Some of a long list, and whether there may be more after it.
#[derive(Debug, Clone)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}
