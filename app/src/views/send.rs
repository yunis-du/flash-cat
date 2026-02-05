use std::{sync::Arc, vec};

use flash_cat_common::{
    consts::PUBLIC_RELAY,
    proto::ClientType,
    utils::{
        fs::{FileCollector, collect_files},
        gen_share_code,
    },
};
use flash_cat_core::{SenderInteractionMessage, sender::FlashCatSender};
use gpui::{Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Sizable,
    button::{Button, ButtonVariants},
    clipboard::Clipboard,
    h_flex,
    label::Label,
    spinner::Spinner,
    v_flex,
};
use rust_i18n::t;
use tokio_stream::StreamExt;

use crate::{
    assets::CustomIconName,
    components::{Card, ProgressBar},
    helpers::{i18n_common, i18n_send, pick_files, pick_folders},
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
    file_collector: Option<FileCollector>,
    send_but_hover: bool,
    share_code: String,
    flash_cat_sender: Option<Arc<FlashCatSender>>,
    progress_bars: Vec<ProgressBar>,
    notification: NotificationType,
}

impl SendView {
    pub fn new(
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            send_state: SendState::Idle,
            selected_files: vec![],
            file_collector: None,
            send_but_hover: false,
            share_code: String::new(),
            flash_cat_sender: None,
            progress_bars: vec![],
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
                                                    for path in picked_path {
                                                        let path_str = path.to_string_lossy().to_string();
                                                        if !view.selected_files.contains(&path_str) {
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
                                                    for path in picked_path {
                                                        let path_str = path.to_string_lossy().to_string();
                                                        if !view.selected_files.contains(&path_str) {
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

        let send_card = {
            let placeholder = div().flex().size_full().justify_center().items_center().child(Label::new(i18n_send(cx, "placeholder")));

            let mut items = vec![];
            if self.send_state == SendState::AwaitingReceive || self.send_state == SendState::Sending || self.send_state == SendState::SendDone {
                for progress_bar in &self.progress_bars {
                    items.push(div().p_2().mb_1().bg(cx.theme().list_hover).rounded_md().child(progress_bar.clone().into_element()));
                }
            } else {
                for (i, file) in self.selected_files.iter().enumerate() {
                    items.push(div().p_2().mb_1().bg(cx.theme().list_hover).rounded_md().child(
                        h_flex().justify_between().child(Label::new(file.clone()).text_sm().text_color(cx.theme().primary)).child(
                            Button::new(("remove_path", i)).cursor_pointer().icon(CustomIconName::Remove).small().ghost().on_click(cx.listener(
                                move |view, _, _, _| {
                                    view.selected_files.remove(i);
                                    if view.selected_files.is_empty() {
                                        view.send_state = SendState::Idle;
                                    }
                                },
                            )),
                        ),
                    ));
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
                        view.progress_bars.clear();
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
                SendState::Idle | SendState::FileSelected => Some(i18n_send(cx, "send")),
                SendState::Collecting => None,
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

            let disabled = self.selected_files.is_empty() || self.send_state == SendState::Collecting;

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
                SendState::Idle | SendState::Collecting => (),
                SendState::FileSelected => {
                    view.send_state = SendState::Collecting;
                    let files = view.selected_files.clone();
                    let relay_addr = cx.global::<FlashCatAppGlobalStore>().read(cx).relay_address();

                    cx.spawn(async move |view, cx| {
                        let file_collector = cx.background_executor().spawn(async move { collect_files(&files) }).await;
                        view.update(cx, |view, _| {
                            view.file_collector = Some(file_collector.clone());

                            let specify_relay = if relay_addr.contains(PUBLIC_RELAY) {
                                None
                            } else {
                                Some(relay_addr)
                            };
                            let share_code = gen_share_code();
                            view.share_code = share_code.clone();
                            view.progress_bars.clear(); // in case of re-collecting files
                            file_collector.files.iter().for_each(|f| view.progress_bars.push(ProgressBar::new(f.file_id, f.name.clone(), f.size)));

                            let fcs = FlashCatSender::new_with_file_collector(share_code.clone(), specify_relay, file_collector.clone(), ClientType::App);
                            match fcs {
                                Ok(fcs) => {
                                    let fcs = Arc::new(fcs);
                                    view.flash_cat_sender.replace(fcs.clone());
                                }
                                Err(_) => {
                                    // todo handle error
                                }
                            }

                            view.send_state = SendState::AwaitingReceive;
                        })
                        .ok();

                        // Start file sending by listening to the sender stream
                        // Use a channel to receive progress updates from the tokio runtime
                        let fcs = view.update(cx, |view, _| view.flash_cat_sender.clone()).ok().flatten();
                        if let Some(fcs) = fcs {
                            // Create a channel to receive messages from tokio runtime
                            let (tx, mut rx) = futures::channel::mpsc::unbounded::<SenderInteractionMessage>();

                            // Spawn a thread with tokio runtime to run the sender
                            std::thread::spawn(move || {
                                let rt = tokio::runtime::Runtime::new().unwrap();
                                rt.block_on(async move {
                                    match fcs.start().await {
                                        Ok(mut stream) => {
                                            while let Some(msg) = stream.next().await {
                                                if tx.unbounded_send(msg).is_err() {
                                                    break;
                                                }
                                            }
                                        }
                                        Err(_e) => {
                                            // Handle start error
                                        }
                                    }
                                });
                            });

                            // Listen for messages from the tokio runtime
                            while let Some(msg) = rx.next().await {
                                let should_break = view
                                    .update(cx, |view, cx| {
                                        match msg {
                                            SenderInteractionMessage::Message(_msg) => {
                                                // Handle message notification if needed
                                            }
                                            SenderInteractionMessage::Error(e) => {
                                                // Handle error
                                                view.notification = NotificationType::Error(e);
                                                view.send_state = SendState::FileSelected;
                                                return true;
                                            }
                                            SenderInteractionMessage::ReceiverReject => {
                                                // Handle rejection
                                                view.notification = NotificationType::Error("Receiver rejected".to_string());
                                                view.send_state = SendState::FileSelected;
                                                return true;
                                            }
                                            SenderInteractionMessage::RelayFailed((_relay_type, error)) => {
                                                // Handle relay failure
                                                view.notification = NotificationType::Error(error);
                                            }
                                            SenderInteractionMessage::ContinueFile(file_id) => {
                                                // Skip this file
                                                if let Some(pb) = view.progress_bars.iter_mut().find(|pb| pb.get_file_id() == file_id) {
                                                    pb.skip();
                                                }
                                            }
                                            SenderInteractionMessage::FileProgress(progress) => {
                                                if view.send_state != SendState::Sending {
                                                    view.send_state = SendState::Sending;
                                                }
                                                if let Some(pb) = view.progress_bars.iter_mut().find(|pb| pb.get_file_id() == progress.file_id) {
                                                    pb.set_progress(progress.position);
                                                }
                                            }
                                            SenderInteractionMessage::FileProgressFinish(file_id) => {
                                                if let Some(pb) = view.progress_bars.iter_mut().find(|pb| pb.get_file_id() == file_id) {
                                                    pb.finish();
                                                }
                                            }
                                            SenderInteractionMessage::OtherClose => {
                                                // Handle other side close
                                                view.notification = NotificationType::Message("Receiver disconnected".to_string());
                                                return true;
                                            }
                                            SenderInteractionMessage::SendDone => {
                                                // Sending complete, waiting for confirmation
                                            }
                                            SenderInteractionMessage::Completed => {
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
                        }
                    })
                    .detach();
                }
                SendState::AwaitingReceive => {
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
            .child(file_counter_with_cleanup)
            .child(send_button)
            .child(share_code)
            .child(notification_view)
    }
}
