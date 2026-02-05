use chrono::{Datelike, Local};
use gpui::{App, Bounds, TitlebarOptions, Window, WindowBounds, WindowKind, WindowOptions, prelude::*, px, size};
use gpui_component::{ActiveTheme, Icon, h_flex, label::Label, v_flex};

use crate::assets::CustomIconName;

struct AboutView;

const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Render for AboutView {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let year = Local::now().year().to_string();
        let years = if year == "2026" {
            "2026"
        } else {
            "2026 - {year}"
        };
        v_flex()
            .size_full()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            // LOGO
            .child(h_flex().items_center().justify_center().child(Icon::new(CustomIconName::Logo).size(px(64.)).text_color(cx.theme().primary)))
            .child(Label::new("Flash Cat App").text_xl())
            .child(Label::new(format!("Version {VERSION}")).text_sm().text_color(cx.theme().muted_foreground))
            .child(Label::new(format!("© {years} Yunis du. All rights reserved.")).text_xs().text_color(cx.theme().muted_foreground))
    }
}

pub fn open_about_window(cx: &mut App) {
    let width = px(300.);
    let height = px(200.);
    let window_size = size(width, height);

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, window_size, cx))),
        is_movable: false,
        is_resizable: false,

        titlebar: Some(TitlebarOptions {
            title: Some("About FlashCatApp".into()),
            appears_transparent: true,
            ..Default::default()
        }),
        focus: true,
        kind: WindowKind::Normal,
        ..Default::default()
    };

    let _ = cx.open_window(options, |_, cx| cx.new(|_cx| AboutView));
}
