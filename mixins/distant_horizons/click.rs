// A click on the mod's settings page: change the setting and keep it.
fn distant_horizons_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use crate::game::{distant_horizons as m, distant_horizons_bridge as b};
    if mod_id != m::ID {
        return;
    }
    let mut settings = b::settings(game);
    settings.click(button);
    b::set_settings(game, settings);
}
