use std::sync::Arc;

use iced::{
    font::Weight, widget::{column, container, image, row, text}, Application, Command, Element, Font, Length
};
use tabs::{
    settings_tab::settings_config::{self, SETTINGS},
    Message as TabsControllerMessage, TabId, TabsController,
};
use title_bar::{Message as TitleBarMessage, TitleBar};

pub mod assets;
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

impl<'a> Application for FlashCatApp {
    type Executor = iced::executor::Default;

    type Message = Message;

    type Theme = iced::Theme;

    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let (tabs_controller, tabs_controller_command) = TabsController::new();
        (
            Self {
                active_tab: TabId::Sender,
                title_bar: TitleBar::new(),
                tabs_controller,
            },
            Command::batch([tabs_controller_command.map(Message::TabsController)]),
        )
    }

    fn title(&self) -> String {
        format!("FlashCat - {}", self.active_tab.to_string())
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::TitleBar(message) => {
                self.title_bar.update(message.clone());
                match message {
                    TitleBarMessage::TabSelected(tab_id) => {
                        let tab_id: TabId = tab_id.into();
                        self.active_tab = tab_id;
                        self.tabs_controller
                            .switch_to_tab(tab_id)
                            .map(Message::TabsController)
                    }
                    TitleBarMessage::BackButtonPressed => Command::none(),
                }
            }
            Message::TabsController(message) => Command::batch([self
                .tabs_controller
                .update(message)
                .map(Message::TabsController)]),
        }
    }

    fn view(&self) -> Element<Message> {
        column![
            logo_widget(),
            self.title_bar
                .view(&self.tabs_controller.get_labels(), false)
                .map(Message::TitleBar),
            self.tabs_controller.view().map(Message::TabsController)
        ]
        .into()
    }

    fn theme(&self) -> iced::Theme {
        let custom_theme = Arc::new(
            match SETTINGS
                .read()
                .unwrap()
                .get_current_settings()
                .appearance
                .theme
            {
                settings_config::Theme::Light => styles::theme::FlashCatTheme::Light,
                settings_config::Theme::Dark => styles::theme::FlashCatTheme::Dark,
            }
            .get_custom_theme(),
        );
        iced::Theme::Custom(custom_theme)
    }
}

pub mod title_bar {
    use iced::widget::{
        button, container, horizontal_space, mouse_area, row, svg, text, Row, Space,
    };
    use iced::{Element, Length};

    use crate::gui::{assets::icons::CARET_LEFT_FILL, styles, tabs::TabLabel};

    #[derive(Clone, Debug)]
    pub enum Message {
        TabSelected(usize),
        BackButtonPressed,
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

        pub fn update(&mut self, message: Message) {
            if let Message::TabSelected(new_active_tab) = message {
                self.active_tab = new_active_tab
            }
        }

        pub fn view(
            &self,
            tab_labels: &[TabLabel],
            show_back_button: bool,
        ) -> iced::Element<'_, Message> {
            let tab_views = tab_labels.iter().enumerate().map(|(index, tab_label)| {
                let svg_handle = svg::Handle::from_memory(tab_label.icon);
                let icon = svg(svg_handle)
                    .width(Length::Shrink)
                    .style(styles::svg_styles::colored_svg_theme());
                let text_label = text(tab_label.text);
                let mut tab = container(
                    mouse_area(row![icon, text_label].spacing(5))
                        .on_press(Message::TabSelected(index)),
                )
                .padding(5);

                // Highlighting the tab if is active
                if index == self.active_tab {
                    tab = tab.style(styles::container_styles::second_class_container_square_theme())
                };
                tab.into()
            });

            let tab_views = Row::with_children(tab_views).spacing(10);

            let back_button: Element<'_, Message> = if show_back_button {
                let back_button_icon_handle = svg::Handle::from_memory(CARET_LEFT_FILL);
                let icon = svg(back_button_icon_handle)
                    .width(20)
                    .style(styles::svg_styles::colored_svg_theme());
                button(icon)
                    .on_press(Message::BackButtonPressed)
                    .style(styles::button_styles::transparent_button_theme())
                    .into()
            } else {
                Space::new(0, 0).into()
            };

            container(row![
                back_button,
                horizontal_space(),
                tab_views,
                horizontal_space()
            ])
            .style(styles::container_styles::first_class_container_square_theme())
            .into()
        }
    }
}

fn logo_widget() -> Element<'static, Message> {
    let logo_image = image(image::Handle::from_memory(assets::logos::IMG_LOGO)).height(65.0);
    let logo_text = text("Flash Cat")
        .size(20)
        .font(Font {
            weight: Weight::Bold,
            ..Default::default()
        })
        .style(styles::text_styles::brown_text_theme());
    container(row![logo_image, logo_text].align_items(iced::Alignment::Center).spacing(10))
        .width(Length::Fill)
        .center_x()
        .center_y()
        .into()
}
