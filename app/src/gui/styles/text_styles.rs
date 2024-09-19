use super::colors::*;
use iced::{widget::text::Style, Theme};

/// A custom theme that makes text purple
pub fn accent_color_theme(_theme: &Theme) -> Style {
    Style{ color: Some(accent_color()) }
}

/// A custom theme that makes text red
pub fn red_text_theme(_theme: &Theme) -> Style {
    Style{ color: Some(red()) }
}

/// A custom theme that makes text green
pub fn green_text_theme(_theme: &Theme) -> Style {
    Style{ color: Some(green()) }
}

/// A custom theme that makes text brown
pub fn brown_text_theme(_theme: &Theme) -> Style {
    Style{ color: Some(brown()) }
}
