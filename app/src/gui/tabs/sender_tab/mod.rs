use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, RwLock};
use std::time::Duration;

use flash_cat_common::consts::PUBLIC_RELAY;
use flash_cat_common::utils::gen_share_code;
use iced::{
    font, mouse,
    widget::{button, column, container, horizontal_space, mouse_area, row, scrollable, svg, text},
    Alignment, Command, Element, Font, Length,
};
use picked_body_widget::{Message as PickedBodyMessage, PickedBody};

use crate::gui::assets::icons::{COPY_ICON, TICK_ICON};
use crate::{
    folder::{pick_files, pick_floders},
    gui::{assets::icons::SENDER_ICON, styles},
};

use super::settings_tab::settings_config::SETTINGS;
use super::Tab;

mod picked_body_widget;
mod sender;

pub(super) static SENDER_STATE: LazyLock<RwLock<SenderState>> =
    LazyLock::new(|| RwLock::new(SenderState::Idle));

pub(super) static SENDER_STREAM_STATE: LazyLock<RwLock<SenderStreamState>> =
    LazyLock::new(|| RwLock::new(SenderStreamState::Normal));

static COPYED_BUTTON_STATE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SenderState {
    Idle,
    Picked,
    AwaitingReceive,
    Sending,
    SendDone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SenderStreamState {
    Normal,
    Message(String),
    Errored(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    PickFiles,
    PickFloders,
    AddPickedPath(Result<Option<Vec<PathBuf>>, String>),
    PickedBody(PickedBodyMessage),
    Send,
    Cancel,
    CopySharCode,
    ResetCopyBut,
    StartSendError(String),
    SendButtonEnter,
    SendButtonExit,
}

pub struct SenderTab {
    share_code: String,
    picked_body: PickedBody,
    send_button_text: String,
}

impl SenderTab {
    pub fn new() -> (Self, Command<Message>) {
        (
            Self {
                share_code: String::new(),
                picked_body: PickedBody::new(Self::scrollable_id()),
                send_button_text: String::new(),
            },
            Command::none(),
        )
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        self.picked_body.subscription().map(Message::PickedBody)
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::PickFiles => Command::perform(pick_files(), |result| {
                Message::AddPickedPath(result.map_err(|err| err.to_string()))
            }),
            Message::PickFloders => Command::perform(pick_floders(), |result| {
                Message::AddPickedPath(result.map_err(|err| err.to_string()))
            }),
            Message::AddPickedPath(pick_paths_result) => {
                if let Ok(paths) = pick_paths_result {
                    *SENDER_STATE.write().unwrap() = SenderState::Picked;
                    if let Some(paths) = paths {
                        let paths = paths
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect::<Vec<_>>();
                        self.picked_body
                            .update(PickedBodyMessage::Add(paths))
                            .map(Message::PickedBody)
                    } else {
                        Command::none()
                    }
                } else {
                    Command::none()
                }
            }
            Message::PickedBody(message) => {
                self.picked_body.update(message).map(Message::PickedBody)
            }
            Message::Send => {
                *SENDER_STATE.write().unwrap() = SenderState::AwaitingReceive;

                let relay_addr = SETTINGS
                    .read()
                    .unwrap()
                    .get_current_settings()
                    .general
                    .relay_addr
                    .to_owned();
                let relay = if relay_addr.contains(PUBLIC_RELAY) {
                    None
                } else {
                    Some(relay_addr)
                };
                self.share_code = gen_share_code();
                self.picked_body
                    .update(PickedBodyMessage::StartSend(self.share_code.clone(), relay))
                    .map(Message::PickedBody)
            }
            Message::Cancel => {
                *SENDER_STATE.write().unwrap() = SenderState::Picked;
                self.send_button_text = "".to_string();
                self.picked_body
                    .update(PickedBodyMessage::CancelSend)
                    .map(Message::PickedBody)
            }
            Message::CopySharCode => {
                COPYED_BUTTON_STATE.store(true, Ordering::Relaxed);
                Command::batch([
                    iced::clipboard::write(self.share_code.clone()),
                    Command::perform(
                        async move { tokio::time::sleep(Duration::from_secs(3)).await },
                        |_| Message::ResetCopyBut,
                    ),
                ])
            }
            Message::ResetCopyBut => {
                COPYED_BUTTON_STATE.store(false, Ordering::Relaxed);
                Command::none()
            }
            Message::StartSendError(_err) => Command::none(),
            Message::SendButtonEnter => {
                let sender_state = SENDER_STATE.read().unwrap();
                if sender_state.eq(&SenderState::Sending)
                    || sender_state.eq(&SenderState::AwaitingReceive)
                {
                    self.send_button_text = "Cancel".to_string();
                }
                Command::none()
            }
            Message::SendButtonExit => {
                self.send_button_text = "".to_string();
                Command::none()
            }
        }
    }

    pub fn view(&self) -> Element<Message> {
        let sender_state = SENDER_STATE.read().unwrap();

        let mut pick_files_button = button("Files");
        let mut pick_floders_button = button("Floders");
        if sender_state.eq(&SenderState::Idle) || sender_state.eq(&SenderState::Picked) {
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

        let mut send_button = button(row![
            horizontal_space(),
            text(if self.send_button_text.is_empty() {
                if sender_state.eq(&SenderState::AwaitingReceive) {
                    "Awaiting receive..."
                } else if sender_state.eq(&SenderState::Sending) {
                    "Sending..."
                } else if sender_state.eq(&SenderState::SendDone) {
                    "Done"
                } else {
                    "Send"
                }
            } else {
                self.send_button_text.as_str()
            })
            .size(18),
            horizontal_space()
        ])
        .width(Length::Fill);

        if sender_state.eq(&SenderState::Picked) {
            send_button = send_button.on_press(Message::Send);
        } else if !self.send_button_text.is_empty() {
            send_button = send_button.on_press(Message::Cancel);
        }

        let send_button = mouse_area(send_button)
            .on_enter(Message::SendButtonEnter)
            .on_exit(Message::SendButtonExit);

        let share_code = if sender_state.eq(&SenderState::AwaitingReceive) {
            row![
                text("Share Code"),
                mouse_area(
                    container(text(self.share_code.as_str()).font(Font {
                        weight: font::Weight::Bold,
                        ..Default::default()
                    }))
                    .style(styles::container_styles::first_class_container_square_theme())
                    .padding(5)
                    .align_y(iced::alignment::Vertical::Center)
                )
                .interaction(mouse::Interaction::Pointer)
                .on_press(Message::CopySharCode),
                if COPYED_BUTTON_STATE.load(Ordering::Relaxed) {
                    let copyed_icon_handle = svg::Handle::from_memory(TICK_ICON);
                    let copyed_icon = svg(copyed_icon_handle)
                        .style(styles::svg_styles::colored_svg_theme())
                        .height(20)
                        .width(20);
                    row![
                        button(copyed_icon)
                            .style(styles::button_styles::transparent_button_theme())
                            .on_press(Message::CopySharCode),
                        text("Copyed").style(styles::text_styles::accent_color_theme())
                    ]
                    .align_items(Alignment::Center)
                } else {
                    let copy_icon_handle = svg::Handle::from_memory(COPY_ICON);
                    let copy_icon = svg(copy_icon_handle)
                        .style(styles::svg_styles::colored_svg_theme())
                        .height(20)
                        .width(20);
                    row![button(copy_icon)
                        .style(styles::button_styles::transparent_button_theme())
                        .on_press(Message::CopySharCode)]
                }
            ]
            .spacing(5)
            .align_items(Alignment::Center)
        } else {
            row![]
        };

        column![
            pick,
            self.picked_body.view().map(Message::PickedBody),
            send_button,
            share_code
        ]
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
        self.picked_body.get_scrollable_offset()
    }
}
