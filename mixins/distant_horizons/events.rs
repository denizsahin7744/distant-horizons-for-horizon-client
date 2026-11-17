// The network events of a frame: Distant Horizons' messages to its inbox.
fn distant_horizons_events(value: Vec<Event>) -> Vec<Event> {
    crate::distant_horizons_inbox::filter(value)
}
