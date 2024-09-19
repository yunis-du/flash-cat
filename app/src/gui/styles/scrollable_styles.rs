use iced::widget::scrollable::{Direction, Scrollbar};

pub fn vertical_direction() -> Direction {
    Direction::Vertical(Scrollbar::new().width(5).scroller_width(5))
}
