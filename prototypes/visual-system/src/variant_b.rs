//! PROTOTYPE. Variant B — "Rail + docked panel": 60px icon rail (library as
//! cover tiles only), compact hero, dense 36px text-only rows with stripes,
//! now-playing as a docked right panel that pushes content instead of
//! covering it, 72px player bar with the scrubber as a full-width top edge.

use iced::widget::{button, column, container, mouse_area, row, rule, scrollable, slider, space, tooltip};
use iced::{Alignment, Element, Length, Padding};

use crate::icons::{self, I};
use crate::parts::{self, bold, semibold, t};
use crate::tokens::*;
use crate::{App, Message, PLAYLIST_SCROLL, Tab, data, variant_a};

const ROW_H: f32 = 36.0;
const HERO_H: f32 = 176.0;
const COLS_H: f32 = 36.0;
const PANEL_W: f32 = 360.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.drawer.interpolate(0.0f32, 1.0, app.now);
    let mut body = row![rail(app), content(app)];
    if p > 0.0 {
        body = body.push(panel(app, p));
    }
    column![container(body).height(Length::Fill).style(fill(BG_BASE)), player_bar(app)].into()
}

fn rail(app: &App) -> Element<'_, Message> {
    let nav = [(I::House, "Home"), (I::Compass, "Explore"), (I::Bell, "Feed"), (I::Search, "Search")].into_iter().enumerate().map(|(i, (g, label))| {
        tooltip(
            button(container(icons::icon(g, 20.0, if app.nav == i { TEXT_PRIMARY } else { TEXT_MUTED })).center(40))
                .on_press(Message::Nav(i))
                .padding(0)
                .style(nav_item(app.nav == i)),
            container(t(label, FS_SMALL, TEXT_PRIMARY)).padding([4, 8]).style(fill_rounded(BG_BUTTON, 4.0)),
            tooltip::Position::Right,
        )
        .into()
    });
    let tiles = app.catalog.library.iter().enumerate().map(|(i, item)| {
        tooltip(
            button(app.covers.view(item.cover, 40.0, 6.0, app.now))
                .on_press(Message::Nav(10 + i))
                .padding(2)
                .style(nav_item(app.nav == 10 + i)),
            container(column![t(&item.name, FS_BODY, TEXT_PRIMARY), t(&item.subtitle, FS_TINY, TEXT_MUTED)])
                .padding([6, 10])
                .style(fill_rounded(BG_BUTTON, 6.0)),
            tooltip::Position::Right,
        )
        .into()
    });
    container(
        column![
            container(icons::icon(I::Library, 22.0, ACCENT)).center_x(Length::Fill),
            column(nav).spacing(4).align_x(Alignment::Center),
            rule::horizontal(1),
            scrollable(column(tiles).spacing(6).align_x(Alignment::Center).width(Length::Fill))
                .direction(scrollable::Direction::Vertical(scrollable::Scrollbar::hidden()))
                .height(Length::Fill),
        ]
        .spacing(14)
        .align_x(Alignment::Center),
    )
    .padding([14, 8])
    .width(60)
    .height(Length::Fill)
    .style(fill(BG_SIDEBAR))
    .into()
}

