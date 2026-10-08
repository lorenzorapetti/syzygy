//! The look: sone's default dark theme ("Violet Night", accent `#A855F7` on
//! `#130F1A`) as run through sone's `deriveTheme`, hardcoded as tokens.
//!
//! `Theme::custom` only feeds iced's defaults (scrollbars, text inputs and
//! such). Every widget syzygy styles gets a plain style fn that reads these
//! tokens; the ones shared across modules live here.

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme, color};

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a,
    }
}

pub const BG_BASE: Color = color!(0x130F1A);
pub const BG_SURFACE: Color = color!(0x1c1627);
pub const BG_SURFACE_HOVER: Color = color!(0x282037);
pub const BG_ELEVATED: Color = color!(0x191422);
pub const BG_SIDEBAR: Color = color!(0x0e0b14);
pub const BG_INSET: Color = color!(0x241c31);
pub const BG_BUTTON: Color = color!(0x342947);
pub const BG_BUTTON_HOVER: Color = color!(0x403257);

pub const ACCENT: Color = color!(0xA855F7);

pub const TEXT_PRIMARY: Color = Color::WHITE;
pub const TEXT_SECONDARY: Color = color!(0xb3b3b3);
pub const TEXT_MUTED: Color = color!(0xa6a6a6);
pub const TEXT_DISABLED: Color = color!(0x535353);

pub const BORDER_SUBTLE: Color = rgba(255, 255, 255, 0.06);
pub const HL_FAINT: Color = rgba(255, 255, 255, 0.04);

pub const SUCCESS: Color = color!(0x1ed760);
pub const WARNING: Color = color!(0xffa726);
pub const ERROR: Color = color!(0xff6666);

/// iced's defaults, in the tokens' colours.
pub fn theme(name: &str) -> Theme {
    Theme::custom(
        name.to_string(),
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

pub fn rounded(radius: f32) -> Border {
    Border {
        radius: radius.into(),
        ..Border::default()
    }
}

/// Where a cover, an avatar or the like goes until it's loaded.
pub fn placeholder(_theme: &Theme, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(BG_SURFACE_HOVER)),
        border: rounded(radius),
        ..container::Style::default()
    }
}

/// A round ghost button around an icon: back and forward, closing a toast.
pub fn icon_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Some(BORDER_SUBTLE.into()),
        button::Status::Active | button::Status::Disabled => None,
    };
    button::Style {
        background,
        text_color: TEXT_PRIMARY,
        border: rounded(999.0),
        ..button::Style::default()
    }
}

/// The tab showing, in a row of pill tabs (Home's feed tabs and the like).
pub fn selected_tab(_theme: &Theme, _status: button::Status) -> button::Style {
    pill(TEXT_PRIMARY, BG_BASE)
}

pub fn unselected_tab(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => BG_BUTTON_HOVER,
        _ => BG_BUTTON,
    };
    pill(background, TEXT_PRIMARY)
}

/// A pill button, as Refresh under a playlist's recommendations.
pub fn pill_button(_theme: &Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered | button::Status::Pressed => pill(BG_BUTTON_HOVER, TEXT_PRIMARY),
        button::Status::Active => pill(BG_BUTTON, TEXT_PRIMARY),
        button::Status::Disabled => pill(BG_BUTTON, TEXT_DISABLED),
    }
}

/// A rounded field on an inset background, as a Page's filter.
pub fn filter_input(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border = match status {
        text_input::Status::Focused { .. } => BORDER_SUBTLE,
        _ => Color::TRANSPARENT,
    };
    text_input::Style {
        background: Background::Color(BG_INSET),
        border: Border {
            color: border,
            width: 1.0,
            radius: 18.0.into(),
        },
        icon: TEXT_MUTED,
        placeholder: TEXT_MUTED,
        value: TEXT_PRIMARY,
        selection: ACCENT.scale_alpha(0.4),
    }
}

fn pill(background: Color, text_color: Color) -> button::Style {
    button::Style {
        background: Some(background.into()),
        text_color,
        border: rounded(18.0),
        ..button::Style::default()
    }
}
