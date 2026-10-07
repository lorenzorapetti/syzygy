//! Engine events into iced (ADR 0003). `boot` creates each engine's channel
//! and keeps the receiver in an [`EventSource`]; `subscription()` streams it.

use iced::Subscription;
use iced::futures::stream;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedReceiver;

/// One engine's event receiver, waiting for its subscription to take it.
/// Hashes by a fixed id, so iced keeps one subscription running for it.
pub struct EventSource<T> {
    id: &'static str,
    receiver: Arc<Mutex<Option<UnboundedReceiver<T>>>>,
}

impl<T> Clone for EventSource<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            receiver: self.receiver.clone(),
        }
    }
}

impl<T> Hash for EventSource<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: Send + 'static> EventSource<T> {
    pub fn new(id: &'static str, receiver: UnboundedReceiver<T>) -> Self {
        Self {
            id,
            receiver: Arc::new(Mutex::new(Some(receiver))),
        }
    }

    /// The events, for as long as the app runs. The receiver can be taken
    /// once, so this must be returned from every `subscription()` call.
    pub fn subscription(&self) -> Subscription<T> {
        Subscription::run_with(self.clone(), |source| {
            let receiver = source
                .receiver
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take();
            if receiver.is_none() {
                log::error!("The {} events were already taken", source.id);
            }
            stream::unfold(receiver, |receiver| async move {
                let mut receiver = receiver?;
                let event = receiver.recv().await?;
                Some((event, Some(receiver)))
            })
        })
    }
}
