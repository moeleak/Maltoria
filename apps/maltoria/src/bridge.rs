//! The helper's events, as a subscription.
//!
//! The supervisor starts with the app and hands back a channel of events; the
//! subscription reads that channel for as long as the app runs.

use std::sync::{Arc, OnceLock};

use iced::Subscription;
use iced::futures::{StreamExt, stream};
use maltoria_core::bridge::{BridgeEvent, BridgeSupervisor};

use crate::Message;

static EVENTS: OnceLock<smol::channel::Receiver<BridgeEvent>> = OnceLock::new();

/// Starts the helper's supervisor, whose events [`events`] delivers.
pub fn start() -> BridgeSupervisor {
    let (supervisor, events) = BridgeSupervisor::start();

    // Only the first supervisor's events are read; there is one per app.
    let _ = EVENTS.set(events);

    supervisor
}

/// The helper's events, once [`start`] ran; none otherwise, as in demo mode.
pub fn events() -> Subscription<Message> {
    Subscription::run(|| match EVENTS.get().cloned() {
        Some(events) => events.map(|event| Message::Bridge(Arc::new(event))).boxed(),
        None => stream::pending().boxed(),
    })
}
