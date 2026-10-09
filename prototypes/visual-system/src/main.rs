//! PROTOTYPE — throwaway. Issue 09: what should syzygy look like in iced?
//!
//! Three variants of the Shell (sidebar, player bar, now-playing drawer, a
//! 5000-track playlist), all on one token set (`tokens.rs`), switchable from a
//! floating bar: ← / → (or the arrows on the bar). `W` toggles windowed vs.
//! full track list, `D` toggles the drawer, Space plays/pauses.
//!
//!   cargo run --release --manifest-path prototypes/visual-system/Cargo.toml -- --variant B

mod covers;
mod data;
mod icons;
mod tokens;
mod variant_a;
mod variant_b;
mod variant_c;

use std::cell::Cell;
use std::time::{Duration, Instant};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{button, column, container, row, scrollable, space, stack, text};
use iced::{Animation, Element, Font, Length, Subscription, Task, window};

use covers::{CoverMsg, Covers};
use data::Catalog;

pub const VARIANTS: [(&str, &str); 3] = [
    ("A", "Faithful sone"),
    ("B", "Rail + docked panel"),
    ("C", "Top transport + player page"),
];

pub const PLAYLIST_SCROLL: &str = "playlist";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Queue,
    Lyrics,
    Credits,
}

#[derive(Debug, Clone)]
pub enum Message {
    PrevVariant,
    NextVariant,
    ToggleWindowing,
    Scrolled(scrollable::Viewport),
    Cover(CoverMsg),
    Frame(Instant),
    Tick,
    Play(usize),
    TogglePlay,
    SkipNext,
    SkipPrev,
    SeekPreview(f32),
    SeekCommit,
    Volume(f32),
    ToggleDrawer,
    DrawerTab(Tab),
    Nav(usize),
    ToggleShuffle,
    CycleRepeat,
    Shoot,
    Shot(window::Screenshot),
}

pub struct App {
    pub variant: usize,
    pub catalog: Catalog,
    pub covers: Covers,
    pub now: Instant,
    pub windowed: bool,
    pub scroll_y: f32,
    pub viewport_h: f32,
    pub current: usize,
    pub playing: bool,
    pub position: f32,
    pub seek_preview: Option<f32>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat: u8,
    pub drawer: Animation<bool>,
    pub tab: Tab,
    pub nav: usize,
    /// (view build time, track rows built) — surfaced in the switcher.
    pub cost: Cell<(Duration, usize)>,
    /// `--shot <path>`: screenshot the window after 4s and exit.
    pub shot: Option<String>,
}

fn main() -> iced::Result {
    iced::application(App::boot, App::update, App::view)
        .title(|app: &App| format!("syzygy visual prototype — {}", VARIANTS[app.variant].1))
        .theme(|_: &App| tokens::theme())
        .subscription(App::subscription)
        .window_size((1440.0, 900.0))
        .default_font(Font::with_name("Inter"))
        .run()
}

