//! PROTOTYPE. sone's default dark theme ("Violet Night", accent #A855F7 on
//! #130F1A), run through sone's `deriveTheme` and hardcoded here.
//!
//! The style approach under test: `Theme::custom` only feeds iced's defaults
//! (scrollbars, sliders we don't restyle), and every widget we care about gets
//! a plain `fn(&Theme, Status) -> Style` that reads these constants.

use iced::widget::{button, container, scrollable, slider};
use iced::{Background, Border, Color, Shadow, Theme, Vector, color};

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

pub const BG_BASE: Color = color!(0x130F1A);
pub const BG_SURFACE: Color = color!(0x1c1627);
pub const BG_SURFACE_HOVER: Color = color!(0x282037);
pub const BG_ELEVATED: Color = color!(0x191422);
pub const BG_SIDEBAR: Color = color!(0x0e0b14);
pub const BG_OVERLAY: Color = color!(0x0c0910);
pub const BG_INSET: Color = color!(0x241c31);
pub const BG_INSET_HOVER: Color = color!(0x2d243e);
pub const BG_BUTTON: Color = color!(0x342947);
pub const BG_BUTTON_HOVER: Color = color!(0x403257);

pub const ACCENT: Color = color!(0xA855F7);
pub const ACCENT_HOVER: Color = color!(0x952ff5);
pub const ON_ACCENT: Color = Color::BLACK;

pub const TEXT_PRIMARY: Color = Color::WHITE;
pub const TEXT_SECONDARY: Color = color!(0xb3b3b3);
pub const TEXT_MUTED: Color = color!(0xa6a6a6);
pub const TEXT_FAINT: Color = color!(0x666666);
pub const TEXT_DISABLED: Color = color!(0x535353);

pub const BORDER_SUBTLE: Color = rgba(255, 255, 255, 0.06);
pub const HL_FAINT: Color = rgba(255, 255, 255, 0.04);
pub const HL_MED: Color = rgba(255, 255, 255, 0.08);
pub const HL_STRONG: Color = rgba(255, 255, 255, 0.12);
pub const SLIDER_TRACK: Color = rgba(255, 255, 255, 0.15);
pub const SLIDER_FILL: Color = rgba(255, 255, 255, 0.65);

pub const SUCCESS: Color = color!(0x1ed760);
pub const ERROR: Color = color!(0xff6666);
pub const WARNING: Color = color!(0xffa726);

// Type scale, in px, as sone uses it.
pub const FS_TINY: f32 = 11.0;
pub const FS_SMALL: f32 = 12.0;
pub const FS_BODY: f32 = 13.0;
pub const FS_ROW: f32 = 14.0;
pub const FS_TITLE: f32 = 18.0;
pub const FS_HERO: f32 = 48.0;

pub fn theme() -> Theme {
    Theme::custom(
        "syzygy",
        iced::theme::Palette {
            background: BG_BASE,
            text: TEXT_PRIMARY,
            primary: ACCENT,
            success: SUCCESS,
            warning: WARNING,
            danger: ERROR,
        },
    )
}

fn bg(c: Color) -> Option<Background> {
    Some(Background::Color(c))
}

pub fn fill(c: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style { background: bg(c), ..Default::default() }
}

pub fn fill_rounded(c: Color, r: f32) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: bg(c),
        border: Border { radius: r.into(), ..Default::default() },
        ..Default::default()
    }
}

pub fn top_border(c: Color) -> impl Fn(&Theme) -> container::Style {
    move |_| container::Style {
        background: bg(c),
        border: Border { color: BORDER_SUBTLE, width: 1.0, radius: 0.0.into() },
        ..Default::default()
    }
}

/// Skeleton bar / placeholder block.
pub fn skeleton(r: f32) -> impl Fn(&Theme) -> container::Style {
    fill_rounded(BG_SURFACE_HOVER, r)
}

