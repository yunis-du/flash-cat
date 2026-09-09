use flash_cat_core::{FileStatus, TransferResults};
use futures::SinkExt;
use std::collections::HashMap;
use std::{sync::Arc, vec};

use flash_cat_common::{
    consts::PUBLIC_RELAY,
    proto::ClientType,
    utils::{
        fs::{FileCollector, collect_files_with_progress},
        gen_share_code,
    },
};
use flash_cat_core::{SenderInteractionMessage, sender::FlashCatSender};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Sizable,
    button::{Button, ButtonVariants},
    clipboard::Clipboard,
    h_flex,
    label::Label,
    spinner::Spinner,
    v_flex,
};
use gpui_kit::{Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder};
use rust_i18n::t;
use tokio_stream::StreamExt;

use crate::{
    assets::CustomIconName,
    components::{Card, ProgressBar},
    helpers::{i18n_common, i18n_send, pick_files, pick_folders, spawn_transfer},
    state::FlashCatAppGlobalStore,
};

#[derive(PartialEq, Eq)]
enum SendState {
    Idle,
    FileSelected,
    Collecting,
    AwaitingReceive,
    Sending,
    SendDone,
}

#[derive(PartialEq, Eq, Clone)]
enum NotificationType {
    None,
    Message(String),
    Error(String),
}

pub struct SendView {
    send_state: SendState,
    selected_files: Vec<String>,
    file_collector: Option<Arc<FileCollector>>,
    scan_cancel: Option<flash_cat_common::Shutdown>,
    send_but_hover: bool,
    share_code: String,
    generation: u64,
    results: TransferResults,
    total_entries: u64,
    flash_cat_sender: Option<Arc<FlashCatSender>>,
    progress_bars: Vec<ProgressBar>,
    progress_index: HashMap<u64, usize>,
    page: usize,
    notification: NotificationType,
}

impl SendView {
    fn progress_mut(
        &mut self,
        file_id: u64,
    ) -> Option<&mut ProgressBar> {
        let index = *self.progress_index.get(&file_id)?;
        self.progress_bars.get_mut(index)
    }

    pub fn new(
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            send_state: SendState::Idle,
            selected_files: vec![],
            file_collector: None,
            scan_cancel: None,
            send_but_hover: false,
            share_code: String::new(),
            generation: 0,
            results: TransferResults::default(),
            total_entries: 0,
            flash_cat_sender: None,
            progress_bars: vec![],
            progress_index: HashMap::new(),
            page: 0,
            notification: NotificationType::None,
        }
    }
}

