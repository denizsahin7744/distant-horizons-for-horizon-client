// Once a frame: the LODs around the player, made and drawn.
fn distant_horizons_frame(game: &mut crate::game::Game<'_>) {
    crate::game::distant_horizons_bridge::frame(game);
}
