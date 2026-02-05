use gpui::App;
use gpui::SharedString;
use rust_i18n::t;

use crate::state::FlashCatAppGlobalStore;

pub fn i18n_common<'a>(
    cx: &'a App,
    key: &'a str,
) -> SharedString {
    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
    t!(format!("common.{key}"), locale = locale).into()
}

pub fn i18n_titlebar<'a>(
    cx: &'a App,
    key: &'a str,
) -> SharedString {
    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
    t!(format!("titlebar.{key}"), locale = locale).into()
}

pub fn i18n_tab<'a>(
    cx: &'a App,
    key: &'a str,
) -> SharedString {
    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
    t!(format!("tab.{key}"), locale = locale).into()
}

pub fn i18n_send<'a>(
    cx: &'a App,
    key: &'a str,
) -> SharedString {
    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
    t!(format!("send.{key}"), locale = locale).into()
}

pub fn i18n_settings<'a>(
    cx: &'a App,
    key: &'a str,
) -> SharedString {
    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
    t!(format!("settings.{key}"), locale = locale).into()
}

pub fn i18n_receive<'a>(
    cx: &'a App,
    key: &'a str,
) -> SharedString {
    let locale = cx.global::<FlashCatAppGlobalStore>().read(cx).locale();
    t!(format!("receive.{key}"), locale = locale).into()
}
