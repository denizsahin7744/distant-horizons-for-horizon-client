// Distant Horizons for Horizon Client: its messages from a server, taken
// out of the network events before the game handles them (it would ignore
// them), for the mod's session in the game (`distant_horizons_bridge`).
// Added to Horizon's library as `horizon_client::distant_horizons_inbox`.
//
// Part of Distant Horizons for Horizon Client, LGPL-3.0 (see runtime.rs).

use crate::net::Event;
use horizon_protocol::packets::play::Clientbound;
use std::sync::Mutex;

/// Distant Horizons' plugin channel (`ModInfo`).
const CHANNEL: &str = "distant_horizons:msg";
/// Payloads kept at most until the game takes them.
const MAX_WAITING: usize = 16384;

static WAITING: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

fn ours(event: &Event) -> bool {
    matches!(event, Event::Packet(p) if matches!(&**p, Clientbound::CustomPayload(c) if c.channel == CHANNEL))
}

/// `Session::poll`'s events without Distant Horizons' payloads, which wait
/// here, in order, for `take`.
pub fn filter(events: Vec<Event>) -> Vec<Event> {
    if !events.iter().any(ours) {
        return events;
    }
    let mut waiting = WAITING.lock().unwrap();
    events
        .into_iter()
        .filter_map(|event| {
            if !ours(&event) {
                return Some(event);
            }
            if let Event::Packet(p) = event
                && let Clientbound::CustomPayload(c) = *p
                && waiting.len() < MAX_WAITING
            {
                waiting.push(c.data);
            }
            None
        })
        .collect()
}

/// The payloads received since the last call.
pub fn take() -> Vec<Vec<u8>> {
    std::mem::take(&mut *WAITING.lock().unwrap())
}
