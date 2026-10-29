// The debug box, with the debug detail colours on.
fn distant_horizons_hud(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    crate::game::distant_horizons_bridge::hud(game, batch, corners);
}
