//! OnSet's mark as the show window's icon.

/// The mark as the window and taskbar icon; `None` if it cannot be decoded.
pub fn window_icon() -> Option<winit::window::Icon> {
    let image = image::load_from_memory(include_bytes!("../../../assets/brand/onset-icon-64.png"))
        .map_err(|e| tracing::warn!("window icon unavailable: {e}"))
        .ok()?
        .into_rgba8();
    let (w, h) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), w, h)
        .map_err(|e| tracing::warn!("window icon unavailable: {e}"))
        .ok()
}
