//! The header search: what's typed, and the dropdown under it with the
//! user's past searches, or TIDAL's suggestions for what's typed once the user
//! pauses. Typing never leaves the Page; submitting, or picking a query,
//! opens the Search Page.

use iced::widget::{
    Column, button, column, container, mouse_area, opaque, row, rule, scrollable, text, text_input,
};
use iced::{
    Alignment, Background, Border, Color, Element, Length, Shadow, Task, Theme, Vector, task,
    widget,
};
use std::time::Duration;
use syzygy_catalog::Suggestions;

use crate::icons::{Icon, icon};
use crate::images::Images;
use crate::page::{Link, search::hit};
use crate::style;

/// How long typing has to pause before suggestions are asked for.
pub const DEBOUNCE: Duration = Duration::from_millis(300);
const FIELD_WIDTH: f32 = 320.0;
const DROPDOWN_WIDTH: f32 = 420.0;
const DROPDOWN_HEIGHT: f32 = 560.0;
const FIELD: widget::Id = widget::Id::new("search");

#[derive(Default)]
pub struct Search {
    query: String,
    open: bool,
    /// Counts keystrokes, so only the wait after the last one fetches.
    keystrokes: u64,
    /// Suggestions were asked for what's typed and haven't come back.
    pending: bool,
    /// The last suggestions that came back for what was typed.
    latest: Option<Suggestions>,
    /// The suggestions read under way. Dropping it aborts it.
    fetch: Option<task::Handle>,
    hover: Option<Zone>,
}

/// The parts of the search the pointer can be over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Field,
    Dropdown,
}

#[derive(Debug, Clone)]
pub enum Message {
    Input(String),
    /// The typing paused after this keystroke.
    Waited(u64),
    Arrived {
        query: String,
        suggestions: Suggestions,
    },
    Submit,
    /// Empty the field.
    Clear,
    /// Search for a suggested or past query.
    Pick(String),
    /// Take a query out of the past searches.
    Forget(String),
    /// A suggested album, artist, playlist or track was picked, or its
    /// cover came into view.
    Link(Link),
    /// The pointer came over a part of the search.
    Enter(Zone),
    Leave(Zone),
    /// A mouse button went down anywhere in the window.
    Pressed,
    Escape,
}

/// What the Shell does for the search.
#[derive(Debug)]
pub enum Effect {
    None,
    /// Send `Waited(n)` after [`DEBOUNCE`].
    Wait(u64),
    /// Ask TIDAL for suggestions for this query.
    Fetch(String),
    /// Remember this query and open its Search Page.
    Search(String),
    Forget(String),
    Follow(Link),
    /// Put the cursor in the field.
    Focus,
}

