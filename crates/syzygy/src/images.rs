//! The App-level image cache: cover URL → `Loading | Allocating | Ready |
//! Failed`, in an LRU capped by decoded bytes. It outlives the Shell. A cover
//! counts as shown each time it's drawn, so one that stays on screen isn't
//! evicted.
//!
//! `update` is pure: it returns the [`Effect`]s for the app to run (fetch
//! through the catalog, allocate on the GPU), so the cache can be tested
//! without TIDAL or a renderer.

use iced::widget::image::{Allocation, Handle};
use iced::widget::{container, image as picture, sensor, space};
use iced::{Animation, ContentFit, Element};
use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use syzygy_catalog::Catalog;

use crate::style;

/// How many covers are fetched at once.
const MAX_FETCHING: usize = 6;
/// How much decoded cover the cache holds before the oldest go.
pub const BYTE_CAP: usize = 256 * 1024 * 1024;
const FADE: Duration = Duration::from_millis(220);
/// Start loading a cover this far before it scrolls into view.
const ANTICIPATE: f32 = 200.0;

pub struct Images {
    slots: HashMap<String, Entry>,
    /// Covers waiting for a fetch, oldest first.
    queue: VecDeque<String>,
    /// Fetches in flight.
    fetching: usize,
    /// Decoded bytes held, and how many may be.
    bytes: usize,
    cap: usize,
    /// Counts up each time a cover is shown, for the LRU order. A `Cell`,
    /// as drawing a cover shows it.
    clock: Cell<u64>,
    /// The time the covers are drawn at.
    now: Instant,
    /// When the last fade-in ends.
    fading_until: Option<Instant>,
}

struct Entry {
    slot: Slot,
    /// When it was last shown, on [`Images::clock`].
    shown: Cell<u64>,
}

enum Slot {
    /// Waiting for a fetch, or being fetched.
    Loading,
    /// Decoded and on its way to the GPU.
    Allocating(Picture),
    Ready {
        picture: Picture,
        /// Holding it keeps the cover on the GPU.
        _allocation: Allocation,
        fade: Animation<bool>,
    },
    /// Not fetched again.
    Failed,
}

impl Slot {
    /// The decoded bytes it holds.
    fn bytes(&self) -> usize {
        match self {
            Slot::Allocating(picture) | Slot::Ready { picture, .. } => picture.bytes,
            Slot::Loading | Slot::Failed => 0,
        }
    }
}

/// A decoded picture, ready to allocate.
#[derive(Debug, Clone)]
pub struct Picture {
    handle: Handle,
    bytes: usize,
}

impl Picture {
    /// RGBA pixels, `width` × `height`.
    pub fn rgba(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        let bytes = pixels.len();
        Self {
            handle: Handle::from_rgba(width, height, pixels),
            bytes,
        }
    }

