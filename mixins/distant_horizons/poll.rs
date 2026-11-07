fn distant_horizons_poll(value: Vec<Event>) -> Vec<Event> {
    crate::distant_horizons_inbox::keep(value)
}
