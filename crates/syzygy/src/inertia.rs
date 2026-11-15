//! Inertia for touchpad scrolling: a flick keeps scrolling after the
//! fingers lift, slowing to a stop.
//!
//! iced's scrollables stop dead when the fingers do. [`inertial`] wraps
//! the whole window and, after a flick, keeps sending the scrollable under
//! it more touchpad scrolling, a frame at a time. A mouse wheel's notches
//! never glide.

use std::time::{Duration, Instant};

use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer};
use iced::{
    Element, Event, Length, Point, Rectangle, Size, Theme, Vector, keyboard, touch, window,
};

/// `content`, whose scrollables glide on after a touchpad flick.
pub fn inertial<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    Element::new(Inertial {
        content: content.into(),
    })
}

struct Inertial<'a, Message> {
    content: Element<'a, Message>,
}

#[derive(Default)]
struct State {
    glide: Glide,
    /// Where the gesture scrolled: the glide scrolls there too, wherever
    /// the pointer goes meanwhile.
    at: Option<Point>,
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Inertial<'_, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        match event {
            // A touchpad's scrolling ends with an empty scroll as the
            // fingers lift.
            Event::Mouse(mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Pixels { x, y },
            }) => {
                let now = Instant::now();
                if *x == 0.0 && *y == 0.0 {
                    state.glide.released(now);
                    if state.glide.is_gliding() {
                        shell.request_redraw();
                    }
                } else {
                    state.glide.scrolled(Vector::new(*x, *y), now);
                    state.at = cursor.position();
                }
            }
            // Anything else the user does catches it, so a key that
            // navigates doesn't leave the new Page gliding.
            Event::Mouse(mouse::Event::WheelScrolled { .. } | mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. })
            | Event::Keyboard(keyboard::Event::KeyPressed { .. }) => state.glide.stop(),
            _ => {}
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );

        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };
        let state = tree.state.downcast_mut::<State>();
        let (Some(delta), Some(at)) = (state.glide.step(*now), state.at) else {
            return;
        };
        // iced runs a frame's update twice when it publishes messages; the
        // second pass has no time to glide through.
        if delta == Vector::ZERO {
            shell.request_redraw();
            return;
        }
        let scroll = Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Pixels {
                x: delta.x,
                y: delta.y,
            },
        });
        // Its own shell, to see whether anything moved: a scrollable asks
        // to be redrawn only when its offset changes. Whether it captured
        // the scroll won't do, as it keeps capturing for 1.5 s after it
        // last moved, even stuck at an end.
        let mut messages = Vec::new();
        let mut glided = Shell::new(&mut messages);
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            &scroll,
            layout,
            mouse::Cursor::Available(at),
            renderer,
            clipboard,
            &mut glided,
            viewport,
        );
        let scrolled = glided.redraw_request() == window::RedrawRequest::NextFrame;
        shell.merge(glided, std::convert::identity);
        let state = tree.state.downcast_mut::<State>();
        if !scrolled {
            // At the end of the content, or nothing scrolls there.
            state.glide.stop();
        } else if state.glide.is_gliding() {
            shell.request_redraw();
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}

/// How quickly a glide slows: its speed falls by 1/e every this many
/// seconds, so it travels this many seconds' worth of its starting speed.
const DECAY: f32 = 0.325;

/// The moves this long before the fingers lift set the glide's speed.
const RECENT: Duration = Duration::from_millis(100);

/// Fingers lifting slower than this, in pixels per second, were dragging,
/// not flicking, and don't glide.
const START_SPEED: f32 = 300.0;

/// A glide this slow, in pixels per second, has stopped.
const STOP_SPEED: f32 = 20.0;

fn speed(velocity: Vector) -> f32 {
    velocity.x.hypot(velocity.y)
}

/// A touchpad gesture's scrolling, and the glide after it.
#[derive(Debug, Default)]
pub struct Glide {
    /// The gesture's latest moves, oldest first.
    samples: Vec<(Instant, Vector)>,
    /// The glide's speed in pixels per second, and when it was last stepped.
    gliding: Option<(Vector, Instant)>,
}

impl Glide {
    /// The fingers moved the content by `delta` pixels.
    pub fn scrolled(&mut self, delta: Vector, at: Instant) {
        self.gliding = None;
        self.forget_before(at);
        self.samples.push((at, delta));
    }

    /// Whether a flick is still gliding.
    pub fn is_gliding(&self) -> bool {
        self.gliding.is_some()
    }

