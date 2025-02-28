use iced::theme::{Custom, Palette};
use iced::{Color, color};

use super::colors::accent_color;

#[derive(Default)]
pub enum FlashCatTheme {
    #[default]
    Light,
    Dark,
}

impl FlashCatTheme {
    pub fn get_custom_theme(&self) -> Custom {
        match self {
            FlashCatTheme::Light => Custom::new(
                "Light".to_owned(),
                Palette {
                    background: color!(0xdddddd),
                    text: Color::BLACK,
                    primary: accent_color(),
                    success: Color::from_rgb(0.0, 1.0, 0.0),
                    danger: Color::from_rgb(1.0, 0.0, 0.0),
                },
            ),
            FlashCatTheme::Dark => Custom::new(
                "Dark".to_owned(),
                Palette {
                    background: color!(0x161616),
                    text: color!(0xcccccc),
                    primary: accent_color(),
                    success: Color::from_rgb(0.0, 1.0, 0.0),
                    danger: Color::from_rgb(1.0, 0.0, 0.0),
                },
            ),
        }
    }
}
