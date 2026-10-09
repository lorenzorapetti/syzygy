//! PROTOTYPE. Variant C — "Top transport + player page": the player lives in
//! a 64px strip at the top, the sidebar is a text-only tree, the playlist is
//! grouped by album (one cover per album block, text rows under it), and
//! "now playing" replaces the content area as a full page with big lyrics
//! instead of floating over it.

use iced::widget::{button, column, container, mouse_area, row, rule, scrollable, space};
use iced::{Alignment, Element, Length, Padding};

use crate::icons::{self, I};
use crate::parts::{self, bold, semibold, skel, t};
use crate::tokens::*;
use crate::{App, Message, PLAYLIST_SCROLL, Tab, data, variant_a};

const PER_ALBUM: usize = 12;
const GROUP_HEAD: f32 = 88.0;
const ROW_H: f32 = 32.0;
const GROUP_H: f32 = GROUP_HEAD + PER_ALBUM as f32 * ROW_H + 16.0;
const TITLE_H: f32 = 150.0;

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.drawer.interpolate(0.0f32, 1.0, app.now);
    let main: Element<_> = if app.drawer.value() || p > 0.0 {
        container(now_playing_page(app, p)).width(Length::Fill).into()
    } else {
        playlist(app)
    };
    column![
        top_strip(app),
        row![sidebar(app), container(main).width(Length::Fill).height(Length::Fill).style(fill(BG_BASE))]
    ]
    .into()
}

fn top_strip(app: &App) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[app.current];
    container(
        row![
            t("syzygy", 18.0, TEXT_PRIMARY).font(bold()).width(200),
            parts::transport(app, 34.0),
            container(
                row![
                    mouse_area(app.covers.view(tr.cover, 40.0, 4.0, app.now)).on_press(Message::ToggleDrawer),
                    column![
                        row![
                            t(&tr.title, FS_BODY, TEXT_PRIMARY).font(semibold()),
                            t(" — ", FS_BODY, TEXT_FAINT),
                            t(&tr.artist, FS_BODY, TEXT_SECONDARY)
                        ],
                        parts::seek_bar(app),
                    ]
                    .spacing(0)
                    .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding([4, 10])
            .width(Length::Fill)
            .max_width(640)
            .style(fill_rounded(BG_INSET, 8.0)),
            space().width(Length::Fill),
            parts::volume_icon(app),
            parts::volume(app),
            button(t(if app.drawer.value() { "Back to library" } else { "Now playing" }, FS_SMALL, TEXT_PRIMARY).font(semibold()))
                .on_press(Message::ToggleDrawer)
                .padding([6, 14])
                .style(crate::tokens::tab(app.drawer.value())),
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    )
    .center_y(64)
    .padding([0, 16])
    .style(top_border(BG_ELEVATED))
    .into()
}

fn sidebar(app: &App) -> Element<'_, Message> {
    let item = |i: usize, label: &str, indent: f32| -> Element<'_, Message> {
        button(t(label.to_string(), FS_BODY, if app.nav == i { TEXT_PRIMARY } else { TEXT_SECONDARY }))
            .on_press(Message::Nav(i))
            .width(Length::Fill)
            .padding(Padding::from([5, 10]).left(10.0 + indent))
            .style(nav_item(app.nav == i))
            .into()
    };
    let section = |s| container(t(s, FS_TINY, TEXT_FAINT).font(bold())).padding(Padding::from([12, 10]).bottom(4.0));
    let mut col = column![section("BROWSE"), item(0, "Home", 0.0), item(1, "Explore", 0.0), item(2, "Feed", 0.0), item(3, "Search", 0.0)]
        .spacing(1);
    col = col.push(section("LIBRARY"));
    for (i, label) in ["Loved tracks", "Albums", "Artists", "Mixes"].into_iter().enumerate() {
        col = col.push(item(4 + i, label, 0.0));
    }
    col = col.push(section("PLAYLISTS"));
    col = col.push(item(8, "▾ Folder: Gym", 0.0));
    for (i, it) in app.catalog.library.iter().enumerate() {
        col = col.push(item(10 + i, &it.name, if i < 4 { 14.0 } else { 0.0 }));
    }
    container(scrollable(col).style(page_scroll).height(Length::Fill))
        .padding([8, 8])
        .width(240)
        .height(Length::Fill)
        .style(fill(BG_SIDEBAR))
        .into()
}

fn playlist(app: &App) -> Element<'_, Message> {
    let title = container(
        column![
            t("Everything I've Ever Loved", 32.0, TEXT_PRIMARY).font(bold()),
            row![
                t("5,000 tracks · 417 albums · 341 hr", FS_BODY, TEXT_MUTED),
                space().width(Length::Fill),
                button(container(row![icons::filled(I::Play, 14.0, ON_ACCENT), t("Play all", FS_BODY, ON_ACCENT).font(bold())].spacing(6).align_y(Alignment::Center)).padding([6, 16]))
                    .on_press(Message::Play(0))
                    .padding(0)
                    .style(accent_button),
                parts::icon(I::Shuffle, 15.0, app.shuffle, Message::ToggleShuffle),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        ]
        .spacing(10),
    )
    .height(TITLE_H)
    .padding(Padding::from([24, 32]));

    // Window over album blocks (fixed height each), not rows.
    let n_tracks = app.catalog.tracks.len();
    let groups = n_tracks.div_ceil(PER_ALBUM);
    let range = if app.windowed {
        let first = (((app.scroll_y - TITLE_H) / GROUP_H).floor().max(0.0) as usize).saturating_sub(1);
        first.min(groups)..(first + (app.viewport_h / GROUP_H).ceil() as usize + 2).min(groups)
    } else {
        0..groups
    };
    app.cost.set((app.cost.get().0, range.len() * PER_ALBUM));
    let (above, below) = (range.start as f32 * GROUP_H, (groups - range.end) as f32 * GROUP_H);
    let blocks = range.map(|g| album_block(app, g));

    scrollable(column![title, space().height(above), column(blocks).padding([0, 24]), space().height(below + 120.0)].width(Length::Fill))
        .id(PLAYLIST_SCROLL)
        .on_scroll(Message::Scrolled)
        .style(page_scroll)
        .height(Length::Fill)
        .into()
}

