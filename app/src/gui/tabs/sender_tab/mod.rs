use iced::{
    widget::{
        column, mouse_area, row,
        scrollable::{self, RelativeOffset},
        text,
    },
    Command, Element,
};

use crate::gui::{assets::icons::SENDER_ICON, styles};

use super::Tab;

#[derive(Debug, Clone)]
pub enum Message {
    Selected
}

pub struct SenderTab {
    scrollable_offset: RelativeOffset,
}

impl SenderTab {
    pub fn new() -> (Self, Command<Message>) {
        (
            Self {
                scrollable_offset: RelativeOffset::START,
            },
            Command::none(),
        )
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Selected => Command::none(),
        }
    }

    pub fn view(&self) -> Element<Message> {
        let go_to_site_text = text("here")
            .size(11)
            .style(styles::text_styles::accent_color_theme());

        let sender_text = row![
            text("- Sender ")
                .size(11),
            mouse_area(go_to_site_text.clone())
        ];

        column![sender_text].into()
    }
}

impl Tab for SenderTab {
    type Message = Message;

    fn title() -> &'static str {
        "Sender"
    }

    fn icon_bytes() -> &'static [u8] {
        SENDER_ICON
    }

    fn get_scrollable_offset(&self) -> scrollable::RelativeOffset {
        self.scrollable_offset
    }
}
