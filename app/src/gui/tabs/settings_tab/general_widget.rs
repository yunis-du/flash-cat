use std::path::{Path, PathBuf};

use iced::{
    Element, Length, Task, mouse,
    widget::{button, column, container, mouse_area, pick_list, row, space, text, text_input},
};
use rust_i18n::t;

use super::settings_config::SETTINGS;
use crate::{folder::pick_floder, gui::styles};

#[derive(Debug, Clone)]
pub enum Message {
    RelayAddrChanged(String),
    RelayAddrSubimt,
    OpenDownloadPath(String),
    ModifySavePath,
    UpdateSavePath(Result<Option<PathBuf>, String>),
    I18nChanged(I18n),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I18n {
    English,
    Chinese,
}

impl I18n {
    const ALL: [I18n; 2] = [I18n::English, I18n::Chinese];

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "en" => Some(I18n::English),
            "zh" => Some(I18n::Chinese),
            _ => None,
        }
    }

    fn to_str(&self) -> &'static str {
        match self {
            I18n::English => "en",
            I18n::Chinese => "zh",
        }
    }
}

impl std::fmt::Display for I18n {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            I18n::English => write!(f, "English"),
            I18n::Chinese => write!(f, "简体中文"),
        }
    }
}

#[derive(Default)]
pub struct General {
    relay_addr: String,
    i18n: Option<I18n>,
    download_path: String,
}

impl General {
    pub fn new() -> Self {
        let settings = SETTINGS.read().unwrap();
        let config = settings.get_current_settings();
        let relay_addr = config.general.relay_addr.clone();
        let i18n = I18n::from_str(&config.general.i18n);
        let download_path = config.general.download_path.clone();
        Self {
            relay_addr,
            i18n,
            download_path,
        }
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> Task<Message> {
        match message {
            Message::RelayAddrChanged(input) => {
                self.relay_addr = input;
                Task::none()
            }
            Message::RelayAddrSubimt => {
                let mut settings_write = SETTINGS.write().unwrap();
                settings_write.change_settings().general.relay_addr = self.relay_addr.clone();
                settings_write.save_settings();
                Task::none()
            }
            Message::OpenDownloadPath(path) => {
                let _ = open::that(Path::new(path.as_str()));
                Task::none()
            }
            Message::ModifySavePath => Task::perform(pick_floder(), |result| {
                Message::UpdateSavePath(result.map_err(|err| err.to_string()))
            }),
            Message::UpdateSavePath(save_path_result) => {
                if let Ok(path) = save_path_result {
                    if let Some(path) = path {
                        let download_path = path.to_string_lossy().to_string();
                        self.download_path = download_path.clone();

                        let mut settings_write = SETTINGS.write().unwrap();
                        settings_write.change_settings().general.download_path = download_path;
                        settings_write.save_settings();
                    }
                }
                Task::none()
            }
            Message::I18nChanged(i18n) => {
                self.i18n = Some(i18n);
                let mut settings_write = SETTINGS.write().unwrap();
                settings_write.change_settings().general.i18n = i18n.to_str().to_string();
                settings_write.save_settings();
                rust_i18n::set_locale(i18n.to_str());
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = column![text(t!("app.tab.settings.general-widget.title")).size(21).style(styles::text_styles::accent_color_theme)].padding(5).spacing(5);

        let i18n_widget =
            column![text(t!("app.tab.settings.general-widget.language")).size(18), column![pick_list(&I18n::ALL[..], self.i18n, Message::I18nChanged)],]
                .spacing(5);

        let relay_addr_widget = column![
            text(t!("app.tab.settings.general-widget.relay-address")).size(18),
            column![
                text(t!("app.tab.settings.general-widget.relay-address-description")),
                row![
                    text_input(
                        t!("app.tab.settings.general-widget.relay-address-low").as_ref(),
                        &self.relay_addr
                    )
                    .on_input(Message::RelayAddrChanged)
                    .on_submit(Message::RelayAddrSubimt)
                    .padding(5),
                    space().width(Length::Fill),
                    button(text(t!("app.tab.settings.general-widget.save"))).on_press(Message::RelayAddrSubimt),
                ]
            ],
        ]
        .spacing(5);

        let save_path_widget = column![
            text(t!("app.tab.settings.general-widget.save-path")).size(18),
            column![
                text(t!("app.tab.settings.general-widget.save-path-description")),
                row![
                    mouse_area(text(self.download_path.clone()).style(styles::text_styles::accent_color_theme),)
                        .interaction(mouse::Interaction::Pointer)
                        .on_press(Message::OpenDownloadPath(self.download_path.clone())),
                    space().width(Length::Fill),
                    button(text(t!("app.tab.settings.general-widget.save"))).on_press(Message::ModifySavePath),
                ]
            ],
        ]
        .spacing(5);

        let content = content.push(i18n_widget).push(relay_addr_widget).push(save_path_widget);

        container(content).style(styles::container_styles::first_class_container_rounded_theme).width(1000).into()
    }
}
