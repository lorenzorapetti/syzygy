//! PROTOTYPE. sone's icon set: Lucide 0.563.0 (lucide-react in sone), as the
//! official lucide-static SVGs in `icons/`, rendered with iced's `svg` widget
//! and tinted through `svg::Style::color`. "Filled" variants swap the root
//! `fill="none"` for `currentColor`, which is how sone fills Play, Pause,
//! SkipBack/Forward and a favourited Heart.

use std::sync::LazyLock;

use iced::widget::svg::{self, Handle, Svg};
use iced::Color;

#[derive(Debug, Clone, Copy)]
pub enum I {
    House,
    Compass,
    Bell,
    Search,
    Library,
    Plus,
    ChevronLeft,
    ChevronRight,
    Play,
    Pause,
    SkipBack,
    SkipForward,
    Shuffle,
    Repeat,
    Heart,
    ListMusic,
    Volume2,
    Volume1,
    VolumeX,
    X,
    Ellipsis,
}

const SOURCES: [&str; 21] = [
    include_str!("../icons/house.svg"),
    include_str!("../icons/compass.svg"),
    include_str!("../icons/bell.svg"),
    include_str!("../icons/search.svg"),
    include_str!("../icons/library.svg"),
    include_str!("../icons/plus.svg"),
    include_str!("../icons/chevron-left.svg"),
    include_str!("../icons/chevron-right.svg"),
    include_str!("../icons/play.svg"),
    include_str!("../icons/pause.svg"),
    include_str!("../icons/skip-back.svg"),
    include_str!("../icons/skip-forward.svg"),
    include_str!("../icons/shuffle.svg"),
    include_str!("../icons/repeat.svg"),
    include_str!("../icons/heart.svg"),
    include_str!("../icons/list-music.svg"),
    include_str!("../icons/volume-2.svg"),
    include_str!("../icons/volume-1.svg"),
    include_str!("../icons/volume-x.svg"),
    include_str!("../icons/x.svg"),
    include_str!("../icons/ellipsis.svg"),
];

// One Handle per icon, built once: a fresh Handle per view would re-rasterise.
static OUTLINE: LazyLock<Vec<Handle>> =
    LazyLock::new(|| SOURCES.iter().map(|s| Handle::from_memory(s.as_bytes())).collect());
static FILLED: LazyLock<Vec<Handle>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|s| Handle::from_memory(s.replacen("fill=\"none\"", "fill=\"currentColor\"", 1).into_bytes()))
        .collect()
});

pub fn icon<'a>(i: I, size: f32, color: Color) -> Svg<'a> {
    sized(OUTLINE[i as usize].clone(), size).style(move |_, _| svg::Style { color: Some(color) })
}

pub fn filled<'a>(i: I, size: f32, color: Color) -> Svg<'a> {
    sized(FILLED[i as usize].clone(), size).style(move |_, _| svg::Style { color: Some(color) })
}

/// Outline icon that brightens when hovered (ghost buttons).
pub fn hover<'a>(i: I, size: f32, idle: Color, hot: Color) -> Svg<'a> {
    sized(OUTLINE[i as usize].clone(), size).style(move |_, status| svg::Style {
        color: Some(if matches!(status, svg::Status::Hovered) { hot } else { idle }),
    })
}

fn sized<'a>(h: Handle, size: f32) -> Svg<'a> {
    iced::widget::svg(h).width(size).height(size)
}
