use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, horizontal_space, mouse_area, row, text, text_input};
use iced::{Element, Task, mouse};

use super::settings_config::SETTINGS;
use crate::folder::pick_floder;
use crate::gui::styles;

#[derive(Debug, Clone)]
pub enum Message {
    RelayAddrChanged(String),
    RelayAddrSubimt,
    OpenDownloadPath(String),
    ModifySavePath,
    UpdateSavePath(Result<Option<PathBuf>, String>),
}

#[derive(Default)]
pub struct General {
    relay_addr: String,
}

impl General {
    pub fn new() -> Self {
        let relay_addr = SETTINGS.read().unwrap().get_current_settings().general.to_owned().relay_addr;
        Self {
            relay_addr,
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
                        let mut settings_write = SETTINGS.write().unwrap();
                        settings_write.change_settings().general.download_path = path.to_string_lossy().to_string();
                        settings_write.save_settings();
                    }
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let content = column![text("General").size(21).style(styles::text_styles::accent_color_theme)].padding(5).spacing(5);

        let relay_addr_widget = column![
            text("Relay Address").size(18),
            column![
                text("Flash-Cat relay address"),
                row![
                    text_input("relay address", &self.relay_addr).on_input(Message::RelayAddrChanged).on_submit(Message::RelayAddrSubimt).padding(5),
                    horizontal_space(),
                    button("Modify").on_press(Message::RelayAddrSubimt),
                ]
            ],
        ]
        .spacing(5);

        let current_download_path = SETTINGS.read().unwrap().get_current_settings().general.to_owned().download_path;

        let save_path_widget = column![
            text("Save Path").size(18),
            column![
                text("The path where the received file is saved"),
                row![
                    mouse_area(text(current_download_path.clone()).style(styles::text_styles::accent_color_theme),)
                        .interaction(mouse::Interaction::Pointer)
                        .on_press(Message::OpenDownloadPath(current_download_path.clone())),
                    horizontal_space(),
                    button("Modify").on_press(Message::ModifySavePath),
                ]
            ],
        ]
        .spacing(5);

        let content = content.push(relay_addr_widget).push(save_path_widget);

        container(content).style(styles::container_styles::first_class_container_rounded_theme).width(1000).into()
    }
}