impl Render for SendView {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let file_selector = {
            let disabled = self.send_state != SendState::Idle && self.send_state != SendState::FileSelected;
            h_flex().child(Label::new(i18n_send(cx, "select"))).child(
                div()
                    .flex()
                    .gap_1()
                    .size_full()
                    .justify_end()
                    .child(
                        Button::new("file-selector")
                            .info()
                            .label(i18n_send(cx, "file"))
                            .icon(IconName::File)
                            .small()
                            .cursor_pointer()
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.spawn(async move |this, cx| {
                                    if let Ok(picked_path) = pick_files().await {
                                        if let Some(picked_path) = picked_path {
                                            let _ = cx.update(|cx| {
                                                let _ = this.update(cx, |view, _| {
                                                    if !matches!(view.send_state, SendState::Idle | SendState::FileSelected) {
                                                        return;
                                                    }
                                                    let mut selected = view.selected_files.iter().cloned().collect::<std::collections::HashSet<_>>();
                                                    for path in picked_path {
                                                        let path_str = path.to_string_lossy().to_string();
                                                        if selected.insert(path_str.clone()) {
                                                            view.selected_files.push(path_str);
                                                        }
                                                    }

                                                    view.send_state = SendState::FileSelected;
                                                });
                                            });
                                        }
                                    }
                                })
                                .detach();
                            }))
                            .disabled(disabled)
                            .when(!disabled, |this| this.cursor_pointer()),
                    )
                    .child(
                        Button::new("folder-selector")
                            .info()
                            .label(i18n_send(cx, "folder"))
                            .icon(IconName::Folder)
                            .small()
                            .cursor_pointer()
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.spawn(async move |this, cx| {
                                    if let Ok(picked_path) = pick_folders().await {
                                        if let Some(picked_path) = picked_path {
                                            let _ = cx.update(|cx| {
                                                let _ = this.update(cx, |view, _| {
                                                    if !matches!(view.send_state, SendState::Idle | SendState::FileSelected) {
                                                        return;
                                                    }
                                                    let mut selected = view.selected_files.iter().cloned().collect::<std::collections::HashSet<_>>();
                                                    for path in picked_path {
                                                        let path_str = path.to_string_lossy().to_string();
                                                        if selected.insert(path_str.clone()) {
                                                            view.selected_files.push(path_str);
                                                        }
                                                    }

                                                    view.send_state = SendState::FileSelected;
                                                });
                                            });
                                        }
                                    }
                                })
                                .detach();
                            }))
                            .disabled(disabled)
                            .when(!disabled, |this| this.cursor_pointer()),
                    ),
            )
        };

        let show_progress = !self.progress_bars.is_empty()
            && (matches!(
                self.send_state,
                SendState::AwaitingReceive | SendState::Sending | SendState::SendDone
            ) || matches!(self.notification, NotificationType::Error(_)));
        let list_len = if show_progress {
            self.progress_bars.len()
        } else {
            self.selected_files.len()
        };
        let pages = list_len.div_ceil(64).max(1);
        let page = self.page.min(pages - 1);

        let send_card = {
            let placeholder = div().flex().size_full().justify_center().items_center().child(Label::new(i18n_send(cx, "placeholder")));

            let mut items = vec![];
            if show_progress {
                for progress_bar in self.progress_bars.iter().skip(page * 64).take(64) {
                    items.push(div().p_2().mb_1().bg(cx.theme().list_hover).rounded_md().child(progress_bar.clone().into_element()));
                }
            } else {
                for (i, file) in self.selected_files.iter().enumerate().skip(page * 64).take(64) {
                    items.push(
                        div().p_2().mb_1().bg(cx.theme().list_hover).rounded_md().child(
                            h_flex().justify_between().child(Label::new(file.clone()).text_sm().truncate().text_color(cx.theme().primary)).child(
                                Button::new(("remove_path", i))
                                    .disabled(self.send_state == SendState::Collecting)
                                    .cursor_pointer()
                                    .icon(CustomIconName::Remove)
                                    .small()
                                    .ghost()
                                    .on_click(cx.listener(move |view, _, _, _| {
                                        view.selected_files.remove(i);
                                        if view.selected_files.is_empty() {
                                            view.send_state = SendState::Idle;
                                        }
                                    })),
                            ),
                        ),
                    );
                }
            }

            Card::new("send-view-card").overflow_y_scrollbar().h_72().when(self.selected_files.is_empty(), |this| this.child(placeholder)).when(
                !self.selected_files.is_empty(),
                |mut this| {
                    for file in items {
                        this = this.child(file);
                    }
                    this
                },
            )
        };

        let file_counter_with_cleanup = {
            let h_flex = h_flex().h_6();
            let clean_button = div().flex().gap_1().size_full().justify_end().child(
                Button::new("clean_all_files").info().label(i18n_send(cx, "clean_all_files")).small().cursor_pointer().on_click(cx.listener(
                    |view, _, _, _| {
                        view.selected_files.clear();
                        view.total_entries = 0;
                        view.results = TransferResults::default();
                        view.notification = NotificationType::None;
                        view.progress_bars.clear();
                        view.progress_index.clear();
                        view.page = 0;
                        view.send_state = SendState::Idle;
                    },
                )),
            );

            match self.send_state {
                SendState::Idle | SendState::Collecting => h_flex,
                SendState::FileSelected => h_flex.child(clean_button),
                SendState::AwaitingReceive => {
                    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
                    h_flex.child(Label::new(t!(
                        "send.file_counter",
                        file_count = if let Some(collector) = &self.file_collector {
                            collector.file_count()
                        } else {
                            0
                        },
                        folder_count = if let Some(collector) = &self.file_collector {
                            collector.folder_count()
                        } else {
                            0
                        },
                        locale = locale
                    )))
                }
                SendState::Sending => h_flex,
                SendState::SendDone => h_flex,
            }
        };

        let send_button = {
            let label = match self.send_state {
                SendState::Idle | SendState::FileSelected => Some(if matches!(self.notification, NotificationType::Error(_)) {
                    i18n_common(cx, "retry")
                } else {
                    i18n_send(cx, "send")
                }),
                SendState::Collecting => Some(i18n_common(cx, "cancel_scan")),
                SendState::AwaitingReceive | SendState::Sending => {
                    if self.send_but_hover {
                        Some(i18n_send(cx, "cancel_send"))
                    } else if self.send_state == SendState::AwaitingReceive {
                        Some(i18n_send(cx, "awaiting_receive"))
                    } else {
                        Some(i18n_send(cx, "sending"))
                    }
                }
                SendState::SendDone => Some(i18n_send(cx, "send_done")),
            };

            let spinner = Spinner::new().color(cx.theme().background);

            let disabled = self.selected_files.is_empty();

            let mut button = Button::new("send_button").size_full().h_10().info().disabled(disabled).when(!disabled, |this| this.cursor_pointer());

            if self.send_state == SendState::Collecting {
                button = button.child(div().flex().justify_center().child(spinner));
            }

            if self.send_state == SendState::AwaitingReceive || self.send_state == SendState::Sending {
                button = button.on_hover(cx.listener(|view, hover, _, _| {
                    view.send_but_hover = *hover;
                }));
            }

            if let Some(label) = label {
                button = button.label(label);
            }

            button.on_click(cx.listener(move |view, _, _, cx| match view.send_state {
                SendState::Idle => (),
                SendState::Collecting => {
                    view.generation += 1;
                    if let Some(cancel) = view.scan_cancel.take() {
                        cancel.shutdown();
                    }
                    view.send_state = SendState::FileSelected;
                    view.notification = NotificationType::Message(i18n_common(cx, "scan_cancelled").to_string());
                    cx.notify();
                }
                SendState::FileSelected => {
                    view.generation += 1;
                    let generation = view.generation;
                    view.results = TransferResults::default();
                    if let Some(sender) = view.flash_cat_sender.take() {
                        sender.shutdown();
                    }
                    view.notification = NotificationType::None;
                    view.send_state = SendState::Collecting;
                    let cancel = flash_cat_common::Shutdown::new();
                    view.scan_cancel = Some(cancel.clone());
                    let files = view.selected_files.clone();
                    let relay_addr = cx.global::<FlashCatAppGlobalStore>().read(cx).relay_address();

                    cx.spawn(async move |view, cx| {
                        let (scan_tx, mut scan_rx) = futures::channel::mpsc::unbounded();
                        let scan = cx.background_executor().spawn(async move {
                            collect_files_with_progress(&files, &cancel, move |progress| {
                                let _ = scan_tx.unbounded_send(progress);
                            })
                        });
                        while let Some(progress) = scan_rx.next().await {
                            let active = view
                                .update(cx, |view, cx| {
                                    if view.generation != generation {
                                        return false;
                                    }
                                    view.notification = NotificationType::Message(format!(
                                        "{}: {} {} · {} {} · {}",
                                        i18n_common(cx, "scanning"),
                                        progress.files,
                                        i18n_common(cx, "files"),
                                        progress.folders,
                                        i18n_common(cx, "folders"),
                                        flash_cat_common::utils::human_bytes(progress.bytes)
                                    ));
                                    cx.notify();
                                    true
                                })
                                .unwrap_or(false);
                            if !active {
                                return;
                            }
                        }
                        let file_collector = match scan.await {
                            Ok(collector) => Arc::new(collector),
                            Err(error) => {
                                let _ = view.update(cx, |view, cx| {
                                    if view.generation == generation {
                                        view.scan_cancel = None;
                                        view.notification = NotificationType::Error(format!("{error:#}"));
                                        view.send_state = SendState::FileSelected;
                                        cx.notify();
                                    }
                                });
                                return;
                            }
                        };
                        let row_files = file_collector.clone();
                        let (rows, indices) = cx
                            .background_executor()
                            .spawn(async move {
                                let rows = row_files.files.iter().map(|f| ProgressBar::new(f.file_id, f.name.clone(), f.size)).collect::<Vec<_>>();
                                let indices = row_files.files.iter().enumerate().map(|(index, f)| (f.file_id, index)).collect::<HashMap<_, _>>();
                                (rows, indices)
                            })
                            .await;
                        view.update(cx, |view, cx| {
                            if view.generation != generation {
                                return;
                            }
                            view.scan_cancel = None;
                            view.notification = NotificationType::None;
                            view.total_entries = file_collector.files.len() as u64;
                            view.file_collector = Some(file_collector.clone());

                            let specify_relay = if relay_addr.contains(PUBLIC_RELAY) {
                                None
                            } else {
                                Some(relay_addr)
                            };
                            let share_code = gen_share_code();
                            view.share_code = share_code.clone();
                            view.progress_bars = rows;
                            view.progress_index = indices;
                            view.page = 0;

                            let fcs = FlashCatSender::new_with_file_collector(share_code.clone(), specify_relay, file_collector.clone(), ClientType::App, true);
                            match fcs {
                                Ok(fcs) => {
                                    let fcs = Arc::new(fcs);
                                    view.flash_cat_sender.replace(fcs.clone());
                                }
                                Err(error) => {
                                    view.notification = NotificationType::Error(error.to_string());
                                    view.send_state = SendState::FileSelected;
                                    cx.notify();
                                    return;
                                }
                            }

                            view.send_state = SendState::AwaitingReceive;
                        })
                        .ok();

                        // Start file sending by listening to the sender stream
                        // Use a channel to receive progress updates from the tokio runtime
                        let fcs = view
                            .update(cx, |view, _| {
                                (view.generation == generation).then(|| view.flash_cat_sender.clone()).flatten()
                            })
                            .ok()
                            .flatten();
                        if let Some(fcs) = fcs {
                            // Create a channel to receive messages from tokio runtime
                            let (mut tx, mut rx) = futures::channel::mpsc::channel::<SenderInteractionMessage>(128);

                            let mut errors = tx.clone();
                            let result = spawn_transfer(async move {
                                match fcs.clone().start().await {
                                    Ok(mut stream) => {
                                        while let Some(msg) = stream.next().await {
                                            let terminal = matches!(
                                                &msg,
                                                SenderInteractionMessage::Completed
                                                    | SenderInteractionMessage::Error(_)
                                                    | SenderInteractionMessage::ReceiverReject
                                                    | SenderInteractionMessage::OtherClose
                                                    | SenderInteractionMessage::ReconnectFailed(_)
                                            );
                                            if tx.send(msg).await.is_err() || terminal {
                                                break;
                                            }
                                        }
                                    }
                                    Err(error) => {
                                        let _ = tx.send(SenderInteractionMessage::Error(error.to_string())).await;
                                    }
                                }
                                fcs.shutdown();
                                fcs.shutdown_complete().await;
                            });
                            if let Err(error) = result {
                                let _ = errors.send(SenderInteractionMessage::Error(error.to_string())).await;
                            }
                            drop(errors);

                            // Listen for messages from the tokio runtime
                            while let Some(msg) = rx.next().await {
                                let should_break = view
                                    .update(cx, |view, cx| {
                                        if view.generation != generation {
                                            return true;
                                        }
                                        cx.notify();
                                        match msg {
                                            SenderInteractionMessage::TransferMode(mode) => {
                                                view.notification =
                                                    NotificationType::Message(format!("{}: {}", i18n_common(cx, "connection"), mode.to_string()));
                                            }
                                            SenderInteractionMessage::Message(msg) => {
                                                view.notification = NotificationType::Message(msg);
                                            }
                                            SenderInteractionMessage::RelayConnected(_) => {}
                                            SenderInteractionMessage::Error(e) => {
                                                if let Some(sender) = view.flash_cat_sender.take() {
                                                    sender.shutdown();
                                                }
                                                view.notification = NotificationType::Error(e);
                                                view.send_state = SendState::FileSelected;
                                                return true;
                                            }
                                            SenderInteractionMessage::ReceiverReject => {
                                                // Handle rejection
                                                view.notification = NotificationType::Error(i18n_common(cx, "receiver_rejected").to_string());
                                                view.send_state = SendState::FileSelected;
                                                return true;
                                            }
                                            SenderInteractionMessage::RelayFailed((_relay_type, error)) => {
                                                if let Some(sender) = view.flash_cat_sender.take() {
                                                    sender.shutdown();
                                                }
                                                view.notification = NotificationType::Error(error);
                                            }
                                            SenderInteractionMessage::FileStage(stage) => {
                                                if let Some(pb) = view.progress_mut(stage.file_id) {
                                                    pb.set_stage(stage.phase, stage.position);
                                                }
                                            }
                                            SenderInteractionMessage::FileProgress(progress) => {
                                                if view.send_state != SendState::Sending {
                                                    view.send_state = SendState::Sending;
                                                }
                                                if let Some(pb) = view.progress_mut(progress.file_id) {
                                                    pb.set_progress(progress.position);
                                                }
                                            }
                                            SenderInteractionMessage::FileResult(result) => {
                                                if view.results.record(result.clone()) {
                                                    if let Some(pb) = view.progress_mut(result.file_id) {
                                                        match result.status() {
                                                            FileStatus::Success => pb.finish(),
                                                            FileStatus::Skipped => pb.skip(),
                                                            FileStatus::Failed => pb.fail(result.error.clone()),
                                                        }
                                                    }
                                                }
                                            }
                                            SenderInteractionMessage::OtherClose => {
                                                if let Some(sender) = view.flash_cat_sender.take() {
                                                    sender.shutdown();
                                                }
                                                view.notification = NotificationType::Error(i18n_common(cx, "receiver_disconnected").to_string());
                                                view.send_state = if view.selected_files.is_empty() {
                                                    SendState::Idle
                                                } else {
                                                    SendState::FileSelected
                                                };
                                                return true;
                                            }
                                            SenderInteractionMessage::ReconnectFailed(error) => {
                                                if let Some(sender) = view.flash_cat_sender.take() {
                                                    sender.shutdown();
                                                }
                                                view.notification = NotificationType::Error(error);
                                                view.send_state = if view.selected_files.is_empty() {
                                                    SendState::Idle
                                                } else {
                                                    SendState::FileSelected
                                                };
                                                return true;
                                            }
                                            SenderInteractionMessage::SendDone => {
                                                // Sending complete, waiting for confirmation
                                            }
                                            SenderInteractionMessage::Completed => {
                                                view.flash_cat_sender = None;
                                                view.send_state = SendState::SendDone;
                                            }
                                        }
                                        cx.notify(); // Trigger UI refresh
                                        false
                                    })
                                    .ok()
                                    .unwrap_or(true);

                                if should_break {
                                    break;
                                }
                            }
                            let _ = view.update(cx, |view, cx| {
                                if view.generation == generation && matches!(view.send_state, SendState::AwaitingReceive | SendState::Sending) {
                                    if let Some(sender) = view.flash_cat_sender.take() {
                                        sender.shutdown();
                                    }
                                    view.notification = NotificationType::Error(i18n_common(cx, "transfer_stopped").to_string());
                                    view.send_state = SendState::FileSelected;
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .detach();
                }
                SendState::AwaitingReceive => {
                    view.generation += 1;
                    // Cancel and shutdown the sender
                    if let Some(fcs) = view.flash_cat_sender.take() {
                        fcs.shutdown();
                    }
                    view.send_state = if view.selected_files.is_empty() {
                        SendState::Idle
                    } else {
                        SendState::FileSelected
                    };
                }
                SendState::Sending => {
                    view.generation += 1;
                    // Cancel and shutdown the sender during sending
                    if let Some(fcs) = view.flash_cat_sender.take() {
                        fcs.shutdown();
                    }
                    view.send_state = if view.selected_files.is_empty() {
                        SendState::Idle
                    } else {
                        SendState::FileSelected
                    };
                }
                SendState::SendDone => {
                    // Reset the view after completion
                    view.selected_files.clear();
                    view.progress_bars.clear();
                    view.progress_index.clear();
                    view.page = 0;
                    view.file_collector = None;
                    view.flash_cat_sender = None;
                    view.share_code.clear();
                    view.notification = NotificationType::None;
                    view.send_state = SendState::Idle;
                }
            }))
        };

        let share_code = if self.send_state == SendState::AwaitingReceive {
            let code = self.share_code.clone();
            h_flex()
                .gap_2()
                .child(Label::new(format!("{}: {}", i18n_common(cx, "share_code"), self.share_code.clone())).text_sm().text_color(cx.theme().primary))
                .child(Clipboard::new("copy-share-code").value(code))
        } else {
            h_flex()
        };

        let notification_view = match &self.notification {
            NotificationType::None => div(),
            NotificationType::Message(msg) => div().child(Label::new(msg.clone()).text_sm().text_color(cx.theme().primary)),
            NotificationType::Error(err) => div().child(Label::new(err.clone()).text_sm().text_color(cx.theme().danger)),
        };

        v_flex()
            .id("send-view")
            .m_2()
            .gap_1()
            .child(file_selector)
            .child(send_card)
            .when(pages > 1, |this| {
                this.child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("previous-page").small().label(i18n_common(cx, "previous")).disabled(page == 0).on_click(cx.listener(
                                move |view, _, _, cx| {
                                    view.page = page.saturating_sub(1);
                                    cx.notify();
                                },
                            )),
                        )
                        .child(Label::new(format!("{} / {}", page + 1, pages)).text_sm())
                        .child(
                            Button::new("next-page").small().label(i18n_common(cx, "next")).disabled(page + 1 >= pages).on_click(cx.listener(
                                move |view, _, _, cx| {
                                    view.page = (page + 1).min(pages - 1);
                                    cx.notify();
                                },
                            )),
                        ),
                )
            })
            .child(file_counter_with_cleanup)
            .child(send_button)
            .child(share_code)
            .child(notification_view)
            .when(self.total_entries > 0, |this| {
                this.child(
                    Label::new(format!(
                        "{} {} · {} {} · {} {} · {} {}",
                        self.results.succeeded,
                        i18n_common(cx, "succeeded"),
                        self.results.skipped,
                        i18n_common(cx, "skip"),
                        self.results.failed,
                        i18n_common(cx, "failed"),
                        self.results.remaining(self.total_entries),
                        i18n_common(cx, "unfinished")
                    ))
                    .text_sm(),
                )
            })
    }
}
