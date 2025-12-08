use iced::{Background, Border, Color, Theme, color, widget::container::Style};

use super::theme::FlashCatTheme;

/// A custom theme for container respecting Light and Dark FlashCatTheme
pub fn first_class_container_rounded_theme(theme: &Theme) -> Style {
    let (background, border_color) = match theme {
        Theme::Custom(custom) => {
            if **custom == FlashCatTheme::get_custom_theme(&FlashCatTheme::Light) {
                (Some(Background::Color(color!(0xcccccc))), color!(0xbbbbbb))
            } else {
                (Some(Background::Color(color!(0x1c1c1c))), Color::BLACK)
            }
        }
        _ => unreachable!("built-in iced themes are not in use"),
    };

    Style {
        background,
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: border_color,
        },
        ..Style::default()
    }
}

/// A custom theme for container respecting Light and Dark FlashCatTheme
pub fn second_class_container_rounded_theme(theme: &Theme) -> Style {
    let (background, border_color) = match theme {
        Theme::Custom(custom) => {
            if **custom == FlashCatTheme::get_custom_theme(&FlashCatTheme::Light) {
                (Some(Background::Color(color!(0xbbbbbb))), color!(0xbbbbbb))
            } else {
                (Some(Background::Color(color!(0x282828))), Color::BLACK)
            }
        }
        _ => unreachable!("built-in iced themes are not in use"),
    };

    Style {
        background,
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: border_color,
        },
        ..Style::default()
    }
}

/// A custom theme for container respecting Light and Dark FlashCatTheme designed for the tabs
pub fn first_class_container_square_theme(theme: &Theme) -> Style {
    let background = match theme {
        Theme::Custom(custom) => {
            if **custom == FlashCatTheme::get_custom_theme(&FlashCatTheme::Light) {
                Some(Background::Color(color!(0xcccccc)))
            } else {
                Some(Background::Color(color!(0x1c1c1c)))
            }
        }
        _ => unreachable!("built-in iced themes are not in use"),
    };

    Style {
        background,
        ..Style::default()
    }
}

/// A custom theme for container respecting Light and Dark FlashCatTheme designed for the tabs
pub fn second_class_container_square_theme(theme: &Theme) -> Style {
    let background = match theme {
        Theme::Custom(custom) => {
            if **custom == FlashCatTheme::get_custom_theme(&FlashCatTheme::Light) {
                Some(Background::Color(color!(0xbbbbbb)))
            } else {
                Some(Background::Color(color!(0x282828)))
            }
        }
        _ => unreachable!("built-in iced themes are not in use"),
    };

    Style {
        background,
        ..Style::default()
    }
}

/// A custom theme for container indicating content that represent success
pub fn success_container_theme(_theme: &Theme) -> Style {
    Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 128_f32 / 255.0,
            b: 0.0,
            a: 0.1,
        })),
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: Color {
                r: 0.0,
                g: 128_f32 / 255.0,
                b: 0.0,
                a: 1.0,
            },
        },
        ..Style::default()
    }
}

/// A custom theme for container indicating content that represent success
pub fn failure_container_theme(_theme: &Theme) -> Style {
    Style {
        background: Some(Background::Color(Color {
            r: 255.0,
            g: 0.0,
            b: 0.0,
            a: 0.1,
        })),
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: Color {
                r: 255.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        },
        ..Style::default()
    }
}

/// A custom theme for container indicating content that represent loading
pub fn loading_container_theme(_theme: &Theme) -> Style {
    Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.5,
            a: 0.1,
        })),
        border: Border {
            width: 1.0,
            radius: 10.0.into(),
            color: Color {
                r: 0.0,
                g: 0.0,
                b: 0.5,
                a: 1.0,
            },
        },
        ..Style::default()
    }
}
