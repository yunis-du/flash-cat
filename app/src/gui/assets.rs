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
    pub static NOTOSANS_REGULAR_STATIC: &[u8] =
        include_bytes!("../../assets/fonts/NotoSans-Regular.ttf");
}

pub mod logos {
    pub static IMG_LOGO: &[u8] = include_bytes!("../../assets/logos/flash-cat.png");
}