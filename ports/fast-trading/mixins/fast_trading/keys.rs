// Track the override key (Left Alt) so a blocked trade can be pressed anyway.
fn fast_trading_keys(event: &winit::event::KeyEvent) {
    crate::game::fast_trading::key(event)
}