/// What the dropdown shows.
#[derive(Debug)]
pub enum Dropdown<'a> {
    Hidden,
    /// The user's last searches, newest first.
    PastSearches(&'a [String]),
    /// Waiting for the first suggestions.
    Loading,
    Suggestions(&'a Suggestions),
    /// TIDAL suggested nothing.
    NoResults,
}

impl Search {
    pub fn update(&mut self, message: Message) -> Effect {
        match message {
            Message::Input(query) => {
                self.query = query;
                self.open = true;
                self.keystrokes += 1;
                if self.trimmed().is_empty() {
                    self.forget_suggestions();
                    Effect::None
                } else {
                    self.pending = true;
                    Effect::Wait(self.keystrokes)
                }
            }
            Message::Waited(keystroke)
                if keystroke == self.keystrokes && !self.trimmed().is_empty() =>
            {
                Effect::Fetch(self.trimmed().to_string())
            }
            Message::Waited(_) => Effect::None,
            Message::Arrived { query, suggestions } if query == self.trimmed() => {
                self.pending = false;
                self.latest = Some(suggestions);
                Effect::None
            }
            Message::Arrived { .. } => Effect::None,
            Message::Submit => match self.trimmed() {
                "" => Effect::None,
                query => {
                    let query = query.to_string();
                    self.close();
                    Effect::Search(query)
                }
            },
            Message::Clear => {
                self.query.clear();
                self.forget_suggestions();
                Effect::Focus
            }
            Message::Pick(query) => {
                let query = query.trim().to_string();
                if query != self.trimmed() {
                    self.forget_suggestions();
                }
                self.query = query.clone();
                self.close();
                Effect::Search(query)
            }
            Message::Forget(query) => Effect::Forget(query),
            Message::Link(link) => {
                // Covers ask for their pictures as they come into view.
                if matches!(link, Link::Open(_)) {
                    self.close();
                }
                Effect::Follow(link)
            }
            Message::Enter(zone) => {
                self.hover = Some(zone);
                Effect::None
            }
            // Leaving one part for the other may come after entering it.
            Message::Leave(zone) => {
                if self.hover == Some(zone) {
                    self.hover = None;
                }
                Effect::None
            }
            Message::Pressed => match self.hover {
                Some(Zone::Field) => self.reopen(),
                Some(Zone::Dropdown) => Effect::None,
                None => {
                    self.close();
                    Effect::None
                }
            },
            Message::Escape => {
                self.close();
                Effect::None
            }
        }
    }

    /// The suggestions read for `Fetch`. It replaces, and so aborts, the
    /// one before.
    pub fn fetching(&mut self, handle: task::Handle) {
        self.fetch = Some(handle.abort_on_drop());
    }

    /// A Search Page for `query` opened: the field shows its query.
    pub fn showing(&mut self, query: &str) {
        if self.query != query {
            self.query = query.to_string();
            self.forget_suggestions();
        }
        self.close();
    }

    /// The field was clicked: the dropdown opens, and suggestions are
    /// asked for if there are none for what's in it.
    fn reopen(&mut self) -> Effect {
        self.open = true;
        if self.trimmed().is_empty() || self.latest.is_some() || self.pending {
            return Effect::None;
        }
        self.pending = true;
        Effect::Fetch(self.trimmed().to_string())
    }

    /// Put the cursor in the field.
    pub fn focus<T: Send + 'static>() -> Task<T> {
        widget::operation::focus(FIELD)
    }

    pub fn dropdown<'a>(&'a self, past_searches: &'a [String]) -> Dropdown<'a> {
        if !self.open {
            return Dropdown::Hidden;
        }
        if self.trimmed().is_empty() {
            return if past_searches.is_empty() {
                Dropdown::Hidden
            } else {
                Dropdown::PastSearches(past_searches)
            };
        }
        match &self.latest {
            Some(latest) if latest.queries.is_empty() && latest.hits.is_empty() => {
                if self.pending {
                    Dropdown::Loading
                } else {
                    Dropdown::NoResults
                }
            }
            Some(latest) => Dropdown::Suggestions(latest),
            None if self.pending => Dropdown::Loading,
            None => Dropdown::Hidden,
        }
    }

    /// The field in the header.
    pub fn field(&self) -> Element<'_, Message> {
        let input = text_input("Search", &self.query)
            .id(FIELD)
            .on_input(Message::Input)
            .on_submit(Message::Submit)
            .padding(0)
            .size(14)
            .style(bare_input);
        let clear = (!self.query.is_empty()).then(|| {
            button(icon(Icon::X, 16.0, style::TEXT_MUTED))
                .padding(0)
                .style(style::icon_button)
                .on_press(Message::Clear)
        });
        let field = container(
            row![icon(Icon::Search, 16.0, style::TEXT_MUTED), input]
                .push(clear)
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .padding([8, 14])
        .width(FIELD_WIDTH)
        .style(field_box);
        mouse_area(field)
            .on_enter(Message::Enter(Zone::Field))
            .on_exit(Message::Leave(Zone::Field))
            .into()
    }

    /// The dropdown under the field, when it has something to show.
    pub fn view<'a>(
        &'a self,
        past_searches: &'a [String],
        images: &'a Images,
    ) -> Option<Element<'a, Message>> {
        let content: Element<'a, Message> = match self.dropdown(past_searches) {
            Dropdown::Hidden => return None,
            Dropdown::PastSearches(past_searches) => {
                let rows = past_searches.iter().map(|query| past_search(query));
                column![label("Recent searches")]
                    .extend(rows)
                    .spacing(2)
                    .into()
            }
            Dropdown::Loading => notice("Searching…"),
            Dropdown::NoResults => notice("No results found"),
            Dropdown::Suggestions(suggestions) => {
                let queries = suggestions.queries.iter().map(|suggestion| {
                    let glyph = if suggestion.searched_before {
                        Icon::Clock
                    } else {
                        Icon::Search
                    };
                    query_row(glyph, &suggestion.query)
                });
                let hits = suggestions
                    .hits
                    .iter()
                    .map(|h| hit(h, images).map(Message::Link));
                let divider = (!suggestions.queries.is_empty() && !suggestions.hits.is_empty())
                    .then(|| rule::horizontal(1));
                let view_all = button(
                    container(text("View all results").size(12).color(style::ACCENT))
                        .center_x(Length::Fill),
                )
                .padding([10, 12])
                .width(Length::Fill)
                .style(style::list_row)
                .on_press(Message::Submit);
                Column::with_children(queries)
                    .push(divider)
                    .extend(hits)
                    .push(view_all)
                    .spacing(2)
                    .into()
            }
        };
        let panel = container(scrollable(container(content).padding(6)))
            .width(DROPDOWN_WIDTH)
            .max_height(DROPDOWN_HEIGHT)
            .style(panel);
        let panel = mouse_area(panel)
            .on_enter(Message::Enter(Zone::Dropdown))
            .on_exit(Message::Leave(Zone::Dropdown));
        Some(opaque(panel))
    }

    fn trimmed(&self) -> &str {
        self.query.trim()
    }

    fn forget_suggestions(&mut self) {
        self.pending = false;
        self.latest = None;
        self.fetch = None;
    }

    fn close(&mut self) {
        self.open = false;
        // The dropdown goes without the pointer leaving it.
        if self.hover == Some(Zone::Dropdown) {
            self.hover = None;
        }
    }
}

