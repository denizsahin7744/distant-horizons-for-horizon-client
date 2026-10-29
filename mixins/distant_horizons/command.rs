// `/dh`: the status and the settings, from chat.
fn distant_horizons_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::{distant_horizons as m, distant_horizons_bridge as b};
    // Only the window's game runs it: in single player the server's has no
    // session, and the client asks first.
    if *ran || !game.native_on(m::ID) || !game.native_window() {
        return;
    }
    *ran = b::command(game, command);
}
