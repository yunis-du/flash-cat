pub mod icons {
    pub static CARD_CHECKLIST: &[u8] = include_bytes!("../../assets/icons/flash-cat.svg");
    pub static GITHUB_ICON: &[u8] = include_bytes!("../../assets/icons/github.svg");
    pub static GEAR_WIDE_CONNECTED: &[u8] =
        include_bytes!("../../assets/icons/gear-wide-connected.svg");
    pub static SENDER_ICON: &[u8] = include_bytes!("../../assets/icons/sender.svg");
    pub static SEND_BUTTON_ICON: &[u8] = include_bytes!("../../assets/icons/send_button.svg");
    pub static RECEIVER_ICON: &[u8] = include_bytes!("../../assets/icons/receiver.svg");
    pub static REMOVE_ICON: &[u8] = include_bytes!("../../assets/icons/remove.svg");
    pub static COPY_ICON: &[u8] = include_bytes!("../../assets/icons/copy.svg");
    pub static TICK_ICON: &[u8] = include_bytes!("../../assets/icons/tick.svg");
}

pub mod fonts {
    use std::borrow::Cow;

    pub static SOURCE_HAN_SANS_CN: &'static str = "Source Han Sans CN";

    pub static FONTS: &[Cow<'static, [u8]>] = &[
        // Source Han Sans CN
        Cow::Borrowed(include_bytes!("../../assets/fonts/SourceHanSansCN-Bold.otf").as_slice()),
        Cow::Borrowed(
            include_bytes!("../../assets/fonts/SourceHanSansCN-ExtraLight.otf").as_slice(),
        ),
        Cow::Borrowed(include_bytes!("../../assets/fonts/SourceHanSansCN-Heavy.otf").as_slice()),
        Cow::Borrowed(include_bytes!("../../assets/fonts/SourceHanSansCN-Light.otf").as_slice()),
        Cow::Borrowed(include_bytes!("../../assets/fonts/SourceHanSansCN-Medium.otf").as_slice()),
        Cow::Borrowed(include_bytes!("../../assets/fonts/SourceHanSansCN-Normal.otf").as_slice()),
        Cow::Borrowed(include_bytes!("../../assets/fonts/SourceHanSansCN-Regular.otf").as_slice()),
    ];
}

pub mod logos {
    pub static IMG_LOGO: &[u8] = include_bytes!("../../assets/logos/flash-cat.png");
}