/// One of the user's past searches, with a way to forget it.
fn past_search(query: &str) -> Element<'_, Message> {
    let forget = button(icon(Icon::X, 14.0, style::TEXT_MUTED))
        .padding(4)
        .style(style::icon_button)
        .on_press(Message::Forget(query.to_string()));
    row![query_row(Icon::Clock, query), forget]
        .align_y(Alignment::Center)
        .into()
}

/// A query to search for when clicked.
fn query_row(glyph: Icon, query: &str) -> Element<'_, Message> {
    let line = row![
        icon(glyph, 15.0, style::TEXT_MUTED),
        text(query).size(13).wrapping(text::Wrapping::None),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    button(container(line).clip(true))
        .padding([10, 12])
        .width(Length::Fill)
        .style(style::list_row)
        .on_press(Message::Pick(query.to_string()))
        .into()
}

fn label(label: &str) -> Element<'_, Message> {
    container(text(label.to_uppercase()).size(11).color(style::TEXT_MUTED))
        .padding([6, 12])
        .into()
}

fn notice(notice: &str) -> Element<'_, Message> {
    container(text(notice).size(13).color(style::TEXT_MUTED))
        .padding(20)
        .center_x(Length::Fill)
        .into()
}

fn field_box(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_INSET.into()),
        border: style::rounded(18.0),
        ..container::Style::default()
    }
}

/// The field draws its own box; the input inside it draws nothing.
fn bare_input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        ..style::filter_input(theme, status)
    }
}

