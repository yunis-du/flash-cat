use gpui::{Context, FontWeight, Image, ImageFormat, IntoElement, ParentElement, Render, Styled, Window, img, px};
use gpui_component::{ActiveTheme, h_flex, label::Label, v_flex};
use std::sync::Arc;

use crate::helpers::i18n_common;

pub struct FlashCatAppHeader {
    logo_image: Arc<Image>,
}

impl FlashCatAppHeader {
    pub fn new(
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            logo_image: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../../assets/logos/flash-cat.png").to_vec(),
            )),
        }
    }
}

impl Render for FlashCatAppHeader {
    fn render(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        h_flex().w_full().items_center().justify_center().gap_4().py_6().child(img(self.logo_image.clone()).h(px(50.0))).child(
            v_flex()
                .gap_1()
                .child(Label::new("Flash Cat").text_xl().font_weight(FontWeight::BOLD))
                .child(Label::new(i18n_common(cx, "flashcat_description")).text_xs().text_color(cx.theme().muted_foreground)),
        )
    }
}
