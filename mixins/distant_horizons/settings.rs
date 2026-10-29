// The mod's settings page.
fn distant_horizons_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::distant_horizons as m;
    if mod_id != m::ID {
        return;
    }
    buttons.extend(crate::game::distant_horizons_bridge::settings(game).labels());
}