pub fn switcher(_: &Theme) -> container::Style {
    // Deliberately not part of the design: high-contrast pill.
    container::Style {
        background: bg(Color::from_rgb8(0xf5, 0xe0, 0x5a)),
        text_color: Some(Color::BLACK),
        border: Border { radius: 999.0.into(), ..Default::default() },
        shadow: Shadow { color: rgba(0, 0, 0, 0.6), offset: Vector::new(0.0, 4.0), blur_radius: 16.0 },
        ..Default::default()
    }
}

/// Track list row: transparent, hl on hover, accent-tinted when current.
pub fn track_row(current: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match (current, status) {
            (true, _) => bg(HL_MED),
            (_, button::Status::Hovered | button::Status::Pressed) => bg(HL_FAINT),
            _ => None,
        };
        button::Style {
            background,
            text_color: TEXT_PRIMARY,
            border: Border { radius: 6.0.into(), ..Default::default() },
            ..Default::default()
        }
    }
}

/// Sidebar / nav item.
pub fn nav_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let background = match (selected, status) {
            (true, _) => bg(HL_MED),
            (_, button::Status::Hovered) => bg(HL_FAINT),
            _ => None,
        };
        button::Style {
            background,
            text_color: if selected { TEXT_PRIMARY } else { TEXT_SECONDARY },
            border: Border { radius: 6.0.into(), ..Default::default() },
            ..Default::default()
        }
    }
}

/// Round ghost icon button (player controls, header arrows).
pub fn icon_button(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if hovered { bg(BORDER_SUBTLE) } else { None },
            text_color: match (active, hovered) {
                (true, _) => ACCENT,
                (_, true) => TEXT_PRIMARY,
                _ => TEXT_SECONDARY,
            },
            border: Border { radius: 999.0.into(), ..Default::default() },
            ..Default::default()
        }
    }
}

/// The big white play/pause circle.
pub fn play_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: bg(if matches!(status, button::Status::Hovered) { TEXT_SECONDARY } else { TEXT_PRIMARY }),
        text_color: BG_BASE,
        border: Border { radius: 999.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Accent pill (hero "Play" button).
pub fn accent_button(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: bg(if matches!(status, button::Status::Hovered) { ACCENT_HOVER } else { ACCENT }),
        text_color: ON_ACCENT,
        border: Border { radius: 999.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// Drawer tab: text, underlined-by-colour when active.
pub fn tab(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: if active { bg(HL_MED) } else if matches!(status, button::Status::Hovered) { bg(HL_FAINT) } else { None },
        text_color: if active { TEXT_PRIMARY } else { TEXT_MUTED },
        border: Border { radius: 999.0.into(), ..Default::default() },
        ..Default::default()
    }
}

/// sone's scrubber: thin track, fill brightens to accent on hover/drag.
pub fn scrubber(_: &Theme, status: slider::Status) -> slider::Style {
    let hot = !matches!(status, slider::Status::Active);
    slider::Style {
        rail: slider::Rail {
            backgrounds: (bg_val(if hot { ACCENT } else { SLIDER_FILL }), bg_val(SLIDER_TRACK)),
            width: if hot { 5.0 } else { 3.0 },
            border: Border { radius: 999.0.into(), ..Default::default() },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: if hot { 6.0 } else { 0.0 } },
            background: bg_val(TEXT_PRIMARY),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

fn bg_val(c: Color) -> Background {
    Background::Color(c)
}

pub fn page_scroll(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let hovered = matches!(status, scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. });
    let rail = scrollable::Rail {
        background: None,
        border: Border::default(),
        scroller: scrollable::Scroller {
            background: bg_val(if hovered { BG_BUTTON_HOVER } else { BG_BUTTON }),
            border: Border { radius: 999.0.into(), ..Default::default() },
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::AutoScroll {
            background: bg_val(BG_ELEVATED),
            border: Border::default(),
            shadow: Shadow::default(),
            icon: TEXT_PRIMARY,
        },
    }
}