impl App {
    fn boot() -> Self {
        let args: Vec<String> = std::env::args().collect();
        let variant = args
            .iter()
            .position(|a| a == "--variant")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| VARIANTS.iter().position(|(k, _)| k.eq_ignore_ascii_case(v)))
            .unwrap_or(0);
        Self {
            variant,
            catalog: Catalog::fake(),
            covers: Covers::default(),
            now: Instant::now(),
            windowed: !args.iter().any(|a| a == "--full"),
            scroll_y: 0.0,
            viewport_h: 800.0,
            current: 3,
            playing: false,
            position: 42.0,
            seek_preview: None,
            volume: 0.7,
            shuffle: false,
            repeat: 0,
            drawer: Animation::new(false).duration(Duration::from_millis(250)).easing(iced::animation::Easing::EaseOutCubic),
            tab: Tab::Queue,
            nav: 0,
            cost: Cell::new((Duration::ZERO, 0)),
            shot: args.iter().position(|a| a == "--shot").and_then(|i| args.get(i + 1)).cloned(),
        }.with_drawer(args.iter().any(|a| a == "--drawer"))
    }

    fn with_drawer(mut self, open: bool) -> Self {
        if open {
            self.drawer = Animation::new(true);
        }
        self
    }

    pub fn track_len(&self) -> f32 {
        self.catalog.tracks[self.current].duration as f32
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        self.now = Instant::now();
        match msg {
            Message::PrevVariant | Message::NextVariant => {
                let n = VARIANTS.len();
                self.variant = if matches!(msg, Message::NextVariant) { (self.variant + 1) % n } else { (self.variant + n - 1) % n };
                self.scroll_y = 0.0;
                return iced::widget::operation::snap_to(PLAYLIST_SCROLL, scrollable::RelativeOffset::START);
            }
            Message::ToggleWindowing => self.windowed = !self.windowed,
            Message::Scrolled(vp) => {
                self.scroll_y = vp.absolute_offset().y;
                self.viewport_h = vp.bounds().height;
            }
            Message::Cover(m) => return self.covers.update(m, self.now),
            Message::Frame(now) => self.now = now,
            Message::Tick => {
                if self.playing && self.seek_preview.is_none() {
                    self.position += 0.25;
                    if self.position >= self.track_len() {
                        self.current = (self.current + 1) % self.catalog.tracks.len();
                        self.position = 0.0;
                    }
                }
            }
            Message::Play(i) => {
                self.current = i;
                self.position = 0.0;
                self.playing = true;
            }
            Message::TogglePlay => self.playing = !self.playing,
            Message::SkipNext => {
                self.current = (self.current + 1) % self.catalog.tracks.len();
                self.position = 0.0;
            }
            Message::SkipPrev => {
                self.current = self.current.saturating_sub(1);
                self.position = 0.0;
            }
            Message::SeekPreview(v) => self.seek_preview = Some(v),
            Message::SeekCommit => {
                if let Some(v) = self.seek_preview.take() {
                    self.position = v;
                }
            }
            Message::Volume(v) => self.volume = v,
            Message::ToggleDrawer => {
                let open = !self.drawer.value();
                self.drawer.go_mut(open, self.now);
            }
            Message::DrawerTab(t) => self.tab = t,
            Message::Nav(i) => self.nav = i,
            Message::ToggleShuffle => self.shuffle = !self.shuffle,
            Message::CycleRepeat => self.repeat = (self.repeat + 1) % 3,
            Message::Shoot => return window::latest().and_then(window::screenshot).map(Message::Shot),
            Message::Shot(s) => {
                if let Some(path) = &self.shot {
                    image::save_buffer(path, &s.rgba, s.size.width, s.size.height, image::ColorType::Rgba8).expect("save shot");
                }
                return iced::exit();
            }
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        let keys = keyboard::listen().filter_map(|e| match e {
            keyboard::Event::KeyPressed { key, .. } => match key.as_ref() {
                Key::Named(Named::ArrowLeft) => Some(Message::PrevVariant),
                Key::Named(Named::ArrowRight) => Some(Message::NextVariant),
                Key::Named(Named::Space) => Some(Message::TogglePlay),
                Key::Character("w") => Some(Message::ToggleWindowing),
                Key::Character("d") => Some(Message::ToggleDrawer),
                _ => None,
            },
            _ => None,
        });
        let mut subs = vec![keys];
        if self.shot.is_some() {
            subs.push(iced::time::every(Duration::from_secs(4)).map(|_| Message::Shoot));
        }
        if self.playing {
            subs.push(iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick));
        }
        if self.drawer.is_animating(self.now) || self.covers.is_animating(self.now) {
            subs.push(window::frames().map(Message::Frame));
        }
        Subscription::batch(subs)
    }

    fn view(&self) -> Element<'_, Message> {
        let start = Instant::now();
        let page = match self.variant {
            0 => variant_a::view(self),
            1 => variant_b::view(self),
            _ => variant_c::view(self),
        };
        let rows = self.cost.get().1;
        self.cost.set((start.elapsed(), rows));
        stack![page, self.switcher()].into()
    }

    /// Which track rows to build, given row height and the y where row 0 starts
    /// inside the scrollable. Windowed: visible ± overscan. Full: all of them.
    pub fn row_window(&self, row_h: f32, list_top: f32) -> std::ops::Range<usize> {
        let n = self.catalog.tracks.len();
        let range = if self.windowed {
            let overscan = 8;
            let first = (((self.scroll_y - list_top) / row_h).floor().max(0.0) as usize).saturating_sub(overscan);
            let visible = (self.viewport_h / row_h).ceil() as usize + 2 * overscan;
            first.min(n)..(first + visible).min(n)
        } else {
            0..n
        };
        self.cost.set((self.cost.get().0, range.len()));
        range
    }

    fn switcher(&self) -> Element<'_, Message> {
        let (ready, loading) = self.covers.stats();
        let (cost, rows) = self.cost.get();
        let (key, name) = VARIANTS[self.variant];
        let arrow = |i, msg| button(icons::icon(i, 16.0, iced::Color::BLACK)).on_press(msg).style(button::text).padding([2, 8]);
        let bar = container(
            row![
                arrow(icons::I::ChevronLeft, Message::PrevVariant),
                text(format!("{key} · {name}")).size(13).font(Font { weight: iced::font::Weight::Bold, ..Font::with_name("Inter") }),
                arrow(icons::I::ChevronRight, Message::NextVariant),
                text("|").size(13),
                button(text(if self.windowed { "list: windowed (W)" } else { "list: FULL (W)" }).size(12).color(iced::Color::BLACK))
                    .on_press(Message::ToggleWindowing)
                    .style(button::text)
                    .padding([2, 4]),
                text(format!(
                    "rows {rows} · view {:.2}ms · covers {ready} ready / {loading} loading / {} evicted · scroll {:.0}",
                    cost.as_secs_f64() * 1000.0,
                    self.covers.evicted,
                    self.scroll_y
                ))
                .size(12),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center),
        )
        .padding([6, 14])
        .style(tokens::switcher);
        column![space().height(Length::Fill), container(bar).center_x(Length::Fill), space().height(110)].into()
    }
}