fn panel(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(style::BG_SURFACE.into()),
        border: Border {
            color: style::BORDER_SUBTLE,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color::BLACK.scale_alpha(0.6),
            offset: Vector::new(0.0, 12.0),
            blur_radius: 32.0,
        },
        ..container::Style::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Route;
    use syzygy_catalog::home_feed::{Card, Target};
    use syzygy_catalog::{Hit, Suggestion, Suggestions};

    fn past_searches() -> Vec<String> {
        vec!["björk".to_string(), "sigur rós".to_string()]
    }

    fn suggestions(query: &str) -> Suggestions {
        Suggestions {
            queries: vec![Suggestion {
                query: format!("{query} live"),
                searched_before: false,
            }],
            hits: vec![Hit::Card(Card {
                title: "Björk".to_string(),
                subtitle: "Artist".to_string(),
                cover: None,
                target: Target::Artist(1),
            })],
        }
    }

    fn typed(search: &mut Search, text: &str) -> Effect {
        search.update(Message::Input(text.to_string()))
    }

    #[test]
    fn suggestions_are_asked_for_once_the_typing_pauses() {
        let mut search = Search::default();

        let first = typed(&mut search, "bj");
        let second = typed(&mut search, "björk ");
        let (Effect::Wait(first), Effect::Wait(second)) = (first, second) else {
            panic!("each keystroke should wait");
        };

        assert!(matches!(
            search.update(Message::Waited(first)),
            Effect::None
        ));
        assert!(
            matches!(search.update(Message::Waited(second)), Effect::Fetch(query) if query == "björk")
        );
    }

    #[test]
    fn with_nothing_typed_the_dropdown_offers_the_past_searches() {
        let mut search = Search::default();
        let past_searches = past_searches();

        assert!(matches!(typed(&mut search, "  "), Effect::None));

        assert!(
            matches!(search.dropdown(&past_searches), Dropdown::PastSearches(h) if h == past_searches)
        );
        assert!(matches!(search.dropdown(&[]), Dropdown::Hidden));
    }

    #[test]
    fn suggestions_show_once_they_arrive_for_what_is_typed() {
        let mut search = Search::default();
        typed(&mut search, "björk");
        assert!(matches!(search.dropdown(&[]), Dropdown::Loading));

        search.update(Message::Arrived {
            query: "björk".to_string(),
            suggestions: suggestions("björk"),
        });

        assert!(
            matches!(search.dropdown(&[]), Dropdown::Suggestions(s) if *s == suggestions("björk"))
        );
    }

    #[test]
    fn suggestions_for_an_older_query_are_dropped_and_the_last_ones_stay_meanwhile() {
        let mut search = Search::default();
        typed(&mut search, "björk");
        search.update(Message::Arrived {
            query: "björk".to_string(),
            suggestions: suggestions("björk"),
        });
        typed(&mut search, "björk jó");

        search.update(Message::Arrived {
            query: "björk j".to_string(),
            suggestions: suggestions("björk j"),
        });

        assert!(
            matches!(search.dropdown(&[]), Dropdown::Suggestions(s) if *s == suggestions("björk"))
        );
    }

    #[test]
    fn nothing_suggested_says_so() {
        let mut search = Search::default();
        typed(&mut search, "zzzz");

        search.update(Message::Arrived {
            query: "zzzz".to_string(),
            suggestions: Suggestions::default(),
        });

        assert!(matches!(search.dropdown(&[]), Dropdown::NoResults));
    }

    #[test]
    fn submitting_searches_for_what_is_typed_and_closes_the_dropdown() {
        let mut search = Search::default();
        typed(&mut search, " björk ");

        assert!(matches!(search.update(Message::Submit), Effect::Search(q) if q == "björk"));
        assert!(matches!(
            search.dropdown(&past_searches()),
            Dropdown::Hidden
        ));
    }

    #[test]
    fn submitting_nothing_does_nothing() {
        let mut search = Search::default();
        typed(&mut search, "   ");

        assert!(matches!(search.update(Message::Submit), Effect::None));
    }

    #[test]
    fn picking_a_query_searches_for_it_trimmed() {
        let mut search = Search::default();
        typed(&mut search, "bj");

        let effect = search.update(Message::Pick(" björk ".to_string()));

        assert!(matches!(effect, Effect::Search(q) if q == "björk"));
    }

    /// Clicking the field reopens the dropdown.
    fn reopen(search: &mut Search) -> Effect {
        search.update(Message::Enter(Zone::Field));
        search.update(Message::Pressed)
    }

    #[test]
    fn after_a_pick_the_dropdown_asks_for_suggestions_for_the_picked_query() {
        let mut search = Search::default();
        typed(&mut search, "bj");
        search.update(Message::Arrived {
            query: "bj".to_string(),
            suggestions: suggestions("bj"),
        });
        search.update(Message::Pick("björk".to_string()));
        search.showing("björk");

        let effect = reopen(&mut search);

        assert!(matches!(effect, Effect::Fetch(q) if q == "björk"));
        assert!(matches!(search.dropdown(&[]), Dropdown::Loading));
    }

    #[test]
    fn suggestions_already_read_for_the_query_are_shown_again_without_asking() {
        let mut search = Search::default();
        typed(&mut search, "björk");
        search.update(Message::Arrived {
            query: "björk".to_string(),
            suggestions: suggestions("björk"),
        });
        search.update(Message::Escape);

        assert!(matches!(reopen(&mut search), Effect::None));
        assert!(
            matches!(search.dropdown(&[]), Dropdown::Suggestions(s) if *s == suggestions("björk"))
        );
    }

    #[test]
    fn clearing_the_field_puts_the_cursor_back_in_it() {
        let mut search = Search::default();
        typed(&mut search, "björk");

        assert!(matches!(search.update(Message::Clear), Effect::Focus));
    }

    #[test]
    fn a_click_outside_closes_the_dropdown_and_one_on_the_field_opens_it() {
        let mut search = Search::default();
        let past_searches = past_searches();
        typed(&mut search, "");

        search.update(Message::Enter(Zone::Dropdown));
        search.update(Message::Pressed);
        assert!(matches!(
            search.dropdown(&past_searches),
            Dropdown::PastSearches(_)
        ));

        search.update(Message::Leave(Zone::Dropdown));
        search.update(Message::Pressed);
        assert!(matches!(search.dropdown(&past_searches), Dropdown::Hidden));

        search.update(Message::Enter(Zone::Field));
        search.update(Message::Pressed);
        assert!(matches!(
            search.dropdown(&past_searches),
            Dropdown::PastSearches(_)
        ));
    }

    #[test]
    fn moving_from_the_dropdown_to_the_field_still_counts_as_on_the_field() {
        let mut search = Search::default();
        let past_searches = past_searches();
        search.update(Message::Enter(Zone::Field));
        search.update(Message::Pressed);
        search.update(Message::Enter(Zone::Dropdown));

        search.update(Message::Enter(Zone::Field));
        search.update(Message::Leave(Zone::Dropdown));
        search.update(Message::Escape);
        search.update(Message::Pressed);

        assert!(matches!(
            search.dropdown(&past_searches),
            Dropdown::PastSearches(_)
        ));
    }

    #[test]
    fn a_cover_coming_into_view_keeps_the_dropdown_open() {
        let mut search = Search::default();
        let past_searches = past_searches();
        typed(&mut search, "");

        let effect = search.update(Message::Link(Link::CoverWanted("cover".to_string())));

        assert!(matches!(effect, Effect::Follow(Link::CoverWanted(_))));
        assert!(matches!(
            search.dropdown(&past_searches),
            Dropdown::PastSearches(_)
        ));
    }

    #[test]
    fn picking_a_hit_opens_it_and_closes_the_dropdown() {
        let mut search = Search::default();
        let past_searches = past_searches();
        typed(&mut search, "");

        let effect = search.update(Message::Link(Link::Open(Route::Favorites)));

        assert!(matches!(
            effect,
            Effect::Follow(Link::Open(Route::Favorites))
        ));
        assert!(matches!(search.dropdown(&past_searches), Dropdown::Hidden));
    }

    #[test]
    fn escape_closes_the_dropdown() {
        let mut search = Search::default();
        typed(&mut search, "");

        search.update(Message::Escape);

        assert!(matches!(
            search.dropdown(&past_searches()),
            Dropdown::Hidden
        ));
    }

    #[test]
    fn opening_a_search_page_shows_its_query_in_the_field() {
        let mut search = Search::default();
        typed(&mut search, "bj");

        search.showing("björk");

        assert!(matches!(
            search.dropdown(&past_searches()),
            Dropdown::Hidden
        ));
        assert!(matches!(search.update(Message::Submit), Effect::Search(q) if q == "björk"));
    }

    #[test]
    fn back_on_a_search_page_the_field_asks_for_its_suggestions_when_clicked() {
        let mut search = Search::default();

        search.showing("björk");

        assert!(matches!(reopen(&mut search), Effect::Fetch(q) if q == "björk"));
    }
}
