use std::sync::Arc;

use crate::gui::{assets::icons::REMOVE_ICON, styles};
use flash_cat_common::utils::fs::{collect_files, FileCollector};
use flash_cat_common::utils::{human_bytes, human_duration};
use flash_cat_core::sender::FlashCatSender;
use iced::widget::scrollable::{Id, RelativeOffset, Viewport};
use iced::{
    font::Weight,
    widget::{
        button, column, container, horizontal_space, progress_bar, row, scrollable, svg, text,
        Column,
    },
    Command, Element, Font, Length,
};

use super::{
    sender::{self, send},
    SenderState, SENDER_STATE,
};

#[derive(Clone, Debug)]
pub enum Message {
    PageScrolled(Viewport),
    Add(Vec<String>),
    Remove(String),
    RemoveAll,
    CancelSend,
    StartSend(String, Option<String>),
    SendProgressed((u64, sender::Progress)),
}

pub struct PickedBody {
    scrollable_offset: RelativeOffset,
    scrollable_id: Id,
    paths: Vec<String>,
    fc: Option<FileCollector>,
    fcs: Option<Arc<FlashCatSender>>,
    progress_bars: Vec<(String, SenderProgressBar)>,
    start_send: bool,
}

impl PickedBody {
    pub fn new(scrollable_id: Id) -> Self {
        Self {
            scrollable_offset: RelativeOffset::START,
            scrollable_id,
            paths: vec![],
            fc: None,
            fcs: None,
            progress_bars: vec![],
            start_send: false,
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        let mut batch = self
            .progress_bars
            .iter()
            .map(|(_, progress_bar)| progress_bar.subscription())
            .collect::<Vec<_>>();

        batch.push(if self.start_send {
            if self.fcs.is_some() {
                let fcs = self.fcs.clone().unwrap();
                send(fcs.clone()).map(Message::SendProgressed)
            } else {
                iced::Subscription::none()
            }
        } else {
            iced::Subscription::none()
        });
        iced::Subscription::batch(batch)
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::PageScrolled(view_port) => {
                self.scrollable_offset = view_port.relative_offset()
            }
            Message::Add(mut add_paths) => {
                self.paths.retain(|path| !add_paths.contains(&path));
                self.paths.append(&mut add_paths);

                let fc = collect_files(self.paths.as_slice());
                fc.files.iter().for_each(|f| {
                    self.progress_bars
                        .push((f.name.to_owned(), SenderProgressBar::new(f.file_id, f.size)))
                });
                self.fc.replace(fc);
            }
            Message::Remove(path) => {
                self.paths.retain(|p| !p.eq(&path));

                let fc = collect_files(self.paths.as_slice());
                self.progress_bars.clear();
                fc.files.iter().for_each(|f| {
                    self.progress_bars
                        .push((f.name.to_owned(), SenderProgressBar::new(f.file_id, f.size)))
                });
                self.fc.replace(fc);
            }
            Message::RemoveAll => {
                self.fc = None;
                self.paths.clear();
                self.progress_bars.clear();
            }
            Message::CancelSend => {
                if let Some(fcs) = self.fcs.clone() {
                    fcs.shutdown();
                }
                self.fcs = None;
                self.start_send = false;
            }
            Message::StartSend(share_code, specify_relay) => {
                if self.fc.is_some() {
                    let fcs = FlashCatSender::new_with_file_collector(
                        share_code,
                        specify_relay,
                        self.fc.clone().unwrap(),
                    );
                    match fcs {
                        Ok(fcs) => {
                            let fcs = Arc::new(fcs);
                            self.fcs.replace(fcs.clone());
                            self.progress_bars.iter_mut().for_each(|(_, p)| p.start());
                            self.start_send = true;
                        }
                        Err(_) => {
                            // todo handle error
                        }
                    }
                }
            }
            Message::SendProgressed((file_id, progress)) => {
                let progress_bar = self
                    .progress_bars
                    .iter_mut()
                    .find(|(_, p)| p.get_id().eq(&file_id));
                if let Some(progress_bar) = progress_bar {
                    progress_bar.1.progress(progress);
                }
            }
        }
        Command::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        if self.paths.is_empty() {
            let text = container(text("No File(s) or Folder(s) Picked"))
                .center_x()
                .center_y()
                .height(100)
                .width(Length::Fill);
            column!(text).width(Length::Fill).padding(10).into()
        } else {
            let picked_body_view: Element<'_, Message> = container(
                scrollable(if SENDER_STATE.read().unwrap().eq(&SenderState::Picked) {
                    Column::with_children(
                        self.paths
                            .iter()
                            .map(|path| {
                                row![
                                    text(&path).style(styles::text_styles::accent_color_theme()),
                                    horizontal_space(),
                                    remove_button(path.to_owned()),
                                ]
                                .align_items(iced::Alignment::Center)
                                .into()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .padding(10)
                    .spacing(5)
                    .width(Length::Fill)
                } else {
                    Column::from_vec(
                        self.progress_bars
                            .iter()
                            .map(|pb| {
                                column![
                                    text(&pb.0).style(styles::text_styles::accent_color_theme()),
                                    pb.1.view(),
                                ]
                                .spacing(3)
                                .into()
                            })
                            .collect(),
                    )
                    .padding(10)
                    .spacing(5)
                    .width(Length::Fill)
                })
                .id(self.scrollable_id.clone())
                .on_scroll(Message::PageScrolled)
                .height(300)
                .direction(styles::scrollable_styles::vertical_direction()),
            )
            .style(styles::container_styles::first_class_container_rounded_theme())
            .width(Length::Fill)
            .into();

            let details_text = if self.fc.is_some() {
                let fc = self.fc.clone().unwrap();
                if fc.num_folders > 0 {
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
                    weight: Weight::Bold,
                    ..Default::default()
                }),
            ]
            .spacing(5)
            .into()
        }
    }

    pub fn get_scrollable_offset(&self) -> scrollable::RelativeOffset {
        self.scrollable_offset
    }
}

fn remove_button(id: String) -> Element<'static, Message> {
    let remove_icon_handle = svg::Handle::from_memory(REMOVE_ICON);
    let remove_icon = svg(remove_icon_handle)
        .style(styles::svg_styles::colored_svg_theme())
        .width(Length::Shrink);

