//! Rendering detail the DJ picks; the renderer and the settings file share it.
use serde::{Deserialize, Serialize};

/// Rendering detail. Higher levels raise the internal resolution, the ray-march steps
/// scenes take, the bloom depth, and allow the heaviest scenes into Auto mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Low,
    Medium,
    #[default]
    High,
    Ultra,
}

impl Quality {
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Ultra];

    /// The value scenes read as `quality()`.
    pub fn code(self) -> f32 {
        match self {
            Self::Low => 0.0,
            Self::Medium => 1.0,
            Self::High => 2.0,
            Self::Ultra => 3.0,
        }
    }

    pub fn internal_scale(self) -> f32 {
        match self {
            Self::Low => 0.5,
            Self::Medium => 0.75,
            Self::High | Self::Ultra => 1.0,
        }
    }

    pub fn bloom_levels(self) -> usize {
        match self {
            Self::Low => 3,
            Self::Medium => 4,
            Self::High | Self::Ultra => 5,
        }
    }

    /// Most expensive scene (by `SceneInfo::cost`) Auto mode may pick.
    pub fn max_cost(self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High | Self::Ultra => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Ultra => "Ultra",
        }
    }
}
