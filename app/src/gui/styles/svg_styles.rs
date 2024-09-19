use iced::color;
use iced::widget::svg::{Status, Style};
use iced::Theme;

/// A custom theme that makes svg coloured
pub fn colored_svg_theme(_theme: &Theme, status: Status) -> Style {
    match status {
        Status::Idle => Style {
            color: Some(color!(0x8f6593)),
        },
        Status::Hovered => Style {
            color: Some(color!(0x8f6593)),
        },
    }
}