    button(remove_icon)
        .style(styles::button_styles::transparent_button_theme())
        .on_press(Message::Remove(id))
        .into()
}

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Sending(f32),
    Skip,
    Finished,
}

#[derive(Debug, Clone)]
pub struct SenderProgressBar {
    id: u64,
    total_progress: u64,
    pb: indicatif::ProgressBar,
    state: State,
}

impl SenderProgressBar {
    pub fn new(id: u64, total_progress: u64) -> Self {
        let pb = indicatif::ProgressBar::new(total_progress);
        Self {
            id,
            total_progress,
            pb,
            state: State::Idle,
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn start(&mut self) {
        match &self.state {
            State::Idle { .. } => {
                self.state = State::Sending(0.0);
            }
            _ => {}
        }
    }

    pub fn progress(&mut self, new_progress: sender::Progress) {
        if let State::Sending(progress) = &mut self.state {
            match new_progress {
                sender::Progress::Started => {
                    *progress = 0.0;
                }
                sender::Progress::Sent(sent) => {
                    *progress = sent;
                    if !SENDER_STATE.read().unwrap().eq(&SenderState::Sending) {
                        *SENDER_STATE.write().unwrap() = SenderState::Sending;
                    }
                }
                sender::Progress::Finished => {
                    self.state = State::Finished;
                }
                sender::Progress::Skip => {
                    self.state = State::Skip;
                }
                _ => {}
            }
        }
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::none()
    }

    pub fn view(&self) -> Element<Message> {
        let current_progress = match &self.state {
            State::Idle => 0.0,
            State::Sending(progress) => *progress,
            State::Skip => 0.0,
            State::Finished => self.total_progress as f32,
        };

        if self.state.eq(&State::Skip) {
            text("Skip").into()
        } else {
            self.pb.set_position(current_progress as u64);
            row![
                progress_bar(0.0..=self.total_progress as f32, current_progress)
                    .height(12)
                    .width(200),
                horizontal_space(),
                text(format!(
                    "{}/{} ({}/s, {})",
                    human_bytes(current_progress as u64),
                    human_bytes(self.total_progress as u64),
                    human_bytes(self.pb.per_sec() as u64),
                    if current_progress == 0.0 {
                        human_duration(std::time::Duration::ZERO)
                    } else {
                        human_duration(self.pb.eta())
                    },
                ))
                .size(12)
            ]
            .spacing(5)
            .align_items(iced::Alignment::Center)
            .into()
        }
    }
}
