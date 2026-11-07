// Part of Distant Horizons for Horizon Client, LGPL-3.0 (see runtime.rs).

use std::sync::Mutex;

const CHANNEL: &str = "distant_horizons:msg";
const LIMIT: usize = 4096;

static INBOX: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

pub fn keep(events: Vec<crate::net::Event>) -> Vec<crate::net::Event> {
    use horizon_protocol::packets::play::Clientbound as C;
    if !events
        .iter()
        .any(|e| matches!(e, crate::net::Event::Packet(p) if matches!(&**p, C::CustomPayload(c) if c.channel == CHANNEL)))
    {
        return events;
    }
    let mut inbox = INBOX.lock().unwrap();
    events
        .into_iter()
        .filter_map(|event| match event {
            crate::net::Event::Packet(p) => match *p {
                C::CustomPayload(c) if c.channel == CHANNEL => {
                    if inbox.len() < LIMIT {
                        inbox.push(c.data);
                    }
                    None
                }
                other => Some(crate::net::Event::Packet(Box::new(other))),
            },
            event => Some(event),
        })
        .collect()
}

pub fn take() -> Vec<Vec<u8>> {
    std::mem::take(&mut *INBOX.lock().unwrap())
}
