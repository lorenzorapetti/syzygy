//! sone's icons: Lucide 0.563.0 (lucide-react in sone), as the official
//! lucide-static SVGs in `icons/`, drawn with iced's `svg` widget and tinted
//! through `svg::Style::color`.

use iced::Color;
use iced::widget::svg::{self, Handle, Svg};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy)]
pub enum Icon {
    ChevronLeft,
    ChevronRight,
    House,
    Library,
    Search,
    X,
}

/// In [`Icon`] order.
const SOURCES: [&str; 6] = [
    include_str!("../icons/chevron-left.svg"),
    include_str!("../icons/chevron-right.svg"),
    include_str!("../icons/house.svg"),
    include_str!("../icons/library.svg"),
    include_str!("../icons/search.svg"),
    include_str!("../icons/x.svg"),
];

// One Handle per icon, built once: a new Handle each view would be
// rasterised again.
static OUTLINE: LazyLock<Vec<Handle>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|source| Handle::from_memory(source.as_bytes()))
        .collect()
});

/// sone fills Play, Pause, Skip and a loved Heart by setting `fill` to the
/// stroke colour; the root's `fill="none"` becomes `currentColor`.
static FILLED: LazyLock<Vec<Handle>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|source| {
            let filled = source.replacen("fill=\"none\"", "fill=\"currentColor\"", 1);
            Handle::from_memory(filled.into_bytes())
        })
        .collect()
});

/// An outline icon, `size` square, in `color`.
pub fn icon<'a>(icon: Icon, size: f32, color: Color) -> Svg<'a> {
    tinted(OUTLINE[icon as usize].clone(), size, color)
}

/// A solid icon, `size` square, in `color`.
#[expect(
    dead_code,
    reason = "the player bar's Play and Pause and a loved Heart are the first"
)]
pub fn filled<'a>(icon: Icon, size: f32, color: Color) -> Svg<'a> {
    tinted(FILLED[icon as usize].clone(), size, color)
}

fn tinted<'a>(handle: Handle, size: f32, color: Color) -> Svg<'a> {
    Svg::new(handle)
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}