    /// Stop gliding, as something else took over.
    pub fn stop(&mut self) {
        self.gliding = None;
        self.samples.clear();
    }

    /// The fingers lifted.
    pub fn released(&mut self, at: Instant) {
        // Only how the fingers were moving as they lifted counts.
        self.forget_before(at);
        let samples = std::mem::take(&mut self.samples);
        let [(first, _), .., (last, _)] = samples[..] else {
            return;
        };
        let span = last.duration_since(first).as_secs_f32();
        if span == 0.0 {
            return;
        }
        let moved = samples[1..]
            .iter()
            .fold(Vector::ZERO, |sum, (_, delta)| sum + *delta);
        let velocity = moved * (1.0 / span);
        if speed(velocity) >= START_SPEED {
            self.gliding = Some((velocity, at));
        }
    }

    /// Drop the moves too long before `at` to count.
    fn forget_before(&mut self, at: Instant) {
        self.samples
            .retain(|(time, _)| at.saturating_duration_since(*time) <= RECENT);
    }

    /// How far to scroll since the last step, while gliding.
    pub fn step(&mut self, now: Instant) -> Option<Vector> {
        let (velocity, last) = self.gliding.as_mut()?;
        let dt = now.saturating_duration_since(*last).as_secs_f32();
        let kept = (-dt / DECAY).exp();
        // The decaying speed's integral over the step.
        let distance = *velocity * (DECAY * (1.0 - kept));
        *velocity *= kept;
        *last = now;
        if speed(*velocity) < STOP_SPEED {
            self.gliding = None;
        }
        Some(distance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    /// Ten events 10 ms apart, each `dy` pixels: `dy` × 100 px/s.
    fn flick(glide: &mut Glide, start: Instant, dy: f32) {
        for i in 0..10 {
            glide.scrolled(Vector::new(0.0, dy), ms(start, i * 10));
        }
    }

    #[test]
    fn a_flick_keeps_scrolling_its_way_after_release() {
        let start = Instant::now();
        let mut glide = Glide::default();
        flick(&mut glide, start, -20.0);
        glide.released(ms(start, 95));

        let step = glide.step(ms(start, 111)).expect("gliding");
        // 2000 px/s for 16 ms, a little less as it slows: about 30 px.
        assert!(step.x == 0.0);
        assert!((-32.0..-25.0).contains(&step.y), "{step:?}");
    }

    #[test]
    fn a_glide_slows_to_a_stop() {
        let start = Instant::now();
        let mut glide = Glide::default();
        flick(&mut glide, start, 20.0);
        glide.released(ms(start, 95));

        let mut steps = Vec::new();
        let mut now = 95;
        while let Some(step) = glide.step(ms(start, now + 16)) {
            now += 16;
            steps.push(step.y);
            assert!(now < 5_000, "still gliding after 5 s");
        }
        assert!(steps.windows(2).all(|pair| pair[1] < pair[0]), "{steps:?}");
        // 2000 px/s decaying with a 325 ms time constant goes about 650 px.
        let total: f32 = steps.iter().sum();
        assert!((600.0..=650.0).contains(&total), "{total}");
    }

    #[test]
    fn fingers_that_rest_before_lifting_dont_glide() {
        let start = Instant::now();
        let mut glide = Glide::default();
        flick(&mut glide, start, 20.0);
        glide.released(ms(start, 300));

        assert_eq!(glide.step(ms(start, 316)), None);
    }

    #[test]
    fn a_slow_drag_doesnt_glide() {
        let start = Instant::now();
        let mut glide = Glide::default();
        // 100 px/s.
        flick(&mut glide, start, 1.0);
        glide.released(ms(start, 95));

        assert_eq!(glide.step(ms(start, 111)), None);
    }

    #[test]
    fn scrolling_again_catches_a_glide() {
        let start = Instant::now();
        let mut glide = Glide::default();
        flick(&mut glide, start, 20.0);
        glide.released(ms(start, 95));
        glide.scrolled(Vector::new(0.0, 1.0), ms(start, 200));

        assert_eq!(glide.step(ms(start, 216)), None);
    }

    #[test]
    fn a_stopped_glide_stays_stopped() {
        let start = Instant::now();
        let mut glide = Glide::default();
        flick(&mut glide, start, 20.0);
        glide.released(ms(start, 95));
        glide.stop();

        assert_eq!(glide.step(ms(start, 111)), None);
    }
}
