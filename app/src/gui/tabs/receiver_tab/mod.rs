use std::{
    path::Path,
    sync::{Arc, LazyLock, RwLock, atomic::Ordering},
};

use iced::{
    Alignment,
    widget::{
        Column,
        scrollable::{Id, RelativeOffset, Viewport},
        tooltip::Position,
    },
};
use iced::{
    Element, Font, Length, Task, font,
    widget::{button, checkbox, column, container, horizontal_space, row, scrollable, svg, text, text_input, tooltip},
};
use receiver::{Error, Progress, RECV_NUM_FILES, recv};

use flash_cat_common::{consts::PUBLIC_RELAY, proto::ClientType};
use flash_cat_core::{ReceiverConfirm, receiver::FlashCatReceiver};

use super::{Tab, settings_tab::settings_config::SETTINGS};
use crate::gui::{
    assets::icons::{HELP_ICON, RECEIVER_ICON},
    progress_bar_widget::{Message as ProgressBarMessage, ProgressBar, State as ProgressBarState},
    styles,
};

mod receiver;

pub(super) static RECEIVER_STATE: LazyLock<RwLock<ReceiverState>> = LazyLock::new(|| RwLock::new(ReceiverState::Idle));

pub(super) static RECEIVER_NOTIFICATION: LazyLock<RwLock<ReceiverNotification>> = LazyLock::new(|| RwLock::new(ReceiverNotification::Normal));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReceiverState {
    Idle,
    Recving,
    RecvDone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReceiverNotification {
    Normal,
    Message(String),
    Errored(String),
    Confirm(ConfirmType, String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmType {
    Receive,
    FileDuplication(u64),
    OpenSavePath,
}

#[derive(Debug, Clone)]
pub enum Message {
    PageScrolled(Viewport),
    ShareCodeChanged(String),
    LanChanged(bool),
    Receive,
    ProgressBar(ProgressBarMessage),
    ReceiveProgressed(Result<(u64, Progress), Error>),
    Confirm(ConfirmType, bool),
    ConfirmResult,
    RecvDone,
}

pub struct ReceiverTab {
    scrollable_offset: RelativeOffset,
    scrollable_id: Id,
    share_code: String,
    lan: bool,
    fcr: Option<Arc<FlashCatReceiver>>,
    progress_bars: Vec<ProgressBar>,
}

impl ReceiverTab {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                scrollable_offset: RelativeOffset::START,
                scrollable_id: Self::scrollable_id(),
                share_code: String::new(),
                lan: false,
                fcr: None,
                progress_bars: vec![],
            },
            Task::none(),
        )
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let mut batch = self.progress_bars.iter().map(|progress_bar| progress_bar.subscription().map(Message::ProgressBar)).collect::<Vec<_>>();

        batch.push(if RECEIVER_STATE.read().unwrap().eq(&ReceiverState::Recving) {
            if self.fcr.is_some() {
                recv(self.fcr.clone().unwrap()).map(Message::ReceiveProgressed)
            } else {
                iced::Subscription::none()
            }
        } else {
            iced::Subscription::none()
        });
        iced::Subscription::batch(batch)
    }

    pub fn update(
        &mut self,
        message: Message,
    ) -> Task<Message> {
        match message {
            Message::PageScrolled(view_port) => {
                self.scrollable_offset = view_port.relative_offset();
            }
            Message::ShareCodeChanged(share_code) => {
                self.share_code = share_code;
            }
            Message::LanChanged(lan) => {
                self.lan = lan;
            }
            Message::Receive => {
                let settings = SETTINGS.read().unwrap();

                let relay_addr = settings.get_current_settings().general.relay_addr.to_owned();
                let relay = if relay_addr.contains(PUBLIC_RELAY) {
                    None
                } else {
                    Some(relay_addr)
                };
                let output = Some(settings.get_current_settings().general.download_path.to_owned());
                let fcr = FlashCatReceiver::new(self.share_code.clone(), relay, output, ClientType::App, self.lan);
                match fcr {
                    Ok(fcr) => {
                        self.fcr.replace(Arc::new(fcr));
                        *RECEIVER_STATE.write().unwrap() = ReceiverState::Recving;
                    }
                    Err(e) => {
                        *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Errored(e.to_string());
                    }
                }
            }
            Message::ProgressBar(_) => {}
            Message::ReceiveProgressed(progress) => match progress {
                Ok((file_id, progress)) => {
                    if let receiver::Progress::New(file_id, file_name, file_size) = progress.clone() {
                        self.progress_bars.push(ProgressBar::new(
                            file_id,
                            file_name.to_owned(),
                            file_size,
                            RECV_NUM_FILES.load(Ordering::Relaxed),
                        ))
                    } else {
                        let new_state = match progress {
                            receiver::Progress::Received(recv) => Some(ProgressBarState::Progress(recv)),
                            receiver::Progress::Finished => Some(ProgressBarState::Finished),
                            receiver::Progress::Skip => Some(ProgressBarState::Skip),
                            _ => None,
                        };
                        let progress_bar = self.progress_bars.iter_mut().find(|progress_bar| progress_bar.get_id().eq(&file_id));
                        if let Some(progress_bar) = progress_bar {
                            progress_bar.update_state(new_state);
                        }
                    }
                }
                Err(e) => match e {
                    Error::ShareCodeNotFound => {
                        *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Errored("Not found, Please check share code.".to_string())
                    }
                    Error::OtherErroe(err_msg) => *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Errored(err_msg),
                },
            },
            Message::Confirm(confirm_type, confirm) => {
                if self.fcr.is_some() {
                    let fcr = self.fcr.clone().unwrap();
                    match confirm_type {
                        ConfirmType::Receive => {
                            return Task::perform(
                                async move { fcr.send_confirm(ReceiverConfirm::ReceiveConfirm(confirm)).await },
                                |result| {
                                    if let Err(_) = result {
                                        // todo handle error
                                    }
                                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Normal;
                                    Message::ConfirmResult
                                },
                            );
                        }
                        ConfirmType::FileDuplication(file_id) => {
                            if !confirm {
                                let progress_bar = self.progress_bars.iter_mut().find(|progress_bar| progress_bar.get_id().eq(&file_id));
                                if let Some(progress_bar) = progress_bar {
                                    progress_bar.update_state(Some(ProgressBarState::Skip));
                                }
                            }
                            return Task::perform(
                                async move { fcr.send_confirm(ReceiverConfirm::FileConfirm((confirm, file_id))).await },
                                |result| {
                                    if let Err(_) = result {
                                        // todo handle error
                                    }
                                    *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Normal;
                                    Message::ConfirmResult
                                },
                            );
                        }
                        ConfirmType::OpenSavePath => {
                            if confirm {
                                let current_download_path = SETTINGS.read().unwrap().get_current_settings().general.to_owned().download_path;
                                let _ = open::that(Path::new(current_download_path.as_str()));
                            }
                        }
                    }
                }
            }
            Message::ConfirmResult => {}
            Message::RecvDone => {
                self.share_code = String::new();
                self.progress_bars.clear();
                *RECEIVER_STATE.write().unwrap() = ReceiverState::Idle;
                *RECEIVER_NOTIFICATION.write().unwrap() = ReceiverNotification::Normal;
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let share_code_input = row![text("Share Code"), horizontal_space(), text_input("", &self.share_code).on_input(Message::ShareCodeChanged).padding(5),]
            .spacing(5)
            .padding(5)
            .align_y(iced::Alignment::Center);

        let help_icon = svg(svg::Handle::from_memory(HELP_ICON)).style(styles::svg_styles::colored_svg_theme).height(20).width(20);

        let help_tooltip = tooltip(help_icon, "Sender is in the same local area network", Position::FollowCursor).gap(10).style(container::rounded_box);

        let lan_checkbox =
            row![checkbox("LAN", self.lan).on_toggle(|lan| Message::LanChanged(lan)), help_tooltip,].spacing(5).padding(5).align_y(iced::Alignment::Center);

        let receiver_state_read = RECEIVER_STATE.read().unwrap();

        let errored = match &*RECEIVER_NOTIFICATION.read().unwrap() {
            &ReceiverNotification::Errored(_) => true,
            _ => false,
        };

        let mut recv_button = button(row![
            horizontal_space(),
            text(if receiver_state_read.eq(&ReceiverState::Recving) {
                "Receiving"
            } else if receiver_state_read.eq(&ReceiverState::RecvDone) || errored {
                "Done"
            } else {
                "Recv"
            })
            .size(18),
            horizontal_space()
        ])
        .width(Length::Fill);

        if !self.share_code.is_empty() && !receiver_state_read.eq(&ReceiverState::Recving) {
            recv_button = recv_button.on_press(Message::Receive);
        }
        if receiver_state_read.eq(&ReceiverState::RecvDone) {
            recv_button = recv_button.on_press(Message::RecvDone);
        }

        let receiver_notification = RECEIVER_NOTIFICATION.read().unwrap();
        let notification = match receiver_notification.clone() {
            ReceiverNotification::Normal => row![],
            ReceiverNotification::Message(msg) => {
                row![text(msg).style(styles::text_styles::accent_color_theme)]
            }
            ReceiverNotification::Errored(err) => {
                row![text(err).style(styles::text_styles::red_text_theme)]
            }
            ReceiverNotification::Confirm(confirm_type, confirm_msg) => row![
                text(confirm_msg).style(styles::text_styles::accent_color_theme).width(Length::Fixed(350.0)),
                horizontal_space(),
                button("Yes").on_press(Message::Confirm(confirm_type.clone(), true)),
                button("No").on_press(Message::Confirm(confirm_type.clone(), false)),
            ]
            .spacing(5)
            .align_y(Alignment::Center),
        };

        column![share_code_input, lan_checkbox, self.progress_view(), recv_button, notification,].padding(5).spacing(5).into()
    }

    fn progress_view(&self) -> Element<'_, Message> {
        if !self.progress_bars.is_empty() {
            column!(
                container(
                    scrollable(
                        Column::from_vec(self.progress_bars.iter().map(|progress_bar| { progress_bar.view().map(Message::ProgressBar) }).collect(),)
                            .padding(10)
                            .spacing(5)
                            .width(Length::Fill)
                    )
                    .id(self.scrollable_id.clone())
                    .on_scroll(Message::PageScrolled)
                    .height(300)
                    .direction(styles::scrollable_styles::vertical_direction())
                )
                .style(styles::container_styles::first_class_container_rounded_theme)
                .height(300)
                .width(Length::Fill)
            )
            .width(Length::Fill)
            .spacing(5)
            .into()
        } else {
            column!(
                container(
                    text(if RECEIVER_STATE.read().unwrap().eq(&ReceiverState::Recving) {
                        "Recving..."
                    } else {
                        "Enter share code to receive"
                    })
                    .font(Font {
                        weight: font::Weight::Bold,
                        ..Default::default()
                    })
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .style(styles::container_styles::first_class_container_rounded_theme)
                .height(300)
                .width(Length::Fill)
            )
            .width(Length::Fill)
            .spacing(5)
            .into()
        }
    }
}

impl Tab for ReceiverTab {
    type Message = Message;

    fn title() -> &'static str {
        "Receiver"
    }

    fn icon_bytes() -> &'static [u8] {
        RECEIVER_ICON
    }

    fn get_scrollable_offset(&self) -> scrollable::RelativeOffset {
        self.scrollable_offset
    }
}
