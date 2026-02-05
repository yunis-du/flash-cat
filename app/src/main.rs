#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

#[cfg(not(target_os = "linux"))]
use gpui::TitlebarOptions;
use gpui::{App, Application, Bounds, Entity, Menu, MenuItem, Window, WindowAppearance, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_component::{ActiveTheme, Root, Theme, ThemeMode, v_flex};

use crate::{
    helpers::{LocaleAction, MemuAction, ThemeAction, new_hot_keys},
    state::{FlashCatAppGlobalStore, FlashCatAppState, update_app_state_and_save},
    views::{FlashCatAppContent, FlashCatAppHeader, TitleBarView, open_about_window},
};

rust_i18n::i18n!("locales", fallback = "en");

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

mod assets;
mod components;
mod helpers;
mod state;
mod views;

pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

struct FlashCatApp {
    // views
    title_bar: Entity<TitleBarView>,
    header: Entity<FlashCatAppHeader>,
    content: Entity<FlashCatAppContent>,
}

impl FlashCatApp {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let title_bar = cx.new(|cx| TitleBarView::new(window, cx));
        let header = cx.new(|cx| FlashCatAppHeader::new(window, cx));
        let content = cx.new(|cx| FlashCatAppContent::new(window, cx));

        cx.observe_window_appearance(window, |_this, _window, cx| {
            if cx.global::<FlashCatAppGlobalStore>().read(cx).theme().is_none() {
                Theme::change(cx.window_appearance(), None, cx);
                cx.refresh_windows();
            }
        })
        .detach();

        Self {
            title_bar,
            header,
            content,
        }
    }
}

impl Render for FlashCatApp {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(_window, cx);

        v_flex()
            .id(PKG_NAME)
            .bg(cx.theme().background)
            .size_full()
            .child(self.title_bar.clone())
            .child(self.header.clone())
            .child(self.content.clone())
            .children(dialog_layer)
    }
}

fn main() {
    let app = Application::new().with_assets(assets::Assets);
    let app_state = FlashCatAppState::try_new().unwrap_or_else(|_| FlashCatAppState::new());

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        cx.activate(true);
        let window_bounds = {
            let window_size = size(px(480.), px(720.));
            Bounds::centered(None, window_size, cx)
        };
        let app_state = cx.new(|_| app_state);
        let app_store = FlashCatAppGlobalStore::new(app_state);
        if let Some(theme) = app_store.read(cx).theme() {
            Theme::change(theme, None, cx);
        }

        cx.set_global(app_store);
        cx.bind_keys(new_hot_keys());

        cx.on_action(|action: &ThemeAction, cx: &mut App| {
            // Convert action to theme mode
            let mode = match action {
                ThemeAction::Light => Some(ThemeMode::Light),
                ThemeAction::Dark => Some(ThemeMode::Dark),
                ThemeAction::System => None, // Follow OS theme
            };

            // Determine actual render mode (resolve System to Light/Dark)
            let render_mode = match mode {
                Some(m) => m,
                None => match cx.window_appearance() {
                    WindowAppearance::Light => ThemeMode::Light,
                    _ => ThemeMode::Dark,
                },
            };

            // Apply theme immediately for instant visual feedback
            Theme::change(render_mode, None, cx);

            // Save preference to disk asynchronously
            update_app_state_and_save(cx, "save_theme", move |state, _cx| {
                state.set_theme(mode);
            });
        });

        cx.on_action(|action: &LocaleAction, cx: &mut App| {
            let locale = match action {
                LocaleAction::Zh => "zh",
                LocaleAction::En => "en",
            };

            // Save locale preference and refresh UI
            update_app_state_and_save(cx, "save_locale", move |state, _cx| {
                state.set_locale(locale.to_string());
            });
        });

        cx.on_action(|e: &MemuAction, cx: &mut App| match e {
            MemuAction::Quit => {
                cx.quit();
            }
            MemuAction::About => {
                open_about_window(cx);
            }
        });
        cx.set_menus(vec![Menu {
            name: "FlashCatApp".into(),
            items: vec![MenuItem::action("About", MemuAction::About), MenuItem::action("Quit", MemuAction::Quit)],
        }]);

        cx.spawn(async move |cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(window_bounds)),
                    #[cfg(not(target_os = "linux"))]
                    titlebar: Some(TitlebarOptions {
                        title: None,
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(px(9.0), px(9.0))),
                    }),
                    show: true,
                    is_resizable: false,
                    ..Default::default()
                },
                |window, cx| {
                    #[cfg(target_os = "macos")]
                    window.on_window_should_close(cx, move |_window, cx| {
                        cx.quit();
                        true
                    });
                    let content_view = cx.new(|cx| FlashCatApp::new(window, cx));
                    cx.new(|cx| Root::new(content_view, window, cx))
                },
            )?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
