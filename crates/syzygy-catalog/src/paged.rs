//! Some of a long list.

/// Some of a long list, and whether there may be more after it.
#[derive(Debug, Clone)]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub has_more: bool,
    /// Where the next page starts, for lists TIDAL pages by cursor rather
    /// than by offset.
    pub cursor: Option<String>,
    /// How long the whole list is, when TIDAL says.
    pub total: Option<usize>,
}
