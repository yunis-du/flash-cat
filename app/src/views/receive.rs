use flash_cat_core::{FileStatus, TransferResults};
use futures::SinkExt;
use std::collections::HashMap;
use std::sync::Arc;

use flash_cat_common::{consts::PUBLIC_RELAY, proto::ClientType};
use flash_cat_core::{ReceiverConfirm, ReceiverInteractionMessage, receiver::FlashCatReceiver};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    label::Label,
    spinner::Spinner,
    v_flex,
};
use gpui_kit::{AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder};
use rust_i18n::t;
use tokio_stream::StreamExt;

use crate::{
    assets::CustomIconName,
    components::{Card, ProgressBar},
    helpers::{i18n_common, i18n_receive, spawn_transfer},
    state::FlashCatAppGlobalStore,
};

#[derive(PartialEq, Eq, Clone)]
enum ReceiveState {
    Idle,
    Connecting,
    AwaitingConfirm,
    Receiving,
    ReceiveDone,
}

#[derive(PartialEq, Eq, Clone)]
enum NotificationType {
    None,
    Message(String),
    Error(String),
    ConfirmReceive {
        file_count: u64,
        folder_count: u64,
    },
    ConfirmFileDuplicate {
        file_id: u64,
        file_path: String,
    },
    ConfirmOpenSavePath,
}

pub struct ReceiveView {
    receive_state: ReceiveState,
    share_code_state: Entity<InputState>,
    lan: bool,
    receive_but_hover: bool,
    flash_cat_receiver: Option<Arc<FlashCatReceiver>>,
    progress_bars: Vec<ProgressBar>,
    progress_index: HashMap<u64, usize>,
    page: usize,
    notification: NotificationType,
    num_files: u64,
    generation: u64,
    results: TransferResults,
    total_entries: u64,
}

impl ReceiveView {
    fn progress_mut(
        &mut self,
        file_id: u64,
    ) -> Option<&mut ProgressBar> {
        let index = *self.progress_index.get(&file_id)?;
        self.progress_bars.get_mut(index)
    }

    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let share_code_state = cx.new(|cx| InputState::new(window, cx));

        Self {
            receive_state: ReceiveState::Idle,
            share_code_state,
            lan: false,
            receive_but_hover: false,
            flash_cat_receiver: None,
            progress_bars: vec![],
            progress_index: HashMap::new(),
            page: 0,
            notification: NotificationType::None,
            num_files: 0,
            generation: 0,
            results: TransferResults::default(),
            total_entries: 0,
        }
    }

    fn send_confirm(
        &self,
        confirm: ReceiverConfirm,
    ) {
        if let Some(fcr) = &self.flash_cat_receiver {
            let fcr = fcr.clone();
            let _ = spawn_transfer(async move {
                let _ = fcr.send_confirm(confirm).await;
            });
        }
    }

    fn get_share_code(
        &self,
        cx: &Context<Self>,
    ) -> String {
        self.share_code_state.read(cx).value().to_string()
    }
}

impl Render for ReceiveView {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let share_code_input = {
            let disabled = self.receive_state != ReceiveState::Idle;
            h_flex()
                .gap_2()
                .child(Label::new(i18n_common(cx, "share_code")).text_color(cx.theme().muted_foreground))
                .child(div().flex_1().child(Input::new(&self.share_code_state).max_w_56().disabled(disabled).small()))
        };

        let lan_checkbox = {
            let disabled = self.receive_state != ReceiveState::Idle;
            let checked = self.lan;
            h_flex()
                .gap_2()
                .child(
                    Checkbox::new("lan_checkbox").label(i18n_receive(cx, "lan")).checked(checked).disabled(disabled).on_click(cx.listener(
                        |view, checked: &bool, _, _| {
                            view.lan = *checked;
                        },
                    )),
                )
                .child(Button::new("lan_tooltip").icon(CustomIconName::Help).cursor_pointer().ghost().small().tooltip(i18n_receive(cx, "lan_tooltip")))
        };

        let pages = self.progress_bars.len().div_ceil(64).max(1);
        let page = self.page.min(pages - 1);

        let receive_card = {
            let placeholder = div().flex().size_full().justify_center().items_center().child(Label::new(i18n_receive(cx, "placeholder")));

            let mut items = vec![];
            for progress_bar in self.progress_bars.iter().skip(page * 64).take(64) {
                items.push(div().p_2().mb_1().bg(cx.theme().list_hover).rounded_md().child(progress_bar.clone().into_element()));
            }

            Card::new("receive-view-card").overflow_y_scrollbar().h_72().when(self.progress_bars.is_empty(), |this| this.child(placeholder)).when(
                !self.progress_bars.is_empty(),
                |mut this| {
                    for file in items {
                        this = this.child(file);
                    }
                    this
                },
            )
        };

