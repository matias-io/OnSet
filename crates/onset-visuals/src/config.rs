//! The show settings live in `onset-settings`, shared with the desktop app; this adds what
//! only the visuals do with them.
use std::path::{Path, PathBuf};

pub use onset_settings::*;
use onset_render::renderer::RenderSettings;

pub trait ConfigExt {
    /// What the renderer needs from the settings.
    fn render_settings(&self) -> RenderSettings;
    /// Copies `source` beside the config as the blackout logo, after checking it is an
    /// image, and returns the copy's path.
    fn adopt_logo(source: &Path) -> anyhow::Result<PathBuf>;
}

impl ConfigExt for Config {
    fn render_settings(&self) -> RenderSettings {
        RenderSettings {
            fx: self.preset.fx().unwrap_or(self.fx),
            quality: self.quality,
            auto: self.auto,
            auto_change: self.auto_change,
            rotation: self.rotation.clone(),
            tweaks: self.tweaks.clone(),
            adaptive: self.adaptive_resolution,
        }
    }

    fn adopt_logo(source: &Path) -> anyhow::Result<PathBuf> {
        let img = image::open(source)?;
        let dir = Self::path()
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join("blackout-logo.png");
        img.save(&dest)?;
        Ok(dest)
    }
}
