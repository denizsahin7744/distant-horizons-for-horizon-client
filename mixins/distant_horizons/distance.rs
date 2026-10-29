// The renderer's distance (far tiles, depth range, fog) reaches the LODs.
fn distant_horizons_distance(value: i32) -> i32 {
    crate::game::distant_horizons_bridge::terrain_distance(value)
}
