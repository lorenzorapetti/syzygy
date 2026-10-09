//! PROTOTYPE. Cover-art cache: id → image Handle, loaded on demand when a
//! `sensor` around the placeholder comes into view, faded in, LRU-evicted.
//!
//! Covers are synthesised (gradient + disc) after a fake network delay, so the
//! pipeline (placeholder → decode off-thread → allocate → fade) is exercised
//! without TIDAL. Real code swaps `synthesise` for reqwest + `image` decode.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use iced::widget::image::{self, Allocation, Handle};
use iced::widget::{container, sensor, space};
use iced::{Animation, Element, Length, Task};

use crate::Message;
use crate::tokens;

pub const CAPACITY: usize = 400;
const SIZE: u32 = 160;

enum Slot {
    Loading,
    Ready { handle: Handle, _allocation: Allocation, fade: Animation<bool> },
}

#[derive(Default)]
pub struct Covers {
    slots: HashMap<u32, Slot>,
    order: VecDeque<u32>,
    pub requested: u64,
    pub evicted: u64,
}

#[derive(Debug, Clone)]
pub enum CoverMsg {
    Wanted(u32),
    Decoded(u32, Handle),
    Allocated(u32, Handle, Result<Allocation, image::Error>),
}

impl Covers {
    pub fn update(&mut self, msg: CoverMsg, now: Instant) -> Task<Message> {
        match msg {
            CoverMsg::Wanted(id) => {
                if self.slots.contains_key(&id) {
                    self.touch(id);
                    return Task::none();
                }
                self.requested += 1;
                self.slots.insert(id, Slot::Loading);
                self.order.push_back(id);
                self.evict();
                Task::perform(synthesise(id), move |h| Message::Cover(CoverMsg::Decoded(id, h)))
            }
            CoverMsg::Decoded(id, handle) => image::allocate(handle.clone())
                .map(move |r| Message::Cover(CoverMsg::Allocated(id, handle.clone(), r))),
            CoverMsg::Allocated(id, handle, Ok(allocation)) => {
                if let Some(slot) = self.slots.get_mut(&id) {
                    *slot = Slot::Ready {
                        handle,
                        _allocation: allocation,
                        fade: Animation::new(false).duration(Duration::from_millis(220)).go(true, now),
                    };
                }
                Task::none()
            }
            CoverMsg::Allocated(id, _, Err(_)) => {
                self.slots.remove(&id);
                Task::none()
            }
        }
    }

    fn touch(&mut self, id: u32) {
        if let Some(pos) = self.order.iter().position(|x| *x == id) {
            self.order.remove(pos);
            self.order.push_back(id);
        }
    }

    fn evict(&mut self) {
        while self.order.len() > CAPACITY {
            if let Some(old) = self.order.pop_front() {
                self.slots.remove(&old);
                self.evicted += 1;
            }
        }
    }

    pub fn is_animating(&self, now: Instant) -> bool {
        self.slots.values().any(|s| matches!(s, Slot::Ready { fade, .. } if fade.is_animating(now)))
    }

    pub fn stats(&self) -> (usize, usize) {
        let loading = self.slots.values().filter(|s| matches!(s, Slot::Loading)).count();
        (self.slots.len() - loading, loading)
    }

    /// A square cover of `size` px with `radius`. Placeholder until loaded.
    pub fn view(&self, id: u32, size: f32, radius: f32, now: Instant) -> Element<'_, Message> {
        match self.slots.get(&id) {
            Some(Slot::Ready { handle, fade, .. }) => iced::widget::image(handle.clone())
                .width(size)
                .height(size)
                .content_fit(iced::ContentFit::Cover)
                .border_radius(radius)
                .opacity(fade.interpolate(0.0, 1.0, now))
                .into(),
            _ => sensor(
                container(space())
                    .width(size)
                    .height(size)
                    .style(tokens::skeleton(radius)),
            )
            .on_show(move |_| Message::Cover(CoverMsg::Wanted(id)))
            .into(),
        }
    }

    /// Same, but filling its parent (drawer hero cover).
    pub fn view_fill(&self, id: u32, radius: f32, now: Instant) -> Element<'_, Message> {
        match self.slots.get(&id) {
            Some(Slot::Ready { handle, fade, .. }) => iced::widget::image(handle.clone())
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(iced::ContentFit::Contain)
                .border_radius(radius)
                .opacity(fade.interpolate(0.0, 1.0, now))
                .into(),
            _ => sensor(container(space()).width(Length::Fill).height(Length::Fill).style(tokens::skeleton(radius)))
                .on_show(move |_| Message::Cover(CoverMsg::Wanted(id)))
                .into(),
        }
    }
}

/// Fake fetch + decode: 120–700ms latency, then build RGBA off-thread.
async fn synthesise(id: u32) -> Handle {
    let delay = 120 + (id.wrapping_mul(2654435761) % 580) as u64;
    tokio::time::sleep(Duration::from_millis(delay)).await;
    tokio::task::spawn_blocking(move || {
        let hue = (id.wrapping_mul(47) % 360) as f32;
        let hue2 = (hue + 40.0 + (id % 5) as f32 * 30.0) % 360.0;
        let mut px = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        let c = SIZE as f32 / 2.0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let t = (x + y) as f32 / (2 * SIZE) as f32;
                let (r, g, b) = hsl(hue + (hue2 - hue) * t, 0.55, 0.25 + 0.25 * t);
                let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
                let ring = d < SIZE as f32 * 0.32 && d > SIZE as f32 * 0.08;
                let k = if ring { 1.35 } else { 1.0 };
                px.extend_from_slice(&[(r * k).min(255.0) as u8, (g * k).min(255.0) as u8, (b * k).min(255.0) as u8, 255]);
            }
        }
        Handle::from_rgba(SIZE, SIZE, px)
    })
    .await
    .expect("synth")
}

fn hsl(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    ((r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0)
}
