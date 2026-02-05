use gpui::{Context, InteractiveElement, IntoElement, ParentElement, Render, Styled, Window, div, prelude::FluentBuilder, px};
use gpui_component::{ActiveTheme, Icon, IconName, h_flex, label::Label, list::ListItem};

use crate::{
    assets::CustomIconName,
    helpers::i18n_tab,
    state::{FlashCatAppGlobalStore, Route, update_app_state_and_save},
};

pub struct TabView {}

impl TabView {
    pub fn new(
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {}
    }

    fn render_tab(
        &self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let send_label = i18n_tab(cx, "send_label");
        let receive_label = i18n_tab(cx, "receive_label");
        let settings_label = i18n_tab(cx, "settings_label");

        let tabs = vec![send_label, receive_label, settings_label];

        let list_active_color = cx.theme().list_active;
        let list_active_border_color = cx.theme().list_active_border;

        let route = cx.global::<FlashCatAppGlobalStore>().read(cx).route();
        let current_index = match route {
            Route::Send => 0,
            Route::Receive => 1,
            Route::Settings => 2,
        };

        h_flex().w_full().children(tabs.into_iter().enumerate().map(|(index, name)| {
            let is_current = index == current_index;

            let icon = match index {
                0 => Icon::new(CustomIconName::Sender),
                1 => Icon::new(CustomIconName::Receiver),
                _ => Icon::new(IconName::Settings),
            };

            ListItem::new(("tab", index))
                .w_full()
                .cursor_pointer()
                .when(is_current, |this| this.bg(list_active_color))
                .py_3()
                .px_4()
                .when(is_current, |this| this.border_b(px(2.0)).border_color(list_active_border_color))
                .child(h_flex().items_center().justify_center().gap_2().child(icon.size_4()).child(Label::new(name).text_xs()))
                .on_click(move |_, _window, cx| {
                    if is_current {
                        return;
                    }

                    let new_route = match index {
                        0 => Route::Send,
                        1 => Route::Receive,
                        2 => Route::Settings,
                        _ => return,
                    };

                    update_app_state_and_save(cx, "change_route", move |state, _cx| {
                        state.go_to(new_route);
                    });
                })
        }))
    }
}

impl Render for TabView {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex()
            .size_full()
            .id("tab-container")
            .justify_start()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().size_full().child(self.render_tab(window, cx)))
    }
}
