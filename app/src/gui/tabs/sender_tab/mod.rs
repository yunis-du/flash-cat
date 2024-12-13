use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, RwLock,
    },
    time::Duration,
};

use flash_cat_core::sender::FlashCatSender;
use iced::{
    font, mouse,
    widget::{button, column, container, horizontal_space, mouse_area, row, scrollable, svg, text},
    Alignment, Element, Font, Length, Task,
};
use iced::{
    widget::{
        scrollable::{Id, RelativeOffset, Viewport},
        Column,
    },
    Padding,
};
use sender::{send, Error, Progress};

use super::{settings_tab::settings_config::SETTINGS, Tab};
use crate::{
    folder::{pick_files, pick_floders},
    gui::{
        assets::icons::{COPY_ICON, REMOVE_ICON, SENDER_ICON, TICK_ICON},
        progress_bar_widget::{
            Message as ProgressBarMessage, ProgressBar, State as ProgressBarState,
        },
        styles,
    },
};
use flash_cat_common::{
    consts::PUBLIC_RELAY,
    proto::ClientType,
    utils::{
        fs::{collect_files, FileCollector},
        gen_share_code,
    },
};

mod sender;

pub(super) static SENDER_STATE: LazyLock<RwLock<SenderState>> =
    LazyLock::new(|| RwLock::new(SenderState::Idle));

pub(super) static SENDER_NOTIFICATION: LazyLock<RwLock<SenderNotification>> =
    LazyLock::new(|| RwLock::new(SenderNotification::Normal));

