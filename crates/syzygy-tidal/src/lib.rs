//! The TIDAL client: the API, sign-in helpers and stream resolution.
//!
//! [`TidalClient`] takes a plain `reqwest::Client` and an event sender. It
//! refreshes tokens only after a 401 and reports the outcome as an [`Event`].

pub mod auth;
mod client;
mod config;
mod error;
pub mod models;
mod rate_gate;
mod stream;

pub use auth::LoginMethod;
pub use client::{Event, TidalClient};
pub use error::Error;
pub use stream::{PlayableStream, Quality};