fn album_block(app: &App, g: usize) -> Element<'_, Message> {
    let first = g * PER_ALBUM;
    let last = (first + PER_ALBUM).min(app.catalog.tracks.len());
    let head_tr = &app.catalog.tracks[first];
    let (q, qc) = parts::quality_label(head_tr.quality);
    let head = row![
        app.covers.view(head_tr.cover, 64.0, 6.0, app.now),
        column![
            t(&head_tr.album, 17.0, TEXT_PRIMARY).font(bold()),
            row![t(&head_tr.artist, FS_BODY, TEXT_MUTED), parts::badge(q, qc)].spacing(8).align_y(Alignment::Center),
        ]
        .spacing(4),
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .height(GROUP_HEAD);

    let rows = (first..last).map(|i| {
        let tr = &app.catalog.tracks[i];
        let current = i == app.current;
        let mut title = row![t(&tr.title, FS_BODY, if current { ACCENT } else { TEXT_PRIMARY })].spacing(6).align_y(Alignment::Center);
        if tr.explicit {
            title = title.push(parts::badge("E", TEXT_MUTED));
        }
        button(
            row![
                t(format!("{}", i - first + 1), FS_SMALL, if current { ACCENT } else { TEXT_FAINT }).width(28),
                container(title).width(Length::Fill).clip(true),
                t(data::fmt_time(tr.duration), FS_SMALL, TEXT_MUTED),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        )
        .on_press(Message::Play(i))
        .height(ROW_H)
        .width(Length::Fill)
        .padding(Padding::from([0, 12]))
        .style(track_row(current))
        .into()
    });
    column![head, container(column(rows)).padding(Padding::ZERO.left(80.0)), space().height(16)].height(GROUP_H).into()
}

fn now_playing_page(app: &App, p: f32) -> Element<'_, Message> {
    let tr = &app.catalog.tracks[app.current];
    let tab = |label, tb| {
        button(t(label, FS_BODY, if app.tab == tb { TEXT_PRIMARY } else { TEXT_MUTED }).font(semibold()))
            .on_press(Message::DrawerTab(tb))
            .padding([6, 14])
            .style(crate::tokens::tab(app.tab == tb))
    };
    let lyrics: Element<_> = match app.tab {
        Tab::Lyrics => column((0..14).map(|i| {
            let w = [70, 52, 80, 44, 66, 58, 75, 40, 62, 50, 72, 46, 68, 55][i];
            let h = if i == 4 { 30.0 } else { 24.0 };
            let line = container(space()).width(Length::FillPortion(w)).height(h).style(fill_rounded(if i == 4 { ACCENT } else { BG_SURFACE_HOVER }, 4.0));
            row![line, space().width(Length::FillPortion(100 - w))].into()
        }))
        .spacing(18)
        .into(),
        _ => variant_a::tab_content(app),
    };
    let up_next = row((app.current + 1..app.current + 9).filter(|&i| i < app.catalog.tracks.len()).map(|i| {
        let tr = &app.catalog.tracks[i];
        button(column![app.covers.view(tr.cover, 96.0, 6.0, app.now), t(&tr.title, FS_SMALL, TEXT_SECONDARY).width(96)].spacing(6))
            .on_press(Message::Play(i))
            .padding(4)
            .style(track_row(false))
            .into()
    }))
    .spacing(8);

    container(
        column![
            row![
                column![
                    container(app.covers.view_fill(tr.cover, 12.0, app.now)).width(Length::Fill).max_width(440).height(440),
                    t(&tr.title, 26.0, TEXT_PRIMARY).font(bold()),
                    t(format!("{} · {}", tr.artist, tr.album), 15.0, TEXT_MUTED),
                ]
                .spacing(12)
                .width(Length::FillPortion(2)),
                column![
                    row![tab("Lyrics", Tab::Lyrics), tab("Queue", Tab::Queue), tab("Credits", Tab::Credits)].spacing(4),
                    scrollable(lyrics).style(page_scroll).height(Length::Fill),
                ]
                .spacing(16)
                .width(Length::FillPortion(3)),
            ]
            .spacing(48)
            .height(Length::Fill),
            rule::horizontal(1),
            t("UP NEXT", FS_TINY, TEXT_FAINT).font(bold()),
            scrollable(up_next).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(4).scroller_width(4))),
            skel(Length::Fixed(0.0), 0.0),
        ]
        .spacing(14),
    )
    .padding(40)
    .width(Length::Fill)
    .height(Length::Fill)
    // Fade the page in/out instead of sliding.
    .style(move |_| iced::widget::container::Style {
        background: Some(iced::Background::Color(iced::Color { a: p, ..BG_BASE })),
        ..Default::default()
    })
    .into()
}