static COPYED_BUTTON_STATE: LazyLock<AtomicBool> = LazyLock::new(|| AtomicBool::new(false));

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SenderState {
    Idle,
    Picked,
    AwaitingReceive,
    Reject,
    Sending,
    SendDone,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SenderNotification {
    Normal,
    Message(String),
    Errored(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    PageScrolled(Viewport),
    PickFiles,
    PickFloders,
    AddPickedPath(Result<Option<Vec<PathBuf>>, String>),
    Send,
    Cancel,
    SendDone,
    CopySharCode,
    ResetCopyBut,
    StartSendError(String),
    SendButtonEnter,
    SendButtonExit,
    Remove(String),
    RemoveAll,
    ProgressBar(ProgressBarMessage),
    SendProgressed(Result<(u64, Progress), Error>),
}

pub struct SenderTab {
    scrollable_offset: RelativeOffset,
    scrollable_id: Id,
    share_code: String,
    send_button_text: String,
    paths: Vec<String>,
    fc: Option<FileCollector>,
    fcs: Option<Arc<FlashCatSender>>,
    progress_bars: Vec<ProgressBar>,
    start_send: bool,
}

impl SenderTab {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                scrollable_offset: RelativeOffset::START,
                scrollable_id: Self::scrollable_id(),
                share_code: String::new(),
                send_button_text: String::new(),
                paths: vec![],
                fc: None,
                fcs: None,
                progress_bars: vec![],
                start_send: false,
            },
            Task::none(),
        )
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let mut batch = self
            .progress_bars
            .iter()
            .map(|progress_bar| progress_bar.subscription().map(Message::ProgressBar))
            .collect::<Vec<_>>();

        batch.push(if self.start_send {
            if self.fcs.is_some() {
                send(self.fcs.clone().unwrap()).map(Message::SendProgressed)
            } else {
                iced::Subscription::none()
            }
        } else {
            iced::Subscription::none()
        });
        iced::Subscription::batch(batch)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PageScrolled(view_port) => {
                self.scrollable_offset = view_port.relative_offset();
            }
            Message::PickFiles => {
                return Task::perform(pick_files(), |result| {
                    Message::AddPickedPath(result.map_err(|err| err.to_string()))
                });
            }
            Message::PickFloders => {
                return Task::perform(pick_floders(), |result| {
                    Message::AddPickedPath(result.map_err(|err| err.to_string()))
                });
            }
            Message::AddPickedPath(pick_paths_result) => {
                if let Ok(paths) = pick_paths_result {
                    if let Some(paths) = paths {
                        let mut add_paths = paths
                            .iter()
                            .map(|p| p.to_string_lossy().to_string())
                            .collect::<Vec<_>>();

                        self.paths.retain(|path| !add_paths.contains(&path));
                        self.paths.append(&mut add_paths);

                        let fc = collect_files(self.paths.as_slice());
                        self.progress_bars.clear();
                        fc.files.iter().for_each(|f| {
                            self.progress_bars.push(ProgressBar::new(
                                f.file_id,
                                f.name.to_owned(),
                                f.size,
                                fc.num_files,
                            ))
                        });
                        self.fc.replace(fc);
                        if !self.paths.is_empty() {
                            *SENDER_STATE.write().unwrap() = SenderState::Picked;
                        }
                    }
                }
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
                let specify_relay = if relay_addr.contains(PUBLIC_RELAY) {
                    None
                } else {
                    Some(relay_addr)
                };
                self.share_code = gen_share_code();

                if self.fc.is_some() {
                    let fcs = FlashCatSender::new_with_file_collector(
                        self.share_code.clone(),
                        specify_relay,
                        self.fc.clone().unwrap(),
                        ClientType::App,
                    );
                    match fcs {
                        Ok(fcs) => {
                            let fcs = Arc::new(fcs);
                            self.fcs.replace(fcs.clone());
                            self.progress_bars
                                .iter_mut()
                                .for_each(|progress_bar| progress_bar.start());
                            self.start_send = true;
                        }
                        Err(_) => {
                            // todo handle error
                        }
                    }
                }
            }
            Message::Remove(path) => {
                self.paths.retain(|p| !p.eq(&path));

                let fc = collect_files(self.paths.as_slice());
                self.progress_bars.clear();
                fc.files.iter().for_each(|f| {
                    self.progress_bars.push(ProgressBar::new(
                        f.file_id,
                        f.name.to_owned(),
                        f.size,
                        fc.num_files,
                    ))
                });
                self.fc.replace(fc);
                if self.paths.is_empty() {
                    *SENDER_STATE.write().unwrap() = SenderState::Idle;
                }
            }
            Message::RemoveAll => {
                self.fc = None;
                self.paths.clear();
                self.progress_bars.clear();
                *SENDER_STATE.write().unwrap() = SenderState::Idle;
            }
            Message::Cancel => {
                *SENDER_STATE.write().unwrap() = SenderState::Picked;
                self.send_button_text = "".to_string();
                if let Some(fcs) = self.fcs.clone() {
                    fcs.shutdown();
                }
                self.fcs = None;
                self.start_send = false;
            }
            Message::SendDone => {
                *SENDER_STATE.write().unwrap() = SenderState::Idle;
                *SENDER_NOTIFICATION.write().unwrap() = SenderNotification::Normal;
                self.fc = None;
                self.fcs = None;
                self.paths.clear();
                self.progress_bars.clear();
                self.start_send = false;
            }
            Message::CopySharCode => {
                COPYED_BUTTON_STATE.store(true, Ordering::Relaxed);
                return Task::batch([
                    iced::clipboard::write(self.share_code.clone()),
                    Task::perform(
                        async move { tokio::time::sleep(Duration::from_secs(3)).await },
                        |_| Message::ResetCopyBut,
                    ),
                ]);
            }
            Message::ResetCopyBut => {
                COPYED_BUTTON_STATE.store(false, Ordering::Relaxed);
            }
            Message::StartSendError(_err) => {}
            Message::SendButtonEnter => {
                let sender_state = SENDER_STATE.read().unwrap();
                if sender_state.eq(&SenderState::Sending)
                    || sender_state.eq(&SenderState::AwaitingReceive)
                {
                    self.send_button_text = "Cancel".to_string();
                }
            }
            Message::SendButtonExit => {
                self.send_button_text = "".to_string();
            }
            Message::ProgressBar(_) => {}
            Message::SendProgressed(progress) => match progress {
                Ok((file_id, progress)) => {
                    let new_state = match progress {
                        sender::Progress::Sent(sent) => {
                            if SENDER_STATE.read().unwrap().ne(&SenderState::Sending)
                                && SENDER_STATE.read().unwrap().ne(&SenderState::SendDone)
                            {
                                *SENDER_STATE.write().unwrap() = SenderState::Sending;
                            }
                            Some(ProgressBarState::Progress(sent))
                        }
                        sender::Progress::Finished => Some(ProgressBarState::Finished),
                        sender::Progress::Skip => Some(ProgressBarState::Skip),
                        sender::Progress::Done => {
                            *SENDER_STATE.write().unwrap() = SenderState::SendDone;
                            None
                        }
                        _ => None,
                    };
                    let progress_bar = self
                        .progress_bars
                        .iter_mut()
                        .find(|progress_bar| progress_bar.get_id().eq(&file_id));
                    if let Some(progress_bar) = progress_bar {
                        progress_bar.update_state(new_state);
                    }
                }
                Err(e) => match e {
                    Error::Reject => *SENDER_STATE.write().unwrap() = SenderState::Reject,
                    Error::OtherClose => {
                        *SENDER_NOTIFICATION.write().unwrap() = SenderNotification::Message(
                            "The receive end is interrupted.".to_string(),
                        );
                    }
                    Error::Errored(e) => {
                        *SENDER_NOTIFICATION.write().unwrap() = SenderNotification::Errored(e)
                    }
                    Error::RelayFailed(err_msg) => {
                        *SENDER_NOTIFICATION.write().unwrap() =
                            SenderNotification::Errored(err_msg);
                    }
                },
            },
        }
        Task::none()
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
            text("Pick").size(16),
            horizontal_space(),
            row![pick_files_button, pick_floders_button]
                .align_y(Alignment::Center)
                .spacing(5),
        ]
        .align_y(Alignment::Center)]
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
        } else if sender_state.eq(&SenderState::SendDone) {
            send_button = send_button.on_press(Message::SendDone);
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
                    .style(styles::container_styles::first_class_container_square_theme)
                    .padding(5)
                    .align_y(iced::alignment::Vertical::Center)
                )
                .interaction(mouse::Interaction::Pointer)
                .on_press(Message::CopySharCode),
                if COPYED_BUTTON_STATE.load(Ordering::Relaxed) {
                    let copyed_icon_handle = svg::Handle::from_memory(TICK_ICON);
                    let copyed_icon = svg(copyed_icon_handle)
                        .style(styles::svg_styles::colored_svg_theme)
                        .height(20)
                        .width(20);
                    row![
                        button(copyed_icon)
                            .style(styles::button_styles::transparent_button_theme)
                            .on_press(Message::CopySharCode),
                        text("Copyed").style(styles::text_styles::accent_color_theme)
                    ]
                    .align_y(Alignment::Center)
                } else {
                    let copy_icon_handle = svg::Handle::from_memory(COPY_ICON);
                    let copy_icon = svg(copy_icon_handle)
                        .style(styles::svg_styles::colored_svg_theme)
                        .height(20)
                        .width(20);
                    row![button(copy_icon)
                        .style(styles::button_styles::transparent_button_theme)
                        .on_press(Message::CopySharCode)]
                }
            ]
            .spacing(5)
            .align_y(Alignment::Center)
        } else {
            row![]
        };

        let sender_notification = SENDER_NOTIFICATION.read().unwrap();
        let notification = match sender_notification.clone() {
            SenderNotification::Normal => row![],
            SenderNotification::Message(msg) => {
                row![text(msg).style(styles::text_styles::accent_color_theme)]
            }
            SenderNotification::Errored(err) => {
                row![text(err).style(styles::text_styles::red_text_theme)]
            }
        };

        column![
            pick,
            self.progress_view(),
            send_button,
            share_code,
            notification,
        ]
        .padding(5)
        .spacing(5)
        .into()
    }

    fn progress_view(&self) -> Element<'_, Message> {
        if self.paths.is_empty() {
            let text = container(text("Pick File(s) or Folder(s) to send").font(Font {
                weight: font::Weight::Bold,
                ..Default::default()
            }))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(styles::container_styles::first_class_container_rounded_theme)
            .height(300)
            .width(Length::Fill);
            column!(text).width(Length::Fill).spacing(5).into()
        } else {
            let sender_state = SENDER_STATE.read().unwrap();
            let picked_body_view: Element<'_, Message> = container(
                scrollable(if sender_state.eq(&SenderState::Picked) {
                    Column::with_children(
                        self.paths
                            .iter()
                            .map(|path| {
                                row![
                                    text(path.clone())
                                        .style(styles::text_styles::accent_color_theme),
                                    horizontal_space(),
                                    remove_button(path.to_owned()),
                                ]
                                .align_y(iced::Alignment::Center)
                                .into()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .padding(Padding {
                        top: 10.0,
                        right: 0.0,
                        bottom: 10.0,
                        left: 10.0,
                    })
                    .spacing(5)
                    .width(Length::Fill)
                } else {
                    Column::from_vec(
                        self.progress_bars
                            .iter()
                            .map(|progress_bar| progress_bar.view().map(Message::ProgressBar))
                            .collect(),
                    )
                    .padding(Padding {
                        top: 10.0,
                        right: 0.0,
                        bottom: 10.0,
                        left: 10.0,
                    })
                    .spacing(5)
                    .width(Length::Fill)
                })
                .id(self.scrollable_id.clone())
                .on_scroll(Message::PageScrolled)
                .height(300)
                .direction(styles::scrollable_styles::vertical_direction()),
            )
            .style(styles::container_styles::first_class_container_rounded_theme)
            .width(Length::Fill)
            .into();

            let details_text = if self.fc.is_some() {
                let fc = self.fc.clone().unwrap();
                if sender_state.eq(&SenderState::SendDone) {
                    format!(
                        "Sent {} files and {} folders ({})",
                        fc.num_files,
                        fc.num_folders,
                        fc.total_size_to_human_readable()
                    )
                } else if sender_state.eq(&SenderState::Reject) {
                    format!(
                        "Rejected {} files and {} folders ({})",
                        fc.num_files,
                        fc.num_folders,
                        fc.total_size_to_human_readable()
                    )
                } else if fc.num_folders > 0 {
                    format!(
                        "{} {} files and {} folders ({})",
                        if self.start_send { "Sending" } else { "" },
                        fc.num_files,
                        fc.num_folders,
                        fc.total_size_to_human_readable()
                    )
                } else {
                    format!(
                        "{} {} files ({})",
                        if self.start_send { "Sending" } else { "" },
                        fc.num_files,
                        fc.total_size_to_human_readable()
                    )
                }
            } else {
                "".to_string()
            };

            column![
                picked_body_view,
                text(details_text).font(Font {
                    weight: font::Weight::Bold,
                    ..Default::default()
                }),
            ]
            .spacing(5)
            .into()
        }
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
        self.scrollable_offset
    }
}

fn remove_button(id: String) -> Element<'static, Message> {
    let remove_icon_handle = svg::Handle::from_memory(REMOVE_ICON);
    let remove_icon = svg(remove_icon_handle)
        .style(styles::svg_styles::colored_svg_theme)
        .width(Length::Shrink);

    button(remove_icon)
        .style(styles::button_styles::transparent_button_theme)
        .on_press(Message::Remove(id))
        .into()
}
