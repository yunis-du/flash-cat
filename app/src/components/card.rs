use gpui::{AnyElement, App, ElementId, Fill, SharedString, StyleRefinement, Styled, Window, div, prelude::*, px};
use gpui_component::{ActiveTheme, Colorize, h_flex, label::Label, scroll::ScrollableElement};

#[derive(IntoElement)]
pub struct Card {
    /// Unique identifier for the element.
    id: ElementId,
    /// Main title text.
    title: Option<SharedString>,
    /// Optional footer element.
    footer: Option<AnyElement>,
    /// Custom background fill.
    bg: Option<Fill>,
    /// Scrollable overflow x container.
    overflow_x: bool,
    /// Scrollable overflow y container.
    overflow_y: bool,
    /// Child elements contained within the card.
    children: Vec<AnyElement>,
    /// Style refinements for the card.
    style: StyleRefinement,
}

#[allow(unused)]
impl Card {
    /// Creates a new `Card` with the given element ID.
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            title: None,
            footer: None,
            bg: None,
            overflow_x: false,
            overflow_y: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Sets the title text.
    /// Accepts any type that can be converted into a `SharedString`.
    pub fn title(
        mut self,
        title: impl Into<SharedString>,
    ) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets a custom footer element at the bottom of the card.
    pub fn footer(
        mut self,
        footer: impl IntoElement,
    ) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// Overrides the default background color/fill.
    pub fn bg(
        mut self,
        bg: impl Into<Fill>,
    ) -> Self {
        self.bg = Some(bg.into());
        self
    }

    /// Sets the overflow to scroll on both the x and y axes.
    pub fn overflow_scrollbar(mut self) -> Self {
        self.overflow_x = true;
        self.overflow_y = true;
        self
    }

    /// Sets the overflow to scroll on the x-axis.
    pub fn overflow_x_scrollbar(mut self) -> Self {
        self.overflow_x = true;
        self
    }

    /// Sets the overflow to scroll on the y-axis.
    pub fn overflow_y_scrollbar(mut self) -> Self {
        self.overflow_y = true;
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(
        &mut self,
        elements: impl IntoIterator<Item = AnyElement>,
    ) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Card {
    fn render(
        self,
        _window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        // Construct the header row: Icon + Title + Spacer + Actions
        let header = h_flex().when_some(self.title, |this, title| {
            this.child(div().flex_1().overflow_hidden().child(Label::new(title).text_base().whitespace_nowrap().text_ellipsis()))
        });

        let bg = if self.bg.is_none() {
            if cx.theme().is_dark() {
                cx.theme().background.lighten(1.0)
            } else {
                cx.theme().background.darken(0.02)
            }
        } else {
            cx.theme().background
        };

        // Construct the main card container using a declarative style
        let mut d = div();
        *d.style() = self.style;
        let container = d
            .id(self.id)
            .border(px(1.))
            .border_color(cx.theme().border)
            .p_2()
            .rounded(cx.theme().radius)
            .bg(bg)
            // Add Header
            .child(header)
            .children(self.children)
            // Add Footer
            .when_some(self.footer, |this, footer| this.child(footer));

        if self.overflow_x && self.overflow_y {
            container.overflow_scrollbar().into_any_element()
        } else if self.overflow_x {
            container.overflow_x_scrollbar().into_any_element()
        } else if self.overflow_y {
            container.overflow_y_scrollbar().into_any_element()
        } else {
            container.into_any_element()
        }
    }
}
