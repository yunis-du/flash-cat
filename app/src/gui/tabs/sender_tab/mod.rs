use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use flash_cat_common::consts::PUBLIC_RELAY;
use flash_cat_common::utils::gen_share_code;
use flash_cat_core::sender::FlashCatSender;
use iced::mouse;
use iced::widget::scrollable::{RelativeOffset, Viewport};
use iced::widget::{checkbox, mouse_area, svg};
use iced::{
    widget::{button, column, container, horizontal_space, row, scrollable, text, Space},
    Alignment, Command, Element, Length,
};
use iced_aw::Wrap;
use pick_path_widget::{Message as PickPathMessage, PickPath};

use crate::gui::assets::icons::{COPY_ICON, TICK_ICON};
use crate::message::IndexedMessage;
use crate::{
    folder::{pick_files, pick_floders},
    gui::{assets::icons::SENDER_ICON, styles},
};

use super::settings_tab::settings_config::SETTINGS;
use super::Tab;

mod pick_path_widget;

pub(super) static SENDER_STATE: LazyLock<RwLock<SenderState>> =
    LazyLock::new(|| RwLock::new(SenderState::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SenderState {
    Ready,
    Picked,
    StartSend,
    Sending,
    SendDone,
}

impl SenderState {
    fn new() -> Self {
        SenderState::Ready
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    PickFiles,
    PickFloders,
    PickedPageScrolled(Viewport),
    UpdatePickPath(Result<Option<Vec<PathBuf>>, String>),
    PickPaths(IndexedMessage<String, PickPathMessage>),
    RemoveAll,
    CheckBoxZip(bool),
    Send,
    CopySharCode,
    StartSend,
}

pub struct SenderTab {
    picked_paths: HashMap<String, PickPath>,
    picked_scrollable_offset: RelativeOffset,
    zip_folder: bool,
    share_code: String,
    copyed_share_code: bool,
}

impl SenderTab {
    pub fn new() -> (Self, Command<Message>) {
        (
            Self {
                picked_paths: HashMap::new(),
                picked_scrollable_offset: RelativeOffset::START,
                zip_folder: false,
                share_code: String::new(),
                copyed_share_code: false,
            },
            Command::none(),
        )
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::PickFiles => Command::perform(pick_files(), |result| {
                Message::UpdatePickPath(result.map_err(|err| err.to_string()))
            }),
            Message::PickFloders => Command::perform(pick_floders(), |result| {
                Message::UpdatePickPath(result.map_err(|err| err.to_string()))
            }),
            Message::PickedPageScrolled(view_port) => {
                self.picked_scrollable_offset = view_port.relative_offset();
                Command::none()
            }
            Message::UpdatePickPath(pick_paths_result) => {
                if let Ok(paths) = pick_paths_result {
                    if let Some(paths) = paths {
                        for path in paths {
                            if !self
                                .picked_paths
                                .contains_key(path.to_str().unwrap_or_default())
                            {
                                let path = path.to_string_lossy().to_string();
                                self.picked_paths.insert(path.clone(), PickPath::new(path));
                            }
                        }
                    }
                }
                if !self.picked_paths.is_empty() {
                    *SENDER_STATE.write().unwrap() = SenderState::Picked;
                }
                Command::none()
            }
            Message::PickPaths(message) => {
                let index = &message.index();
                match message.message() {
                    PickPathMessage::Remove => {
                        self.picked_paths.remove(index);
                        if self.picked_paths.is_empty() {
                            *SENDER_STATE.write().unwrap() = SenderState::Ready;
                        }
                        Command::none()
                    }
                }
            }
            Message::RemoveAll => {
                self.picked_paths.clear();
                *SENDER_STATE.write().unwrap() = SenderState::Ready;
                Command::none()
            }
            Message::CheckBoxZip(zip_folder) => {
                self.zip_folder = zip_folder;
                Command::none()
            }
            Message::Send => {
                *SENDER_STATE.write().unwrap() = SenderState::StartSend;

                let relay_addr = SETTINGS
                    .read()
                    .unwrap()
                    .get_current_settings()
                    .general
                    .relay_addr
                    .to_owned();
                let relay = if relay_addr.eq(PUBLIC_RELAY) {
                    None
                } else {
                    Some(relay_addr)
                };
                self.share_code = gen_share_code();
                let files = self
                    .picked_paths
                    .values()
                    .map(|p| p.get_path())
                    .collect::<Vec<_>>();
                Command::perform(
                    FlashCatSender::new(self.share_code.clone(), relay, files, self.zip_folder),
                    |result| {
                        if let Ok(flash_cat_sender) = result {
                            println!("flash_cat_sender: {:?}", flash_cat_sender.get_file_collector());
                        }
                        Message::StartSend
                    }
                )
            }
            Message::CopySharCode => {
                self.copyed_share_code = true;
                iced::clipboard::write(self.share_code.clone())
            }
            Message::StartSend => Command::none(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let sender_state = SENDER_STATE.read().unwrap();

        let mut pick_files_button = button("Files");
        let mut pick_floders_button = button("Floders");
        if sender_state.eq(&SenderState::Ready) || sender_state.eq(&SenderState::Picked) {
            pick_files_button = pick_files_button.on_press(Message::PickFiles);
            pick_floders_button = pick_floders_button.on_press(Message::PickFloders);
        }
        let pick = column![row![
            text("Pick file(s) or folder(s) to send").size(16),
            horizontal_space(),
            row![
                text("Pick:")
                    .size(16)
                    .style(styles::text_styles::accent_color_theme()),
                pick_files_button,
                pick_floders_button
            ]
            .align_items(Alignment::Center)
            .spacing(5),
        ]
        .align_items(Alignment::Center)]
        .padding(5);

        let picked_body = column![
            if self.picked_paths.len() > 1
                && (sender_state.eq(&SenderState::Ready) || sender_state.eq(&SenderState::Picked))
            {
                row![
                    horizontal_space(),
                    button("Remove All").on_press(Message::RemoveAll)
                ]
            } else {
                row![Space::new(0, 28)]
            },
            container(
                scrollable(
                    column![pick_paths_viewer(self.picked_paths.values().collect())
                        .map(Message::PickPaths)]
                    .width(Length::Fill)
                    .align_items(Alignment::Start)
                    .padding(10),
                )
                .id(Self::scrollable_id())
                .height(300)
                .on_scroll(Message::PickedPageScrolled)
                .direction(styles::scrollable_styles::vertical_direction()),
            )
            .style(styles::container_styles::first_class_container_rounded_theme())
            .width(Length::Fill)
        ]
        .spacing(2);

        let mut zip_option = checkbox("Zip folder before send", self.zip_folder);

        if sender_state.eq(&SenderState::Ready) || sender_state.eq(&SenderState::Picked) {
            zip_option = zip_option.on_toggle(Message::CheckBoxZip);
        }

        let mut send_button = button(row![
            horizontal_space(),
            text({
                if sender_state.eq(&SenderState::StartSend) {
                    "Awaiting receive..."
                } else if sender_state.eq(&SenderState::Sending) {
                    "Sending..."
                } else if sender_state.eq(&SenderState::SendDone) {
                    "Done"
                } else {
                    "Send"
                }
            })
            .size(18),
            horizontal_space()
        ])
        .width(Length::Fill);

        if sender_state.eq(&SenderState::Picked) {
            send_button = send_button.on_press(Message::Send);
        }

        let share_code = if sender_state.eq(&SenderState::StartSend) {
            row![
                text("Share Code"),
                mouse_area(
                    container(text(self.share_code.as_str()))
                        .style(styles::container_styles::first_class_container_square_theme())
                        .padding(5)
                        .align_y(iced::alignment::Vertical::Center)
                )
                .interaction(mouse::Interaction::Pointer)
                .on_press(Message::CopySharCode),
                if self.copyed_share_code {
                    let copyed_icon_handle = svg::Handle::from_memory(TICK_ICON);
                    let copyed_icon = svg(copyed_icon_handle)
                        .style(styles::svg_styles::colored_svg_theme())
                        .height(20)
                        .width(20);
                    button(copyed_icon)
                        .style(styles::button_styles::transparent_button_theme())
                        .on_press(Message::CopySharCode)
                } else {
                    let copy_icon_handle = svg::Handle::from_memory(COPY_ICON);
                    let copy_icon = svg(copy_icon_handle)
                        .style(styles::svg_styles::colored_svg_theme())
                        .height(20)
                        .width(20);
                    button(copy_icon)
                        .style(styles::button_styles::transparent_button_theme())
                        .on_press(Message::CopySharCode)
                }
            ]
            .spacing(5)
            .align_items(Alignment::Center)
        } else {
            row![]
        };

        column![pick, picked_body, zip_option, send_button, share_code]
            .padding(5)
            .spacing(5)
            .into()
    }
}

impl Tab for SenderTab {
    type Message = Message;

    fn title() -> &'static str {
        "Sender"
    }

    fn icon_bytes() -> &'static [u8] {
        SENDER_ICON
    }

    fn get_scrollable_offset(&self) -> scrollable::RelativeOffset {
        self.picked_scrollable_offset
    }
}

fn pick_paths_viewer<'a>(
    paths: Vec<&'a PickPath>,
) -> Element<'a, IndexedMessage<String, PickPathMessage>> {
    if paths.is_empty() {
        let text = container(text("No File(s) or Folder(s) Picked"))
            .center_x()
            .center_y()
            .height(100)
            .width(Length::Fill);
        column!(text).width(Length::Fill).padding(10).into()
    } else {
        let wrapped_paths =
            Wrap::with_elements_vertical(paths.iter().map(|path| path.view()).collect())
                .spacing(2.0)
                .line_spacing(5.0);

        column!(wrapped_paths).spacing(5).width(Length::Fill).into()
    }
}
