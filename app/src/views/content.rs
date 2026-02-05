use gpui::{Entity, Window, div, prelude::*};
use gpui_component::v_flex;
use tracing::{debug, info};

use crate::{
    state::{FlashCatAppGlobalStore, Route},
    views::TabView,
};

use super::{ReceiveView, SendView, SettingsView};

pub struct FlashCatAppContent {
    /// Cached views - lazily initialized and cleared when switching routes
    tab: Option<Entity<TabView>>,
    send: Option<Entity<SendView>>,
    receive: Option<Entity<ReceiveView>>,
    settings: Option<Entity<SettingsView>>,
}

impl FlashCatAppContent {
    pub fn new(
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        info!("Creating new content view");
        Self {
            tab: None,
            send: None,
            receive: None,
            settings: None,
        }
    }

    fn render_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Reuse existing view or create new one
        let tab = self
            .tab
            .get_or_insert_with(|| {
                debug!("Creating new tab view");
                cx.new(|cx| TabView::new(window, cx))
            })
            .clone();

        div().child(tab)
    }

    fn render_send(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Reuse existing view or create new one
        let send = self
            .send
            .get_or_insert_with(|| {
                debug!("Creating new servers view");
                cx.new(|cx| SendView::new(window, cx))
            })
            .clone();

        div().m_1().child(send)
    }

    fn render_receive(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Reuse existing view or create new one
        let receive = self
            .receive
            .get_or_insert_with(|| {
                debug!("Creating new servers view");
                cx.new(|cx| ReceiveView::new(window, cx))
            })
            .clone();

        div().m_1().child(receive)
    }

    fn render_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = self
            .settings
            .get_or_insert_with(|| {
                debug!("Creating new settings view");
                cx.new(|cx| SettingsView::new(window, cx))
            })
            .clone();
        div().m_1().child(settings)
    }
}

impl Render for FlashCatAppContent {
    fn render(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let route = cx.global::<FlashCatAppGlobalStore>().read(cx).route();

        v_flex().id("main-container").flex_1().h_full().child(self.render_tab(window, cx).into_any_element()).child(v_flex().flex_1().child(match route {
            Route::Send => self.render_send(window, cx).into_any_element(),
            Route::Receive => self.render_receive(window, cx).into_any_element(),
            Route::Settings => self.render_settings(window, cx).into_any_element(),
        }))
    }
}