/// Shared bits the variants may use. Kept tiny on purpose: each variant owns
/// its layout.
pub mod parts {
    use super::*;
    use iced::widget::{Text, slider};

    pub fn t<'a>(s: impl text::IntoFragment<'a>, size: f32, color: iced::Color) -> Text<'a> {
        text(s).size(size).color(color).wrapping(text::Wrapping::None)
    }

    pub fn bold() -> Font {
        Font { weight: iced::font::Weight::Bold, ..Font::with_name("Inter") }
    }

    pub fn semibold() -> Font {
        Font { weight: iced::font::Weight::Semibold, ..Font::with_name("Inter") }
    }

    pub use crate::icons::I;

    /// Round ghost button around a Lucide icon (sone: text-secondary, white on
    /// hover, accent when active).
    pub fn icon<'a>(i: I, size: f32, active: bool, msg: Message) -> Element<'a, Message> {
        let glyph = if active {
            icons::icon(i, size, tokens::ACCENT)
        } else {
            icons::hover(i, size, tokens::TEXT_SECONDARY, tokens::TEXT_PRIMARY)
        };
        button(container(glyph).center(size + 14.0))
            .on_press(msg)
            .padding(0)
            .style(tokens::icon_button(active))
            .into()
    }

    pub fn seek_bar(app: &App) -> Element<'_, Message> {
        let len = app.track_len();
        let pos = app.seek_preview.unwrap_or(app.position).min(len);
        row![
            t(data::fmt_time(pos as u32), tokens::FS_TINY, tokens::TEXT_SECONDARY),
            slider(0.0..=len, pos, Message::SeekPreview)
                .on_release(Message::SeekCommit)
                .step(0.25_f32)
                .height(17)
                .style(tokens::scrubber),
            t(data::fmt_time(len as u32), tokens::FS_TINY, tokens::TEXT_SECONDARY),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// sone's VolumeIcon: VolumeX at 0, Volume1 below half, Volume2 otherwise.
    pub fn volume_icon<'a>(app: &App) -> Element<'a, Message> {
        let i = if app.volume == 0.0 { I::VolumeX } else if app.volume < 0.5 { I::Volume1 } else { I::Volume2 };
        icons::icon(i, 16.0, tokens::TEXT_SECONDARY).into()
    }

    pub fn volume(app: &App) -> Element<'_, Message> {
        slider(0.0..=1.0, app.volume, Message::Volume).step(0.01_f32).width(100).style(tokens::scrubber).into()
    }

    pub fn transport(app: &App, big: f32) -> Element<'_, Message> {
        // Skip: filled 18px icons in a 32px ghost circle, as in sone.
        let skip = |i, msg| {
            button(container(icons::filled(i, 18.0, tokens::TEXT_SECONDARY)).center(32))
                .on_press(msg)
                .padding(0)
                .style(tokens::icon_button(false))
        };
        // Repeat: accent dot underneath when on, a "1" badge for repeat-one.
        let mut repeat = stack![icon(I::Repeat, 15.0, app.repeat > 0, Message::CycleRepeat)];
        if app.repeat > 0 {
            repeat = repeat.push(
                container(container(space()).width(4).height(4).style(tokens::fill_rounded(tokens::ACCENT, 2.0)))
                    .center_x(29)
                    .align_bottom(29),
            );
        }
        if app.repeat == 2 {
            repeat = repeat.push(
                container(
                    container(t("1", 7.0, tokens::ON_ACCENT).font(bold()))
                        .center(12)
                        .style(tokens::fill_rounded(tokens::ACCENT, 6.0)),
                )
                .align_right(29),
            );
        }
        let play = if app.playing { I::Pause } else { I::Play };
        row![
            icon(I::Shuffle, 15.0, app.shuffle, Message::ToggleShuffle),
            skip(I::SkipBack, Message::SkipPrev),
            button(container(icons::filled(play, 17.0, tokens::BG_BASE)).center(big))
                .on_press(Message::TogglePlay)
                .padding(0)
                .style(tokens::play_button),
            skip(I::SkipForward, Message::SkipNext),
            repeat,
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .into()
    }

    /// Quality / explicit badge, sone-style.
    pub fn badge<'a>(label: &'a str, color: iced::Color) -> Element<'a, Message> {
        container(t(label, 9.0, color).font(bold()))
            .padding([1, 4])
            .style(tokens::fill_rounded(tokens::HL_MED, 3.0))
            .into()
    }

    pub fn quality_label(q: &str) -> (&'static str, iced::Color) {
        match q {
            "HI_RES_LOSSLESS" => ("HI-RES", tokens::WARNING),
            "LOSSLESS" => ("LOSSLESS", tokens::ACCENT),
            _ => ("HIGH", tokens::TEXT_MUTED),
        }
    }

    /// Skeleton line of `w` fraction-of-fill width.
    pub fn skel<'a>(width: impl Into<Length>, h: f32) -> Element<'a, Message> {
        container(space()).width(width).height(h).style(tokens::skeleton(4.0)).into()
    }
}
