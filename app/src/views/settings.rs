use std::path::Path;

use gpui::{AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, Window, div};
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    label::Label,
    v_flex,
};

use crate::{
    assets::CustomIconName,
    built_info,
    components::Card,
    helpers::{i18n_common, i18n_settings, pick_folder},
    state::{FlashCatAppGlobalStore, update_app_state_and_save},
};

pub struct SettingsView {
    edit_relay_address: bool,
    relay_address: String,
    relay_address_state: Entity<InputState>,
    save_path: String,
}

impl SettingsView {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (relay_address, save_path) = {
            let store = cx.global::<FlashCatAppGlobalStore>().read(cx);
            (store.relay_address(), store.save_path())
        };

        let relay_address_state = cx.new(|cx| {
            let mut state = InputState::new(window, cx);
            state.set_value(relay_address.clone(), window, cx);
            state
        });

        Self {
            edit_relay_address: false,
            relay_address,
            relay_address_state,
            save_path,
        }
    }

    fn general_settings_card(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let relay_setting = v_flex()
            .mb_2()
            .child(
                Label::new(i18n_settings(cx, "relay_address"))
                    .secondary(i18n_settings(cx, "relay_address_description"))
                    .text_base()
                    .whitespace_nowrap()
                    .text_ellipsis(),
            )
            .child(if self.edit_relay_address {
                h_flex().items_center().child(h_flex().w_full().child(Input::new(&self.relay_address_state).small())).child(
                    h_flex()
                        .gap_1()
                        .w_full()
                        .justify_end()
                        .child(
                            Button::new("edit-relay-save")
                                .icon(IconName::Check)
                                .small()
                                .ghost()
                                .cursor_pointer()
                                .tooltip(i18n_common(cx, "update_tooltip"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let new_address = this.relay_address_state.read(cx).value().to_string();
                                    this.relay_address = new_address.clone();
                                    this.edit_relay_address = false;
                                    update_app_state_and_save(cx, "update relay address", move |state, _| {
                                        state.set_relay_address(new_address);
                                    });
                                })),
                        )
                        .child(
                            Button::new("edit-relay-cancel")
                                .icon(CustomIconName::Remove)
                                .small()
                                .ghost()
                                .cursor_pointer()
                                .tooltip(i18n_common(cx, "cancel_tooltip"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.edit_relay_address = false;
                                    let old_address = this.relay_address.clone();
                                    this.relay_address_state.update(cx, |state, cx| {
                                        state.set_value(old_address, window, cx);
                                    });
                                })),
                        ),
                )
            } else {
                h_flex().items_center().child(h_flex().w_full().child(Label::new(&self.relay_address).text_sm().ml_2())).child(
                    h_flex().w_full().justify_end().child(
                        Button::new("edit-relay")
                            .icon(CustomIconName::Edit)
                            .small()
                            .ghost()
                            .cursor_pointer()
                            .tooltip(i18n_common(cx, "edit_tooltip"))
                            .on_click(cx.listener(|this, _, _, _| {
                                this.edit_relay_address = true;
                            })),
                    ),
                )
            });

        let save_path_setting = v_flex()
            .mb_2()
            .child(
                Label::new(i18n_settings(cx, "save_path"))
                    .secondary(i18n_settings(cx, "save_path_description"))
                    .text_base()
                    .whitespace_nowrap()
                    .text_ellipsis(),
            )
            .child(
                h_flex()
                    .items_center()
                    .child(
                        h_flex().w_full().child(
                            div()
                                .id("save-path")
                                .ml_2()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, _| {
                                    let _ = open::that(Path::new(this.save_path.as_str()));
                                }))
                                .hover(|s| s.opacity(0.7))
                                .child(Label::new(&self.save_path).text_sm().text_color(cx.theme().primary)),
                        ),
                    )
                    .child(
                        h_flex().w_full().justify_end().child(
                            Button::new("edit-save-path")
                                .icon(CustomIconName::Edit)
                                .small()
                                .ghost()
                                .cursor_pointer()
                                .tooltip(i18n_settings(cx, "edit_save_path_tooltip"))
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.spawn(async move |this, cx| {
                                        if let Ok(picked_path) = pick_folder().await {
                                            if let Some(picked_path) = picked_path {
                                                let save_path = picked_path.to_string_lossy().to_string();
                                                let _ = cx.update(|cx| {
                                                    let _ = this.update(cx, |this, _| {
                                                        this.save_path = save_path.clone();
                                                    });
                                                    update_app_state_and_save(cx, "save_download_path", move |state, _| {
                                                        state.set_save_path(save_path);
                                                    });
                                                });
                                            }
                                        }
                                    })
                                    .detach();
                                })),
                        ),
                    ),
            );

        Card::new("general-settings-card").title(i18n_settings(cx, "general")).m_2().child(relay_setting).child(save_path_setting)
    }

    fn about_card(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let items = [
            (i18n_settings(cx, "author"), built_info::PKG_AUTHORS),
            (i18n_settings(cx, "version"), built_info::PKG_VERSION),
            (i18n_settings(cx, "license"), built_info::PKG_LICENSE),
        ];
        let repository_url = built_info::PKG_REPOSITORY;

        Card::new("about-card").title(i18n_settings(cx, "about")).m_2().child(
            v_flex()
                .gap_1()
                .children(items.into_iter().map(|(label, value)| {
                    h_flex().child(div().w_24().child(Label::new(label).text_sm())).child(Label::new(value).text_sm().text_color(cx.theme().primary))
                }))
                .child(
                    h_flex().child(div().w_24().child(Label::new(i18n_settings(cx, "repository")).text_sm())).child(
                        div()
                            .id("repository-link")
                            .cursor_pointer()
                            .on_click(|_, _, cx| {
                                cx.open_url(repository_url);
                            })
                            .hover(|s| s.opacity(0.7))
                            .child(Label::new(repository_url).text_sm().text_color(cx.theme().primary)),
                    ),
                ),
        )
    }
}

impl Render for SettingsView {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        v_flex().id("settings").bg(cx.theme().background).size_full().child(self.general_settings_card(cx)).child(self.about_card(cx))
    }
}