    /// Decode a JPEG, PNG or WebP. Slow: run it off the UI thread.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let rgba = image::load_from_memory(bytes)?.into_rgba8();
        let (width, height) = rgba.dimensions();
        Ok(Self::rgba(width, height, rgba.into_raw()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Catalog(Arc<syzygy_catalog::Error>),
    #[error("Could not decode the image: {0}")]
    Decode(#[from] image::ImageError),
}

#[derive(Debug, Clone)]
pub enum Message {
    /// Covers came into view.
    Wanted(Vec<String>),
    /// A cover was fetched and decoded, or not.
    Decoded(String, Result<Picture, Arc<Error>>),
    /// A cover is on the GPU, or not, at this time.
    Allocated(
        String,
        Instant,
        Result<Allocation, Arc<iced::widget::image::Error>>,
    ),
    /// A frame is drawn while covers fade in.
    Frame(Instant),
}

/// Fetch a cover through the catalog (disk-cached) and decode it off the UI
/// thread.
pub async fn fetch(catalog: Catalog, url: String) -> Result<Picture, Arc<Error>> {
    let bytes = catalog
        .image(&url)
        .await
        .map_err(|e| Arc::new(Error::Catalog(e)))?;
    tokio::task::spawn_blocking(move || Picture::decode(&bytes))
        .await
        .unwrap_or_else(|e| Err(image::ImageError::IoError(std::io::Error::other(e)).into()))
        .map_err(Arc::new)
}

/// What the app runs for the cache.
#[derive(Debug, PartialEq)]
pub enum Effect {
    /// Fetch and decode a cover.
    Fetch(String),
    /// Upload a decoded cover, so it draws on the next frame.
    Allocate(String, Handle),
}

impl Images {
    /// An empty cache holding up to `cap` decoded bytes.
    pub fn new(cap: usize) -> Self {
        Self {
            slots: HashMap::new(),
            queue: VecDeque::new(),
            fetching: 0,
            bytes: 0,
            cap,
            clock: Cell::new(0),
            now: Instant::now(),
            fading_until: None,
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Wanted(urls) => {
                for url in urls {
                    let shown = self.tick();
                    self.slots
                        .entry(url)
                        .and_modify(|entry| entry.shown.set(shown))
                        .or_insert_with_key(|url| {
                            self.queue.push_back(url.clone());
                            Entry {
                                slot: Slot::Loading,
                                shown: Cell::new(shown),
                            }
                        });
                }
                self.fetch_more()
            }
            Message::Decoded(url, result) => {
                self.fetching = self.fetching.saturating_sub(1);
                let mut effects = Vec::new();
                if let Some(entry) = self.slots.get_mut(&url)
                    && matches!(entry.slot, Slot::Loading)
                {
                    match result {
                        Ok(picture) => {
                            self.bytes += picture.bytes;
                            effects.push(Effect::Allocate(url.clone(), picture.handle.clone()));
                            entry.slot = Slot::Allocating(picture);
                            self.evict(&url);
                        }
                        Err(e) => {
                            log::warn!("Could not load the cover {url}: {e}");
                            entry.slot = Slot::Failed;
                        }
                    }
                }
                effects.extend(self.fetch_more());
                effects
            }
            Message::Allocated(url, at, result) => {
                // Gone already if it was evicted on the way.
                let Some(entry) = self.slots.get_mut(&url) else {
                    return Vec::new();
                };
                let slot = std::mem::replace(&mut entry.slot, Slot::Failed);
                let Slot::Allocating(picture) = slot else {
                    entry.slot = slot;
                    return Vec::new();
                };
                match result {
                    Ok(allocation) => {
                        self.now = at;
                        self.fading_until = Some(self.now + FADE);
                        entry.slot = Slot::Ready {
                            picture,
                            _allocation: allocation,
                            fade: Animation::new(false).duration(FADE).go(true, self.now),
                        };
                    }
                    Err(e) => {
                        log::warn!("Could not allocate the cover {url}: {e}");
                        self.bytes -= picture.bytes;
                    }
                }
                Vec::new()
            }
            Message::Frame(now) => {
                self.now = now;
                Vec::new()
            }
        }
    }

    /// A cover is fading in, so frames are wanted.
    pub fn is_animating(&self) -> bool {
        self.fading_until.is_some_and(|until| self.now < until)
    }

    /// A `size` square cover with rounded corners. Until it's loaded, a
    /// placeholder that sends `wanted` as it comes into view.
    pub fn cover<'a, M: Clone + 'a>(
        &'a self,
        url: &str,
        size: f32,
        radius: f32,
        wanted: M,
    ) -> Element<'a, M> {
        let entry = self.slots.get(url);
        if let Some(entry) = entry {
            entry.shown.set(self.tick());
        }
        let slot = entry.map(|entry| &entry.slot);
        if let Some(Slot::Ready { picture, fade, .. }) = slot {
            return picture::Image::new(picture.handle.clone())
                .width(size)
                .height(size)
                .content_fit(ContentFit::Cover)
                .border_radius(radius)
                .opacity(fade.interpolate(0.0, 1.0, self.now))
                .into();
        }
        let placeholder = placeholder(size, radius);
        if let Some(Slot::Failed) = slot {
            return placeholder;
        }
        // Keyed by the URL, so a placeholder reused for another cover asks
        // again.
        sensor(placeholder)
            .key(url.to_string())
            .anticipate(ANTICIPATE)
            .on_show(move |_| wanted.clone())
            .into()
    }

    /// The next step on the LRU clock.
    fn tick(&self) -> u64 {
        let now = self.clock.get() + 1;
        self.clock.set(now);
        now
    }

    /// Start fetches from the queue while there's room.
    fn fetch_more(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        while self.fetching < MAX_FETCHING
            && let Some(url) = self.queue.pop_front()
        {
            self.fetching += 1;
            effects.push(Effect::Fetch(url));
        }
        effects
    }

    /// Drop the covers shown longest ago until the cache is under its cap,
    /// sparing `keep`, the cover just decoded.
    fn evict(&mut self, keep: &str) {
        while self.bytes > self.cap {
            let oldest = self
                .slots
                .iter()
                .filter(|(url, entry)| *url != keep && entry.slot.bytes() > 0)
                .min_by_key(|(_, entry)| entry.shown.get())
                .map(|(url, _)| url.clone());
            let Some(entry) = oldest.and_then(|url| self.slots.remove(&url)) else {
                return;
            };
            self.bytes -= entry.slot.bytes();
        }
    }
}

/// A `size` square stand-in for a picture that isn't there (yet).
pub fn placeholder<'a, M: 'a>(size: f32, radius: f32) -> Element<'a, M> {
    container(space())
        .width(size)
        .height(size)
        .style(move |theme| style::placeholder(theme, radius))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wanted(urls: &[&str]) -> Message {
        Message::Wanted(urls.iter().map(|url| url.to_string()).collect())
    }

    fn fetches(effects: &[Effect]) -> Vec<&str> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Fetch(url) => Some(url.as_str()),
                Effect::Allocate(..) => None,
            })
            .collect()
    }

    #[test]
    fn six_covers_are_fetched_at_a_time_and_each_only_once() {
        let mut images = Images::new(1 << 20);
        let urls: Vec<String> = (0..8).map(|n| format!("cover-{n}")).collect();
        let urls: Vec<&str> = urls.iter().map(String::as_str).collect();

        let effects = images.update(wanted(&urls));
        assert_eq!(fetches(&effects), urls[..6]);

        // Shown again while loading: nothing new goes out.
        let effects = images.update(wanted(&["cover-0", "cover-7"]));
        assert_eq!(fetches(&effects), [] as [&str; 0]);
    }

    fn picture(bytes: usize) -> Picture {
        Picture::rgba(1, (bytes / 4) as u32, vec![0; bytes])
    }

    fn decoded(url: &str, bytes: usize) -> Message {
        Message::Decoded(url.to_string(), Ok(picture(bytes)))
    }

    fn failed(url: &str) -> Message {
        let error = image::ImageError::IoError(std::io::Error::other("truncated"));
        Message::Decoded(url.to_string(), Err(Arc::new(Error::from(error))))
    }

    #[test]
    fn a_finished_fetch_makes_room_for_the_next() {
        let mut images = Images::new(1 << 20);
        let urls: Vec<String> = (0..8).map(|n| format!("cover-{n}")).collect();
        images.update(Message::Wanted(urls));

        let effects = images.update(decoded("cover-0", 16));
        assert_eq!(fetches(&effects), ["cover-6"]);
        assert!(
            matches!(&effects[..], [Effect::Allocate(url, _), _] if url == "cover-0"),
            "{effects:?}"
        );

        let effects = images.update(failed("cover-1"));
        assert_eq!(effects, [Effect::Fetch("cover-7".to_string())]);
    }

    #[test]
    fn a_cover_that_failed_is_not_fetched_again() {
        let mut images = Images::new(1 << 20);
        images.update(wanted(&["broken"]));
        images.update(failed("broken"));

        let effects = images.update(wanted(&["broken"]));
        assert_eq!(effects, []);
    }

    #[test]
    fn over_the_byte_cap_the_cover_shown_longest_ago_goes() {
        let mut images = Images::new(300);
        images.update(wanted(&["a", "b", "c"]));
        for url in ["a", "b", "c"] {
            images.update(decoded(url, 100));
        }
        // "a" is shown again, so "b" is now the oldest.
        assert_eq!(images.update(wanted(&["a"])), []);
        images.update(wanted(&["d"]));
        images.update(decoded("d", 100));

        assert_eq!(images.update(wanted(&["a", "c", "d"])), []);
        assert_eq!(
            images.update(wanted(&["b"])),
            [Effect::Fetch("b".to_string())]
        );
    }

    #[test]
    fn a_cover_still_on_screen_is_not_the_one_that_goes() {
        let mut images = Images::new(300);
        images.update(wanted(&["a", "b", "c"]));
        for url in ["a", "b", "c"] {
            images.update(decoded(url, 100));
        }
        // Loaded covers send nothing; being drawn is what keeps "a" fresh.
        let _ = images.cover("a", 160.0, 4.0, ());
        images.update(wanted(&["d"]));
        images.update(decoded("d", 100));

        assert_eq!(images.update(wanted(&["a", "c", "d"])), []);
        assert_eq!(
            images.update(wanted(&["b"])),
            [Effect::Fetch("b".to_string())]
        );
    }
}
