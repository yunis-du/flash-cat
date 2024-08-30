use iced::{
    widget::scrollable::{self, Id, RelativeOffset},
    Command, Element,
};
use receiver_tab::{Message as ReceiverMessage, ReceiverTab};
use sender_tab::{Message as SenderMessage, SenderTab};
use settings_tab::{Message as SettingsMessage, SettingsTab};

pub mod receiver_tab;
pub mod sender_tab;
pub mod settings_tab;

pub trait Tab {
    type Message;

    fn title() -> &'static str;

    fn icon_bytes() -> &'static [u8];

    fn tab_label() -> TabLabel {
        TabLabel::new(Self::title(), Self::icon_bytes())
    }

    fn get_scrollable_offset(&self) -> RelativeOffset;

    fn set_scrollable_offset(scrollable_offset: RelativeOffset) -> Command<Self::Message>
    where
        Self::Message: 'static,
    {
        scrollable::snap_to(Self::scrollable_id(), scrollable_offset)
    }

    fn scrollable_id() -> Id {
        Id::new(format!("{}-scrollable", Self::title()))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TabId {
    Sender,
    Receiver,
    Settings,
}

impl From<usize> for TabId {
    fn from(value: usize) -> Self {
        match value {
            0 => Self::Sender,
            1 => Self::Receiver,
            2 => Self::Settings,
            _ => unreachable!("no more tabs"),
        }
    }
}

impl From<TabId> for usize {
    fn from(val: TabId) -> Self {
        match val {
            TabId::Sender => 0,
            TabId::Receiver => 1,
            TabId::Settings => 2,
        }
    }
}

impl std::fmt::Display for TabId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            TabId::Sender => "Sender",
            TabId::Receiver => "Receiver",
            TabId::Settings => "Settings",
        };

        write!(f, "{text}")
    }
}

pub struct TabLabel {
    pub text: &'static str,
    pub icon: &'static [u8],
}

impl TabLabel {
    pub fn new(text: &'static str, icon: &'static [u8]) -> Self {
        Self { text, icon }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Sender(SenderMessage),
    Receiver(ReceiverMessage),
    Settings(SettingsMessage),
}

pub struct TabsController {
    current_tab: TabId,
    sender_tab: SenderTab,
    receiver_tab: ReceiverTab,
    settings_tab: SettingsTab,
    tabs_scrollable_offsets: [RelativeOffset; 3],
}

impl TabsController {
    pub fn new() -> (Self, Command<Message>) {
        let (sender_tab, sender_command) = SenderTab::new();
        let (receiver_tab, receiver_command) = ReceiverTab::new();
        let (settings_tab, settings_command) = SettingsTab::new();

        (
            Self {
                current_tab: TabId::Sender,
                sender_tab,
                receiver_tab,
                settings_tab,
                tabs_scrollable_offsets: [RelativeOffset::START; 3],
            },
            Command::batch([
                sender_command.map(Message::Sender),
                receiver_command.map(Message::Receiver),
                settings_command.map(Message::Settings),
            ]),
        )
    }

    fn record_scrollable_offset(&mut self, index: usize, scrollable_offset: RelativeOffset) {
        self.tabs_scrollable_offsets[index] = scrollable_offset;
    }

    fn record_tabs_scrollable_offsets(&mut self) {
        let index: usize = self.current_tab.into();

        match self.current_tab {
            TabId::Sender => {
                self.record_scrollable_offset(index, self.sender_tab.get_scrollable_offset())
            }
            TabId::Receiver => {
                self.record_scrollable_offset(index, self.receiver_tab.get_scrollable_offset())
            }
            TabId::Settings => {
                self.record_scrollable_offset(index, self.settings_tab.get_scrollable_offset())
            }
        }
    }

    pub fn update_scrollables_offsets(&mut self) -> Command<Message> {
        self.record_tabs_scrollable_offsets();
        self.restore_scrollable_offset()
    }

    fn restore_scrollable_offset(&mut self) -> Command<Message> {
        let index: usize = self.current_tab.into();

        match self.current_tab {
            TabId::Sender => SenderTab::set_scrollable_offset(self.tabs_scrollable_offsets[index])
                .map(Message::Sender),
            TabId::Receiver => {
                ReceiverTab::set_scrollable_offset(self.tabs_scrollable_offsets[index])
                    .map(Message::Receiver)
            }
            TabId::Settings => {
                SettingsTab::set_scrollable_offset(self.tabs_scrollable_offsets[index])
                    .map(Message::Settings)
            }
        }
    }

    pub fn switch_to_tab(&mut self, tab: TabId) -> Command<Message> {
        self.record_tabs_scrollable_offsets();

        self.current_tab = tab;

        let tab_command = match tab {
            TabId::Sender => Command::none(),
            TabId::Receiver => Command::none(),
            TabId::Settings => Command::none(),
        };

        Command::batch([self.restore_scrollable_offset(), tab_command])
    }

    pub fn subscription(&self) -> iced::Subscription<Message> {
        iced::Subscription::batch([self.sender_tab.subscription().map(Message::Sender)])
    }

    pub fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::Sender(message) => self.sender_tab.update(message).map(Message::Sender),
            Message::Receiver(message) => self.receiver_tab.update(message).map(Message::Receiver),
            Message::Settings(message) => self.settings_tab.update(message).map(Message::Settings),
        }
    }

    pub fn get_labels(&self) -> [TabLabel; 3] {
        [
            SenderTab::tab_label(),
            ReceiverTab::tab_label(),
            SettingsTab::tab_label(),
        ]
    }

    pub fn view(&self) -> Element<'_, Message> {
        match self.current_tab {
            TabId::Sender => self.sender_tab.view().map(Message::Sender),
            TabId::Receiver => self.receiver_tab.view().map(Message::Receiver),
            TabId::Settings => self.settings_tab.view().map(Message::Settings),
        }
    }
}
