//! A long list read a page at a time: the first page through the cache,
//! the rest as the user scrolls to the end.

use iced::widget::{button, row, sensor, space, text};
use iced::{Alignment, Element};
use std::sync::Arc;
use syzygy_catalog::{Paged, Read};

use super::Remote;

/// Start loading more this far before the end comes into view.
const LOAD_MORE_AHEAD: f32 = 400.0;

pub struct List<T> {
    pub list: Remote<Paged<T>>,
    more: More,
    /// Pages were added after the first. A new first page would drop them,
    /// so it's no longer taken.
    extended: bool,
}

enum More {
    Idle,
    Loading,
    /// Loading from this offset failed. It.s asked for again only on Retry.
    Failed(usize),
}

impl<T> List<T> {
    pub fn new() -> Self {
        Self {
            list: Remote::Loading,
            more: More::Idle,
            extended: false,
        }
    }

    pub fn items(&self) -> &[T] {
        self.list.loaded().map_or(&[], |page| &page.items)
    }

    /// There's more of the list than has been read.
    pub fn has_more(&self) -> bool {
        self.list.loaded().is_some_and(|page| page.has_more)
    }

    /// Where TIDAL's next page starts, for a list it pages by cursor.
    pub fn cursor(&self) -> Option<String> {
        self.list.loaded()?.cursor.clone()
    }

    /// How long the whole list is, when TIDAL says.
    pub fn total(&self) -> Option<usize> {
        self.list.loaded()?.total
    }

    /// The list is being read again: the next first page replaces it, even
    /// once it's paged past its first, and what's loading after it is
    /// dropped.
    pub fn reread(&mut self) {
        self.extended = false;
        self.more = More::Idle;
    }

    /// The list can't be read: show `error` in its place.
    pub fn fail(&mut self, error: syzygy_catalog::Error) {
        self.list = Remote::Failed(Arc::new(error));
    }

    /// A read of the first page.
    pub fn apply(&mut self, read: Read<Paged<T>>, what: &str) {
        if self.extended {
            log::debug!("Keeping the paged {what} over a new first page");
        } else {
            self.list.apply(read, what);
        }
    }

    /// The items from `offset` on. Dropped unless they follow on from what's
    /// on screen.
    pub fn more(&mut self, offset: usize, result: Result<Paged<T>, Arc<syzygy_catalog::Error>>) {
        let Remote::Loaded(page) = &mut self.list else {
            return;
        };
        if page.items.len() != offset {
            return;
        }
        match result {
            Ok(more) => {
                self.more = More::Idle;
                page.items.extend(more.items);
                page.has_more = more.has_more;
                page.cursor = more.cursor;
                page.total = more.total.or(page.total);
                self.extended = true;
            }
            Err(e) => {
                log::warn!("Could not load more: {e}");
                self.more = More::Failed(offset);
            }
        }
    }

    /// Try the page that failed again.
    pub fn retry(&mut self) -> Option<usize> {
        if matches!(self.more, More::Failed(_)) {
            self.more = More::Idle;
        }
        self.next()
    }

    /// Where the next page starts, if one should be loaded now. It's then
    /// counted as loading.
    pub fn next(&mut self) -> Option<usize> {
        let Remote::Loaded(page) = &self.list else {
            return None;
        };
        let offset = page.items.len();
        let wanted = match self.more {
            More::Loading => false,
            More::Failed(failed) => failed != offset && page.has_more,
            More::Idle => page.has_more,
        };
        wanted.then(|| {
            self.more = More::Loading;
            offset
        })
    }

    /// Goes at the end of the list: a line while more loads, the error with
    /// Retry when it failed, or else a sensor that asks for more as it comes
    /// into view (again for each new page while the end is still in view).
    pub fn end<'a, Message: Clone + 'a>(
        &self,
        end_in_view: Message,
        retry: Message,
    ) -> Option<Element<'a, Message>> {
        let page = self.list.loaded().filter(|page| page.has_more)?;
        let end = match self.more {
            More::Loading => text("Loading…").style(text::secondary).into(),
            More::Failed(_) => row![
                text("Couldn't load more").style(text::secondary),
                button(text("Retry")).on_press(retry),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
            More::Idle => sensor(space().height(1))
                .key(page.items.len())
                .anticipate(LOAD_MORE_AHEAD)
                .on_show(move |_| end_in_view.clone())
                .into(),
        };
        Some(end)
    }
}
