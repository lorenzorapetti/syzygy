//! PROTOTYPE. Variant A — "Faithful sone": 280px sidebar with cover list,
//! 64px header, playlist hero, 60px rows with 40px covers, 90px player bar,
//! now-playing drawer that slides up over everything above the player bar
//! (45% cover / 55% tabs), behind an 80% black backdrop.

use iced::widget::{button, column, container, mouse_area, opaque, responsive, row, scrollable, space, stack};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::icons::{self, I};
use crate::parts::{self, bold, semibold, skel, t};
use crate::tokens::*;
use crate::{App, Message, PLAYLIST_SCROLL, Tab, data};

const ROW_H: f32 = 60.0;
const HERO_H: f32 = 300.0;
const COLS_H: f32 = 44.0;
const LIST_TOP: f32 = HERO_H + COLS_H;

pub fn view(app: &App) -> Element<'_, Message> {
    let main = row![sidebar(app), column![header(), playlist(app)].width(Length::Fill)];
    let upper: Element<_> = if app.drawer.value() || app.drawer.is_animating(app.now) {
        stack![main, drawer(app)].into()
    } else {
        main.into()
    };
    column![container(upper).height(Length::Fill).style(fill(BG_BASE)), player_bar(app)].into()
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let nav = [(I::House, "Home"), (I::Compass, "Explore"), (I::Bell, "Feed"), (I::Search, "Search")]
        .into_iter()
        .enumerate()
        .map(|(i, (glyph, label))| {
            let c = if app.nav == i { TEXT_PRIMARY } else { TEXT_SECONDARY };
            button(row![icons::icon(glyph, 20.0, c), t(label, FS_ROW, c).font(semibold())].spacing(14).align_y(Alignment::Center))
                .on_press(Message::Nav(i))
                .width(Length::Fill)
                .padding([8, 12])
                .style(nav_item(app.nav == i))
                .into()
        });
    let library = app.catalog.library.iter().enumerate().map(|(i, item)| {
        button(
            row![
                app.covers.view(item.cover, 40.0, 4.0, app.now),
                column![t(&item.name, FS_ROW, TEXT_PRIMARY), t(&item.subtitle, FS_SMALL, TEXT_FAINT)].spacing(2),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .on_press(Message::Nav(10 + i))
        .width(Length::Fill)
        .padding(6)
        .style(nav_item(app.nav == 10 + i))
        .into()
    });
    container(
        column![
            t("syzygy", 20.0, TEXT_PRIMARY).font(bold()),
            column(nav).spacing(2),
            row![
                icons::icon(I::Library, 20.0, TEXT_MUTED),
                t("Your Library", FS_ROW, TEXT_MUTED).font(bold()),
                space().width(Length::Fill),
                parts::icon(I::Plus, 18.0, false, Message::Nav(0)),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            row![chip("Playlists"), chip("Albums"), chip("Artists"), chip("Mixes")].spacing(6),
            scrollable(column(library).spacing(2)).style(page_scroll).height(Length::Fill),
        ]
        .spacing(16),
    )
    .padding(Padding::new(16.0).right(8.0))
    .width(280)
    .height(Length::Fill)
    .style(fill(BG_SIDEBAR))
    .into()
}

fn chip(label: &str) -> Element<'_, Message> {
    container(t(label, FS_SMALL, TEXT_SECONDARY)).padding([4, 10]).style(fill_rounded(BG_INSET, 999.0)).into()
}

fn header() -> Element<'static, Message> {
    let arrow = |i| container(icons::icon(i, 20.0, TEXT_MUTED)).center(32).style(fill_rounded(BG_INSET, 999.0));
    container(
        row![
            arrow(I::ChevronLeft),
            arrow(I::ChevronRight),
            container(
                row![icons::icon(I::Search, 16.0, TEXT_FAINT), t("Search tracks, albums, artists…", FS_ROW, TEXT_FAINT)]
                    .spacing(10)
                    .align_y(Alignment::Center),
            )
                .padding([8, 16])
                .width(360)
                .style(fill_rounded(BG_INSET, 999.0)),
            space().width(Length::Fill),
            container(t("L", FS_ROW, ON_ACCENT).font(bold())).center(32).style(fill_rounded(ACCENT, 999.0)),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .center_y(64)
    .padding([0, 24])
    .into()
}

fn playlist(app: &App) -> Element<'_, Message> {
    let hero = container(
        row![
            app.covers.view(9_999, 232.0, 8.0, app.now),
            column![
                t("PLAYLIST", FS_SMALL, TEXT_SECONDARY).font(bold()),
                t("Everything I've Ever Loved", FS_HERO, TEXT_PRIMARY).font(bold()),
                t("Lorenzo · 5,000 tracks · 341 hr 12 min", FS_BODY, TEXT_MUTED),
                space().height(8),
                row![
                    button(
                        container(
                            row![icons::filled(I::Play, 16.0, ON_ACCENT), t("Play", FS_ROW, ON_ACCENT).font(bold())]
                                .spacing(8)
                                .align_y(Alignment::Center),
                        )
                        .padding([8, 22]),
                    )
                    .on_press(Message::Play(0))
                    .padding(0)
                    .style(accent_button),
                    parts::icon(I::Shuffle, 18.0, app.shuffle, Message::ToggleShuffle),
                    parts::icon(I::Heart, 18.0, false, Message::Nav(0)),
                    parts::icon(I::Ellipsis, 22.0, false, Message::Nav(0)),
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            ]
            .spacing(6),
        ]
        .spacing(24)
        .align_y(Alignment::End),
    )
    .height(HERO_H)
    .padding(Padding::new(24.0).top(16.0))
    .align_y(Alignment::End);

    let cols = container(
        row![
            t("#", FS_SMALL, TEXT_MUTED).width(36),
            t("TITLE", FS_SMALL, TEXT_MUTED).width(Length::FillPortion(4)),
            t("ALBUM", FS_SMALL, TEXT_MUTED).width(Length::FillPortion(2)),
            t("TIME", FS_SMALL, TEXT_MUTED).width(56).align_x(Alignment::End),
        ]
        .spacing(16),
    )
    .padding([12, 40])
    .height(COLS_H);

    let range = app.row_window(ROW_H, LIST_TOP);
    let n = app.catalog.tracks.len();
    let (above, below) = (range.start as f32 * ROW_H, (n - range.end) as f32 * ROW_H);
    let rows = range.map(|i| track_row_a(app, i));

    scrollable(
        column![
            hero,
            cols,
            space().height(above),
            container(column(rows)).padding([0, 24]),
            space().height(below + 120.0),
        ]
        .width(Length::Fill),
    )
    .id(PLAYLIST_SCROLL)
    .on_scroll(Message::Scrolled)
    .style(page_scroll)
    .height(Length::Fill)
    .into()
}

fn track_row_a(app: &App, i: usize) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[i];
    let current = i == app.current;
    let num: Element<_> = if current {
        icons::filled(if app.playing { I::Pause } else { I::Play }, 14.0, ACCENT).into()
    } else {
        t((i + 1).to_string(), FS_ROW, TEXT_MUTED).into()
    };
    let (q, qc) = parts::quality_label(tr.quality);
    let mut meta = row![].spacing(6).align_y(Alignment::Center);
    if tr.explicit {
        meta = meta.push(parts::badge("E", TEXT_MUTED));
    }
    if q != "HIGH" {
        meta = meta.push(parts::badge(q, qc));
    }
    meta = meta.push(t(&tr.artist, FS_BODY, TEXT_MUTED));
    button(
        row![
            container(num).width(36),
            row![
                app.covers.view(tr.cover, 40.0, 4.0, app.now),
                column![t(&tr.title, FS_ROW, if current { ACCENT } else { TEXT_PRIMARY }), meta].spacing(4),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .width(Length::FillPortion(4)),
            t(&tr.album, FS_ROW, TEXT_MUTED).width(Length::FillPortion(2)),
            t(data::fmt_time(tr.duration), FS_ROW, TEXT_MUTED).width(56).align_x(Alignment::End),
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Play(i))
    .height(ROW_H)
    .width(Length::Fill)
    .padding([10, 16])
    .style(track_row(current))
    .into()
}

fn player_bar(app: &App) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[app.current];
    let (q, qc) = parts::quality_label(tr.quality);
    let left = row![
        mouse_area(app.covers.view(tr.cover, 64.0, 6.0, app.now)).on_press(Message::ToggleDrawer),
        column![
            t(&tr.title, FS_BODY, TEXT_PRIMARY).font(semibold()),
            t(&tr.artist, FS_TINY, TEXT_SECONDARY),
            row![parts::badge(q, qc), t("24-bit · 96 kHz", 10.0, TEXT_FAINT)].spacing(6).align_y(Alignment::Center),
        ]
        .spacing(3),
        parts::icon(I::Heart, 16.0, false, Message::Nav(0)),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .width(Length::FillPortion(3));

    let center = column![parts::transport(app, 36.0), parts::seek_bar(app)]
        .spacing(4)
        .align_x(Alignment::Center)
        .max_width(600)
        .width(Length::FillPortion(4));

    let right = row![
        space().width(Length::Fill),
        parts::icon(I::ListMusic, 16.0, app.drawer.value(), Message::ToggleDrawer),
        parts::volume_icon(app),
        parts::volume(app),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .width(Length::FillPortion(3));

    container(row![left, center, right].align_y(Alignment::Center))
        .center_y(90)
        .padding([0, 16])
        .style(top_border(BG_ELEVATED))
        .into()
}

fn drawer(app: &App) -> Element<'_, Message> {
    let p = app.drawer.interpolate(0.0f32, 1.0, app.now);
    let backdrop = opaque(
        mouse_area(container(space()).width(Length::Fill).height(Length::Fill).style(fill(Color { a: 0.8 * p, ..Color::BLACK })))
            .on_press(Message::ToggleDrawer),
    );
    let panel = responsive(move |size| {
        container(column![space().height(size.height * (1.0 - p)), container(opaque(drawer_body(app))).height(size.height)])
            .clip(true)
            .into()
    });
    stack![backdrop, panel].into()
}

fn drawer_body(app: &App) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[app.current];
    let left = column![
        container(app.covers.view_fill(tr.cover, 10.0, app.now)).width(Length::Fill).max_width(560).height(560),
        t(&tr.title, 22.0, TEXT_PRIMARY).font(bold()),
        t(&tr.artist, 15.0, TEXT_MUTED),
    ]
    .spacing(16)
    .padding(40)
    .align_x(Alignment::Center)
    .width(Length::Fill);

    let tabs = row![
        tab_btn(app, "Queue", Tab::Queue),
        tab_btn(app, "Lyrics", Tab::Lyrics),
        tab_btn(app, "Credits", Tab::Credits),
        space().width(Length::Fill),
        parts::icon(I::X, 18.0, false, Message::ToggleDrawer),
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    let right = container(column![tabs, scrollable(tab_content(app)).style(page_scroll).height(Length::Fill)].spacing(12))
        .padding(Padding::new(24.0).top(20.0))
        .width(Length::FillPortion(55))
        .height(Length::Fill);

    container(row![container(left).center_y(Length::Fill).width(Length::FillPortion(45)), right])
        .width(Length::Fill)
        .height(Length::Fill)
        .style(fill(BG_BASE))
        .into()
}

fn tab_btn<'a>(app: &App, label: &'a str, tab: Tab) -> Element<'a, Message> {
    button(t(label, FS_BODY, if app.tab == tab { TEXT_PRIMARY } else { TEXT_MUTED }).font(semibold()))
        .on_press(Message::DrawerTab(tab))
        .padding([6, 14])
        .style(crate::tokens::tab(app.tab == tab))
        .into()
}

pub fn tab_content(app: &App) -> Element<'_, Message> {
    match app.tab {
        Tab::Queue => {
            let label = |s| t(s, FS_BODY, TEXT_MUTED).font(bold());
            let next = (app.current + 1..(app.current + 31).min(app.catalog.tracks.len())).map(|i| queue_row(app, i));
            column![label("NOW PLAYING"), queue_row(app, app.current), label("NEXT UP"), column(next).spacing(2)]
                .spacing(10)
                .into()
        }
        Tab::Lyrics => column(
            [65, 45, 72, 55, 80, 50, 60, 40, 70, 52, 66, 38]
                .into_iter()
                .map(|w| row![parts::skel(Length::FillPortion(w), 22.0), space().width(Length::FillPortion(100 - w))].into()),
        )
        .spacing(14)
        .width(Length::Fill)
        .into(),
        Tab::Credits => column((0..8).map(|i| {
            column![skel(Length::Fixed(80.0 + (i * 13 % 60) as f32), 11.0), skel(Length::Fixed(140.0 + (i * 29 % 90) as f32), 13.0)]
                .spacing(6)
                .into()
        }))
        .spacing(18)
        .into(),
    }
}

fn queue_row(app: &App, i: usize) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[i];
    button(
        row![
            app.covers.view(tr.cover, 40.0, 4.0, app.now),
            column![
                t(&tr.title, FS_BODY, if i == app.current { ACCENT } else { TEXT_PRIMARY }),
                t(&tr.artist, FS_TINY, TEXT_MUTED)
            ]
            .spacing(2)
            .width(Length::Fill),
            t(data::fmt_time(tr.duration), FS_SMALL, TEXT_FAINT),
        ]
        .spacing(12)
        .padding(Padding::ZERO.right(14.0))
        .align_y(Alignment::Center),
    )
    .on_press(Message::Play(i))
    .width(Length::Fill)
    .padding(6)
    .style(track_row(i == app.current))
    .into()
}
