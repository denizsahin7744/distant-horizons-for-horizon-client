// `/dh` in the chat's tree, so the chat box colours and completes it.
fn distant_horizons_commands(game: &mut crate::game::Game<'_>, commands: &mut Vec<&'static str>) {
    crate::game::distant_horizons_bridge::commands(game, commands);
}
