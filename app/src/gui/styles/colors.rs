use iced::{Color, color};

/// The accent color for the program
pub fn accent_color() -> Color {
    purple()
}

pub fn red() -> Color {
    Color::from_rgb(2.55, 0.0, 0.0)
}

pub fn purple() -> Color {
    color!(0x8f6593)
}

pub fn green() -> Color {
    color!(0x008000)
}

pub fn gray() -> Color {
    color!(0x282828)
}

pub fn brown() -> Color {
    color!(0x563009)
}
