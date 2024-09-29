use iced::theme::{palette, Theme};
use iced::widget::button::{Status, Style};
use iced::{Background, Border};

/// A custom theme that makes button transparent
pub fn transparent_button_theme(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();
    let base = transparent_styled(palette.success.base);

    match status {
        Status::Active | Status::Pressed | Status::Hovered => base,
        Status::Disabled => disabled(base),
    }
}

/// A custom theme that makes button transparent, with rounded border
pub fn transparent_button_with_rounded_border_theme(theme: &Theme, status: Status) -> Style {
    let palette = theme.extended_palette();
    let base = transparent_with_rounded_border_styled(palette.success.base);

    match status {
        Status::Active | Status::Pressed | Status::Hovered => base,
        Status::Disabled => disabled(base),
    }
}

fn transparent_styled(pair: palette::Pair) -> Style {
    Style {
        background: Some(Background::Color(iced::Color::TRANSPARENT)),
        text_color: pair.text,
        ..Style::default()
    }
}

fn transparent_with_rounded_border_styled(pair: palette::Pair) -> Style {
    Style {
        background: Some(Background::Color(iced::Color::TRANSPARENT)),
        text_color: pair.text,
        border: Border {
            color: super::colors::accent_color(),
            width: 1.0,
            radius: 10.0.into(),
        },
        ..Style::default()
    }
}

fn disabled(style: Style) -> Style {
    Style {
        background: style
            .background
            .map(|background| background.scale_alpha(0.5)),
        text_color: style.text_color.scale_alpha(0.5),
        ..style
    }
}