        let receive_button = {
            let label = match &self.receive_state {
                ReceiveState::Idle => Some(if matches!(self.notification, NotificationType::Error(_)) {
                    i18n_common(cx, "retry")
                } else {
                    i18n_receive(cx, "receive")
                }),
                ReceiveState::Connecting | ReceiveState::AwaitingConfirm => {
                    if self.receive_but_hover {
                        Some(i18n_receive(cx, "cancel_receive"))
                    } else {
                        None // show spinner
                    }
                }
                ReceiveState::Receiving => {
                    if self.receive_but_hover {
                        Some(i18n_receive(cx, "cancel_receive"))
                    } else {
                        Some(i18n_receive(cx, "receiving"))
                    }
                }
                ReceiveState::ReceiveDone => Some(i18n_receive(cx, "receive_done")),
            };

            let spinner = Spinner::new().color(cx.theme().background);

            let share_code_empty = self.get_share_code(cx).is_empty();
            let disabled = share_code_empty && self.receive_state == ReceiveState::Idle;

            let mut button = Button::new("receive_button").size_full().h_10().info().disabled(disabled).when(!disabled, |this| this.cursor_pointer());

            if self.receive_state == ReceiveState::Connecting || self.receive_state == ReceiveState::AwaitingConfirm {
                if !self.receive_but_hover {
                    button = button.child(div().flex().justify_center().child(spinner));
                }
            }

            if self.receive_state == ReceiveState::Connecting
                || self.receive_state == ReceiveState::AwaitingConfirm
                || self.receive_state == ReceiveState::Receiving
            {
                button = button.on_hover(cx.listener(|view, hover, _, _| {
                    view.receive_but_hover = *hover;
                }));
            }

            if let Some(label) = label {
                button = button.label(label);
            }

            button.on_click(cx.listener(move |view, _, window, cx| match view.receive_state {
                ReceiveState::Idle => {
                    let share_code = view.get_share_code(cx);
                    if share_code.is_empty() {
                        return;
                    }
                    view.generation += 1;
                    let generation = view.generation;
                    view.results = TransferResults::default();
                    if let Some(receiver) = view.flash_cat_receiver.take() {
                        receiver.shutdown();
                    }
                    view.progress_bars.clear();
                    view.progress_index.clear();
                    view.page = 0;
                    view.notification = NotificationType::None;
                    view.receive_state = ReceiveState::Connecting;
                    let relay_addr = cx.global::<FlashCatAppGlobalStore>().read(cx).relay_address();
                    let save_path = cx.global::<FlashCatAppGlobalStore>().read(cx).save_path();

                    let relay = if relay_addr.contains(PUBLIC_RELAY) {
                        None
                    } else {
                        Some(relay_addr)
                    };

                    let lan = view.lan;

                    let fcr = FlashCatReceiver::new(share_code, relay, Some(save_path), ClientType::App, lan);
                    match fcr {
                        Ok(fcr) => {
                            let fcr = Arc::new(fcr);
                            view.flash_cat_receiver.replace(fcr.clone());

                            cx.spawn(async move |view, cx| {
                                // Create a channel to receive messages from tokio runtime
                                let (mut tx, mut rx) = futures::channel::mpsc::channel::<ReceiverInteractionMessage>(128);

                                let fcr_for_thread = view
                                    .update(cx, |view, _| {
                                        (view.generation == generation).then(|| view.flash_cat_receiver.clone()).flatten()
                                    })
                                    .ok()
                                    .flatten();
                                if let Some(fcr) = fcr_for_thread {
                                    let mut errors = tx.clone();
                                    let result = spawn_transfer(async move {
                                        match fcr.clone().start().await {
                                            Ok(mut stream) => {
                                                while let Some(msg) = stream.next().await {
                                                    let terminal = matches!(
                                                        &msg,
                                                        ReceiverInteractionMessage::ReceiveDone
                                                            | ReceiverInteractionMessage::Error(_)
                                                            | ReceiverInteractionMessage::OtherClose
                                                            | ReceiverInteractionMessage::ReconnectFailed(_)
                                                    );
                                                    if tx.send(msg).await.is_err() || terminal {
                                                        break;
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                let _ = tx.send(ReceiverInteractionMessage::Error(error.to_string())).await;
                                            }
                                        }
                                        fcr.shutdown();
                                    });
                                    if let Err(error) = result {
                                        let _ = errors.send(ReceiverInteractionMessage::Error(error.to_string())).await;
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
                                                    ReceiverInteractionMessage::TransferMode(_) => {}
                                                    ReceiverInteractionMessage::Message(msg) => {
                                                        view.notification = NotificationType::Message(msg);
                                                    }
                                                    ReceiverInteractionMessage::Error(e) => {
                                                        if let Some(receiver) = view.flash_cat_receiver.take() {
                                                            receiver.shutdown();
                                                        }
                                                        let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
                                                        if e.contains("NotFound") {
                                                            view.notification =
                                                                NotificationType::Error(t!("receive.error_share_code_not_found", locale = locale).to_string());
                                                        } else {
                                                            view.notification =
                                                                NotificationType::Error(t!("receive.error_other", error = e, locale = locale).to_string());
                                                        }
                                                        view.receive_state = ReceiveState::Idle;
                                                        return true;
                                                    }
                                                    ReceiverInteractionMessage::SendFilesRequest(req) => {
                                                        view.num_files = req.num_files;
                                                        view.total_entries = req.num_entries;
                                                        view.notification = NotificationType::ConfirmReceive {
                                                            file_count: req.num_files,
                                                            folder_count: req.num_folders,
                                                        };
                                                        view.receive_state = ReceiveState::AwaitingConfirm;
                                                    }
                                                    ReceiverInteractionMessage::FileRenamed((_, path)) => {
                                                        view.notification = NotificationType::Message(format!("Saving as {path}"));
                                                    }
                                                    ReceiverInteractionMessage::FileDuplication(dup) => {
                                                        view.notification = NotificationType::ConfirmFileDuplicate {
                                                            file_id: dup.file_id,
                                                            file_path: dup.path,
                                                        };
                                                    }
                                                    ReceiverInteractionMessage::RecvNewFile(new_file) => {
                                                        if !view.progress_index.contains_key(&new_file.file_id) {
                                                            view.progress_index.insert(new_file.file_id, view.progress_bars.len());
                                                            view.progress_bars.push(ProgressBar::new(new_file.file_id, new_file.filename, new_file.size));
                                                        }
                                                        if view.receive_state != ReceiveState::Receiving {
                                                            view.receive_state = ReceiveState::Receiving;
                                                            view.notification = NotificationType::None;
                                                        }
                                                    }
                                                    ReceiverInteractionMessage::BreakPoint(bp) => {
                                                        if let Some(pb) = view.progress_mut(bp.file_id) {
                                                            pb.set_progress(bp.position);
                                                        }
                                                        view.send_confirm(ReceiverConfirm::BreakPointConfirm((true, bp.file_id, bp.position)));
                                                    }
                                                    ReceiverInteractionMessage::FileStage(stage) => {
                                                        if let Some(pb) = view.progress_mut(stage.file_id) {
                                                            pb.set_stage(stage.phase, stage.position);
                                                        }
                                                    }
                                                    ReceiverInteractionMessage::FileProgress(progress) => {
                                                        if let Some(pb) = view.progress_mut(progress.file_id) {
                                                            pb.set_progress(progress.position);
                                                        }
                                                    }
                                                    ReceiverInteractionMessage::FileResult(result) => {
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
                                                    ReceiverInteractionMessage::OtherClose => {
                                                        if let Some(receiver) = view.flash_cat_receiver.take() {
                                                            receiver.shutdown();
                                                        }
                                                        view.notification = NotificationType::Error(i18n_common(cx, "sender_disconnected").to_string());
                                                        view.receive_state = ReceiveState::Idle;
                                                        return true;
                                                    }
                                                    ReceiverInteractionMessage::ReconnectFailed(error) => {
                                                        if let Some(receiver) = view.flash_cat_receiver.take() {
                                                            receiver.shutdown();
                                                        }
                                                        view.notification = NotificationType::Error(error);
                                                        view.receive_state = ReceiveState::Idle;
                                                        return true;
                                                    }
                                                    ReceiverInteractionMessage::ReceiveDone => {
                                                        view.receive_state = ReceiveState::ReceiveDone;
                                                        view.notification = NotificationType::ConfirmOpenSavePath;
                                                    }
                                                }
                                                cx.notify();
                                                false
                                            })
                                            .ok()
                                            .unwrap_or(true);

                                        if should_break {
                                            break;
                                        }
                                    }
                                    let _ = view.update(cx, |view, cx| {
                                        if view.generation == generation
                                            && matches!(
                                                view.receive_state,
                                                ReceiveState::Connecting | ReceiveState::AwaitingConfirm | ReceiveState::Receiving
                                            )
                                        {
                                            if let Some(receiver) = view.flash_cat_receiver.take() {
                                                receiver.shutdown();
                                            }
                                            view.notification = NotificationType::Error(i18n_common(cx, "transfer_stopped").to_string());
                                            view.receive_state = ReceiveState::Idle;
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .detach();
                        }
                        Err(e) => {
                            let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
                            view.notification = NotificationType::Error(t!("receive.error_other", error = e.to_string(), locale = locale).to_string());
                            view.receive_state = ReceiveState::Idle;
                        }
                    }
                }
                ReceiveState::Connecting | ReceiveState::AwaitingConfirm | ReceiveState::Receiving => {
                    view.generation += 1;
                    // Cancel receive
                    if let Some(fcr) = view.flash_cat_receiver.take() {
                        fcr.shutdown();
                    }
                    view.receive_state = ReceiveState::Idle;
                    view.notification = NotificationType::None;
                    view.progress_bars.clear();
                    view.progress_index.clear();
                    view.page = 0;
                }
                ReceiveState::ReceiveDone => {
                    // Reset after completion
                    view.total_entries = 0;
                    view.results = TransferResults::default();
                    view.progress_bars.clear();
                    view.progress_index.clear();
                    view.page = 0;
                    view.flash_cat_receiver = None;
                    view.share_code_state.update(cx, |state, cx| {
                        state.set_value("".to_string(), window, cx);
                    });
                    view.notification = NotificationType::None;
                    view.receive_state = ReceiveState::Idle;
                }
            }))
        };

        let notification_view = match &self.notification {
            NotificationType::None => div(),
            NotificationType::Message(msg) => div().child(Label::new(msg.clone()).text_sm().text_color(cx.theme().primary)),
            NotificationType::Error(err) => div().child(Label::new(err.clone()).text_sm().text_color(cx.theme().danger)),
            NotificationType::ConfirmReceive {
                file_count,
                folder_count,
            } => {
                let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
                let msg = if *folder_count > 0 {
                    t!(
                        "receive.confirm_receive",
                        file_count = file_count,
                        folder_count = folder_count,
                        locale = locale
                    )
                } else {
                    t!("receive.confirm_receive_no_folder", file_count = file_count, locale = locale)
                };
                h_flex()
                    .gap_2()
                    .child(Label::new(msg.to_string()).text_sm().text_color(cx.theme().primary))
                    .child(
                        Button::new("confirm_yes").small().info().label("Yes").cursor_pointer().on_click(cx.listener(|view, _, _, _| {
                            view.send_confirm(ReceiverConfirm::ReceiveConfirm(true));
                            view.notification = NotificationType::None;
                        })),
                    )
                    .child(
                        Button::new("confirm_no").small().ghost().label("No").cursor_pointer().on_click(cx.listener(|view, _, _, _| {
                            view.send_confirm(ReceiverConfirm::ReceiveConfirm(false));
                            view.notification = NotificationType::None;
                            view.receive_state = ReceiveState::Idle;
                            if let Some(fcr) = view.flash_cat_receiver.take() {
                                fcr.shutdown();
                            }
                        })),
                    )
            }
            NotificationType::ConfirmFileDuplicate {
                file_id,
                file_path,
            } => {
                let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
                let msg = t!("receive.file_duplicate", file_path = file_path, locale = locale);
                let file_id = *file_id;
                h_flex()
                    .gap_2()
                    .child(Label::new(msg.to_string()).text_sm().text_color(cx.theme().primary))
                    .child(
                        Button::new("dup_yes").small().info().label("Yes").cursor_pointer().on_click(cx.listener(move |view, _, _, _| {
                            view.send_confirm(ReceiverConfirm::FileConfirm((true, file_id)));
                            view.notification = NotificationType::None;
                        })),
                    )
                    .child(
                        Button::new("dup_no").small().ghost().label("No").cursor_pointer().on_click(cx.listener(move |view, _, _, _| {
                            view.send_confirm(ReceiverConfirm::FileConfirm((false, file_id)));
                            if let Some(pb) = view.progress_mut(file_id) {
                                pb.skip();
                            }
                            view.notification = NotificationType::None;
                        })),
                    )
            }
            NotificationType::ConfirmOpenSavePath => {
                let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
                let msg = t!("receive.open_save_path", locale = locale);
                h_flex()
                    .gap_2()
                    .child(Label::new(msg.to_string()).text_sm().text_color(cx.theme().primary))
                    .child(
                        Button::new("open_yes").small().info().label("Yes").cursor_pointer().on_click(cx.listener(|view, _, _, cx| {
                            let save_path = cx.global::<FlashCatAppGlobalStore>().read(cx).save_path();
                            let _ = open::that(save_path);
                            view.notification = NotificationType::None;
                        })),
                    )
                    .child(
                        Button::new("open_no").small().ghost().label("No").cursor_pointer().on_click(cx.listener(|view, _, _, _| {
                            view.notification = NotificationType::None;
                        })),
                    )
            }
        };

        v_flex()
            .id("receive-view")
            .m_2()
            .gap_1()
            .child(share_code_input)
            .child(lan_checkbox)
            .child(receive_card)
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
            .child(receive_button)
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
