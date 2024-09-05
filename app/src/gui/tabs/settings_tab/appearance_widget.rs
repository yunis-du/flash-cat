use iced::{
    widget::{column, container, radio, text, Column, Space},
    Command, Element, Length,
};

use super::settings_config::{Theme, ALL_THEMES, SETTINGS};
use crate::gui::styles;

#[derive(Debug, Clone)]
pub enum Message {
    ThemeSelected(Theme),
}

#[derive(Default)]
pub struct Appearance;

impl Appearance {
    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::ThemeSelected(theme) => {
                let mut settings_write = SETTINGS.write().unwrap();
                settings_write.change_settings().appearance.theme = theme;
                settings_write.save_settings();

                Command::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let content = column![text("Appearance")
            .size(21)
            .style(styles::text_styles::accent_color_theme())]
        .padding(5)
        .spacing(5);

        let theme_text = text("Theme").size(18);

        let current_theme = {
            let settings = SETTINGS
                .read()
                .unwrap()
                .get_current_settings()
                .appearance
                .to_owned();
            Some(settings.theme)
        };

        let theme_list = Column::with_children(ALL_THEMES.iter().map(|theme| {
            let elem: Element<'_, Message> =
                radio(theme.to_string(), theme, current_theme.as_ref(), |theme| {
                    Message::ThemeSelected(theme.clone())
                })
                .into();
            elem
        }))
        .spacing(5);

        let content = content.push(
            column!(theme_text, Space::with_width(20), theme_list)
                .padding(5)
                .spacing(5),
        );

        container(content)
            .style(styles::container_styles::first_class_container_rounded_theme())
            .width(Length::Fill)
            .into()
    }
}