fn content(app: &App) -> Element<'_, Message> {
    let hero = container(
        row![
            app.covers.view(9_999, 128.0, 6.0, app.now),
            column![
                t("Everything I've Ever Loved", 28.0, TEXT_PRIMARY).font(bold()),
                t("Playlist · Lorenzo · 5,000 tracks · 341 hr", FS_BODY, TEXT_MUTED),
                row![
                    button(container(icons::filled(I::Play, 16.0, ON_ACCENT)).center(40)).on_press(Message::Play(0)).padding(0).style(accent_button),
                    parts::icon(I::Shuffle, 16.0, app.shuffle, Message::ToggleShuffle),
                    parts::icon(I::Heart, 16.0, false, Message::Nav(0)),
                    space().width(Length::Fill),
                    container(row![icons::icon(I::Search, 14.0, TEXT_FAINT), t("Filter", FS_BODY, TEXT_FAINT)].spacing(8).align_y(Alignment::Center)).padding([6, 12]).width(200).style(fill_rounded(BG_INSET, 6.0)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            ]
            .spacing(8),
        ]
        .spacing(20)
        .align_y(Alignment::Center),
    )
    .height(HERO_H)
    .padding([24, 24]);

    let head = |s, w: Length| t(s, FS_TINY, TEXT_FAINT).font(bold()).width(w);
    let cols = container(
        row![
            head("#", Length::Fixed(44.0)),
            head("TITLE", Length::FillPortion(4)),
            head("ARTIST", Length::FillPortion(3)),
            head("ALBUM", Length::FillPortion(3)),
            head("QUALITY", Length::Fixed(64.0)),
            head("TIME", Length::Fixed(44.0)),
        ]
        .spacing(12),
    )
    .padding([10, 32])
    .height(COLS_H);

    let range = app.row_window(ROW_H, HERO_H + COLS_H);
    let n = app.catalog.tracks.len();
    let (above, below) = (range.start as f32 * ROW_H, (n - range.end) as f32 * ROW_H);
    let rows = range.map(|i| dense_row(app, i));

    scrollable(
        column![hero, cols, rule::horizontal(1), space().height(above), container(column(rows)).padding([0, 20]), space().height(below + 120.0)]
            .width(Length::Fill),
    )
    .id(PLAYLIST_SCROLL)
    .on_scroll(Message::Scrolled)
    .style(page_scroll)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn dense_row(app: &App, i: usize) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[i];
    let current = i == app.current;
    let (q, qc) = parts::quality_label(tr.quality);
    let mut title = row![t(&tr.title, FS_BODY, if current { ACCENT } else { TEXT_PRIMARY })].spacing(6).align_y(Alignment::Center);
    if tr.explicit {
        title = title.push(parts::badge("E", TEXT_MUTED));
    }
    let stripe = i % 2 == 1 && !current;
    let r = button(
        row![
            container(if current {
                Element::<Message>::from(icons::filled(if app.playing { I::Pause } else { I::Play }, 12.0, ACCENT))
            } else {
                t((i + 1).to_string(), FS_SMALL, TEXT_FAINT).into()
            })
            .width(44),
            container(title).width(Length::FillPortion(4)).clip(true),
            t(&tr.artist, FS_BODY, TEXT_MUTED).width(Length::FillPortion(3)),
            t(&tr.album, FS_BODY, TEXT_MUTED).width(Length::FillPortion(3)),
            t(q, 10.0, qc).font(bold()).width(64),
            t(data::fmt_time(tr.duration), FS_SMALL, TEXT_MUTED).width(44),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Play(i))
    .height(ROW_H)
    .width(Length::Fill)
    .padding([0, 12])
    .style(track_row(current));
    if stripe { container(r).style(fill_rounded(HL_FAINT, 4.0)).into() } else { r.into() }
}

fn panel(app: &App, p: f32) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[app.current];
    let seg = |label, tab| {
        button(t(label, FS_SMALL, if app.tab == tab { TEXT_PRIMARY } else { TEXT_MUTED }).font(semibold()))
            .on_press(Message::DrawerTab(tab))
            .padding([5, 12])
            .style(crate::tokens::tab(app.tab == tab))
    };
    container(
        column![
            row![t("Now playing", FS_ROW, TEXT_PRIMARY).font(bold()), space().width(Length::Fill), parts::icon(I::X, 14.0, false, Message::ToggleDrawer)]
                .align_y(Alignment::Center),
            container(app.covers.view_fill(tr.cover, 8.0, app.now)).width(Length::Fill).height(PANEL_W - 40.0),
            column![t(&tr.title, 18.0, TEXT_PRIMARY).font(bold()), t(&tr.artist, FS_ROW, TEXT_MUTED)].spacing(4),
            container(row![seg("Queue", Tab::Queue), seg("Lyrics", Tab::Lyrics), seg("Credits", Tab::Credits)].spacing(2))
                .padding(3)
                .style(fill_rounded(BG_INSET, 999.0)),
            scrollable(variant_a::tab_content(app)).style(page_scroll).height(Length::Fill),
        ]
        .spacing(14),
    )
    .padding(20)
    .width(PANEL_W * p)
    .height(Length::Fill)
    .clip(true)
    .style(fill(BG_ELEVATED))
    .into()
}

fn player_bar(app: &App) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[app.current];
    let len = app.track_len();
    let pos = app.seek_preview.unwrap_or(app.position).min(len);
    let edge = slider(0.0..=len, pos, Message::SeekPreview).on_release(Message::SeekCommit).step(0.25_f32).height(10).style(scrubber);
    let bar = row![
        row![
            mouse_area(app.covers.view(tr.cover, 44.0, 4.0, app.now)).on_press(Message::ToggleDrawer),
            column![t(&tr.title, FS_BODY, TEXT_PRIMARY).font(semibold()), t(&tr.artist, FS_TINY, TEXT_SECONDARY)].spacing(2),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(1)),
        parts::transport(app, 32.0),
        row![
            space().width(Length::Fill),
            t(format!("{} / {}", data::fmt_time(pos as u32), data::fmt_time(len as u32)), FS_TINY, TEXT_MUTED),
            parts::volume(app),
            parts::icon(I::ListMusic, 16.0, app.drawer.value(), Message::ToggleDrawer),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .width(Length::FillPortion(1)),
    ]
    .align_y(Alignment::Center)
    .padding(Padding::from([0, 16]));
    container(column![edge, container(bar).center_y(Length::Fill)]).height(72).style(fill(BG_ELEVATED)).into()
}
