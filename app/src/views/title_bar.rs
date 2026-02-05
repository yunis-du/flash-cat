use gpui::{App, Context, Corner, Window, prelude::*};
use gpui_component::{
    IconName, Sizable, ThemeMode, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    menu::{DropdownMenu, PopupMenu},
};

use crate::{
    helpers::{LocaleAction, ThemeAction, i18n_titlebar},
    state::FlashCatAppGlobalStore,
};

pub struct TitleBarView;

impl TitleBarView {
    pub fn new(
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self
    }

    fn render_settings_menu(
        view: PopupMenu,
        cx: &App,
    ) -> PopupMenu {
        let store = cx.global::<FlashCatAppGlobalStore>().read(cx);
        let (locale, theme) = (store.locale(), store.theme());

        view
            // language menu
            .label(i18n_titlebar(cx, "language"))
            .menu_with_check("中文", locale == "zh", Box::new(LocaleAction::Zh))
            .menu_with_check("English", locale == "en", Box::new(LocaleAction::En))
            .separator()
            // theme menu
            .label(i18n_titlebar(cx, "theme"))
            .menu_with_check(
                i18n_titlebar(cx, "light"),
                theme == Some(ThemeMode::Light),
                Box::new(ThemeAction::Light),
            )
            .menu_with_check(
                i18n_titlebar(cx, "dark"),
                theme == Some(ThemeMode::Dark),
                Box::new(ThemeAction::Dark),
            )
            .menu_with_check(i18n_titlebar(cx, "system"), theme.is_none(), Box::new(ThemeAction::System))
            .separator()
    }
}

impl Render for TitleBarView {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // right actions container
        let right_actions = h_flex().items_center().justify_end().px_2().gap_2().mr_2();

        TitleBar::new()
            // left placeholder
            .child(h_flex().flex_1())
            // right actions container
            .child(
                right_actions
                    .child(
                        Button::new("settings")
                            .cursor_pointer()
                            .icon(IconName::Settings2)
                            .small()
                            .ghost()
                            .dropdown_menu(move |this, _, cx| Self::render_settings_menu(this, cx))
                            .anchor(Corner::TopRight),
                    )
                    .child(
                        Button::new("github")
                            .cursor_pointer()
                            .tooltip(i18n_titlebar(cx, "github_tooltip"))
                            .icon(IconName::GitHub)
                            .small()
                            .ghost()
                            .on_click(|_, _, cx| cx.open_url("https://github.com/yunis-du/flash-cat")),
                    ),
            )
    }
}
