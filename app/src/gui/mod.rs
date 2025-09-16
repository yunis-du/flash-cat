use std::sync::Arc;

use iced::{
    Element, Font, Length, Task,
    font::Weight,
    widget::{column, container, image, row, text},
};
use tabs::{
    Message as TabsControllerMessage, TabId, TabsController,
    settings_tab::settings_config::{self, SETTINGS},
};
use title_bar::{Message as TitleBarMessage, TitleBar};

pub mod assets;
pub mod progress_bar_widget;
pub mod styles;
pub mod tabs;

pub struct FlashCatApp {
    active_tab: TabId,
    title_bar: TitleBar,
    tabs_controller: TabsController,
}

#[derive(Debug, Clone)]
pub enum Message {
    TitleBar(TitleBarMessage),
    TabsController(TabsControllerMessage),
}

impl FlashCatApp {
    pub fn title(&self) -> String {
        format!("FlashCat - {}", self.active_tab.to_string())
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        self.tabs_controller.subscription().map(Message::TabsController)
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> Task<Message> {
        match message {
            Message::TitleBar(message) => {
                self.title_bar.update(message.clone());
                match message {
                    TitleBarMessage::TabSelected(tab_id) => {
                        let tab_id: TabId = tab_id.into();
                        self.active_tab = tab_id;
                        self.tabs_controller.switch_to_tab(tab_id).map(Message::TabsController)
                    }
                }
            }
            Message::TabsController(message) => Task::batch([self.tabs_controller.update(message).map(Message::TabsController)]),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        column![
            logo_widget(),
            self.title_bar.view(&self.tabs_controller.get_labels()).map(Message::TitleBar),
            self.tabs_controller.view().map(Message::TabsController)
        ]
        .into()
    }

    pub fn theme(&self) -> iced::Theme {
        let custom_theme = Arc::new(
            match SETTINGS.read().unwrap().get_current_settings().appearance.theme {
                settings_config::Theme::Light => styles::theme::FlashCatTheme::Light,
                settings_config::Theme::Dark => styles::theme::FlashCatTheme::Dark,
            }
            .get_custom_theme(),
        );
        iced::Theme::Custom(custom_theme)
    }
}

impl Default for FlashCatApp {
    fn default() -> Self {
        let (tabs_controller, _) = TabsController::new();
        Self {
            active_tab: TabId::Sender,
            title_bar: TitleBar::new(),
            tabs_controller,
        }
    }
}

pub mod title_bar {
    use iced::widget::{Row, container, horizontal_space, mouse_area, row, svg, text};
    use iced::{Alignment, Length};

    use crate::gui::{styles, tabs::TabLabel};

    #[derive(Clone, Debug)]
    pub enum Message {
        TabSelected(usize),
    }

    pub struct TitleBar {
        active_tab: usize,
    }

    impl TitleBar {
        pub fn new() -> Self {
            Self {
                active_tab: usize::default(),
            }
        }

        pub fn update(
            &mut self,
            message: Message,
        ) {
            match message {
                Message::TabSelected(new_active_tab) => self.active_tab = new_active_tab,
            }
        }

        pub fn view(
            &self,
            tab_labels: &[TabLabel],
        ) -> iced::Element<'_, Message> {
            let tab_views = tab_labels.iter().enumerate().map(|(index, tab_label)| {
                let svg_handle = svg::Handle::from_memory(tab_label.icon);
                let icon = svg(svg_handle).width(Length::Shrink).style(styles::svg_styles::colored_svg_theme);
                let text_label = text(tab_label.text).size(18);
                let mut tab =
                    container(mouse_area(row![icon, text_label].align_y(Alignment::Center).spacing(5)).on_press(Message::TabSelected(index))).padding(5);

                // Highlighting the tab if is active
                if index == self.active_tab {
                    tab = tab.style(styles::container_styles::second_class_container_square_theme)
                };
                tab.into()
            });

            let tab_views = Row::with_children(tab_views).spacing(10);

            container(row![horizontal_space(), tab_views, horizontal_space()]).style(styles::container_styles::first_class_container_square_theme).into()
        }
    }
}

fn logo_widget() -> Element<'static, Message> {
    let logo_image = image(image::Handle::from_bytes(assets::logos::IMG_LOGO)).height(65.0);
    let logo_text = text("Flash Cat")
        .size(20)
        .font(Font {
            weight: Weight::Bold,
            ..Default::default()
        })
        .style(styles::text_styles::brown_text_theme);
    container(row![logo_image, logo_text].align_y(iced::Alignment::Center).spacing(10))
        .width(Length::Fill)
        .center_x(Length::Fill)
        // .center_y(Length::Fill)
        .into()
}
