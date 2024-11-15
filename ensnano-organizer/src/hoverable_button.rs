//! Allow widgets to emmit messages when hovered
//!
//! A [`HoverableContainer`] is an `iced_native::Container` that produces a messages when hovered.

use iced_native::alignment::{self, Alignment};
use iced_native::event::{self, Event};
use iced_native::layout;
use iced_native::mouse;
use iced_native::overlay;
use iced_native::renderer;
use iced_native::{
    Background, Clipboard, Color, Element, Layout, Length, Padding, Point, Rectangle, Shell, Widget,
};

use std::u32;

pub use iced_style::container::{Style, StyleSheet};

/// The local state of an [`HoverableContainer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    is_hovered: bool,
}

impl State {
    /// Creates a new [`State`].
    pub fn new() -> State {
        State::default()
    }
}

/// An `iced_native::Container` that emits a message when hovered.
#[allow(missing_debug_implementations)]
pub struct HoverableContainer<'a, Message: Clone, Renderer> {
    padding: Padding,
    width: Length,
    height: Length,
    max_width: u32,
    max_height: u32,
    horizontal_alignment: alignment::Horizontal,
    vertical_alignment: alignment::Vertical,
    style_sheet: Box<dyn StyleSheet + 'a>,
    content: Element<'a, Message, Renderer>,
    on_hovered_in: Option<Message>,
    on_hovered_out: Option<Message>,
    state: &'a mut State,
}

impl<'a, Message: Clone, Renderer> HoverableContainer<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    /// Creates an empty [Container](iced::widget::container::Container).
    pub fn new<T>(state: &'a mut State, content: T) -> Self
    where
        T: Into<Element<'a, Message, Renderer>>,
    {
        HoverableContainer {
            padding: Padding::ZERO,
            width: Length::Shrink,
            height: Length::Shrink,
            max_width: u32::MAX,
            max_height: u32::MAX,
            horizontal_alignment: alignment::Horizontal::Left,
            vertical_alignment: alignment::Vertical::Top,
            style_sheet: Default::default(),
            content: content.into(),
            on_hovered_in: None,
            on_hovered_out: None,
            state,
        }
    }

    /// Sets the [`Padding`] of the [Container](iced::widget::container::Container).
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the width of the [Container](iced::widget::container::Container).
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// Sets the height of the [Container](iced::widget::container::Container).
    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
        self
    }

    /// Sets the maximum width of the [Container](iced::widget::container::Container).
    pub fn max_width(mut self, max_width: u32) -> Self {
        self.max_width = max_width;
        self
    }

    /// Sets the maximum height of the [Container](iced::widget::container::Container) in pixels.
    pub fn max_height(mut self, max_height: u32) -> Self {
        self.max_height = max_height;
        self
    }

    /// Sets the content alignment for the horizontal axis of the [Container](iced::widget::container::Container).
    pub fn align_x(mut self, alignment: alignment::Horizontal) -> Self {
        self.horizontal_alignment = alignment;
        self
    }

    /// Sets the content alignment for the vertical axis of the [Container](iced::widget::container::Container).
    pub fn align_y(mut self, alignment: alignment::Vertical) -> Self {
        self.vertical_alignment = alignment;
        self
    }

    /// Centers the contents in the horizontal axis of the [Container](iced::widget::container::Container).
    pub fn center_x(mut self) -> Self {
        self.horizontal_alignment = alignment::Horizontal::Center;
        self
    }

    /// Centers the contents in the vertical axis of the [Container](iced::widget::container::Container).
    pub fn center_y(mut self) -> Self {
        self.vertical_alignment = alignment::Vertical::Center;
        self
    }

    /// Sets the style of the [Container](iced::widget::container::Container).
    pub fn style(mut self, style_sheet: impl Into<Box<dyn StyleSheet + 'a>>) -> Self {
        self.style_sheet = style_sheet.into();
        self
    }

    pub fn on_hovered_in(mut self, message: Message) -> Self {
        self.on_hovered_in = Some(message);
        self
    }

    pub fn on_hovered_out(mut self, message: Message) -> Self {
        self.on_hovered_out = Some(message);
        self
    }
}

/// Computes the layout of a [Container](iced::widget::container::Container).
pub fn layout<Renderer>(
    renderer: &Renderer,
    limits: &layout::Limits,
    width: Length,
    height: Length,
    padding: Padding,
    horizontal_alignment: alignment::Horizontal,
    vertical_alignment: alignment::Vertical,
    layout_content: impl FnOnce(&Renderer, &layout::Limits) -> layout::Node,
) -> layout::Node {
    let limits = limits.loose().width(width).height(height).pad(padding);

    let mut content = layout_content(renderer, &limits.loose());
    let size = limits.resolve(content.size());

    content.move_to(Point::new(padding.left.into(), padding.top.into()));
    content.align(
        Alignment::from(horizontal_alignment),
        Alignment::from(vertical_alignment),
        size,
    );

    layout::Node::with_children(size.pad(padding), vec![content])
}

impl<'a, Message: Clone, Renderer> Widget<Message, Renderer>
    for HoverableContainer<'a, Message, Renderer>
where
    Renderer: iced_native::Renderer,
{
    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
    }

    fn layout(&self, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        layout(
            renderer,
            limits,
            self.width,
            self.height,
            self.padding,
            self.horizontal_alignment,
            self.vertical_alignment,
            |renderer, limits| self.content.layout(renderer, limits),
        )
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor_position: Point,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        if let event::Status::Captured = self.content.on_event(
            event.clone(),
            layout.children().next().unwrap(),
            cursor_position,
            renderer,
            clipboard,
            shell,
        ) {
            event::Status::Captured
        } else {
            if let Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
                let bounds = layout.bounds();
                if bounds.contains(cursor_position) {
                    if !self.state.is_hovered {
                        if let Some(on_hovered_in) = self.on_hovered_in.clone() {
                            shell.publish(on_hovered_in)
                        }
                        self.state.is_hovered = true;
                    }
                } else {
                    if self.state.is_hovered {
                        if let Some(on_hovered_out) = self.on_hovered_out.clone() {
                            shell.publish(on_hovered_out)
                        }
                        self.state.is_hovered = false;
                    }
                }
            }
            event::Status::Ignored
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor_position: Point,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.mouse_interaction(
            layout.children().next().unwrap(),
            cursor_position,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor_position: Point,
        viewport: &Rectangle,
    ) {
        let style = self.style_sheet.style();

        draw_background(renderer, &style, layout.bounds());

        self.content.draw(
            renderer,
            &renderer::Style {
                text_color: style.text_color.unwrap_or(renderer_style.text_color),
            },
            layout.children().next().unwrap(),
            cursor_position,
            viewport,
        );
    }

    fn overlay(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'_, Message, Renderer>> {
        self.content
            .overlay(layout.children().next().unwrap(), renderer)
    }
}

/// Draws the background of a
/// [Container](iced::widget::container::Container) given its [Style] and its `bounds`.
pub fn draw_background<Renderer>(renderer: &mut Renderer, style: &Style, bounds: Rectangle)
where
    Renderer: iced_native::Renderer,
{
    if style.background.is_some() || style.border_width > 0.0 {
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border_radius: style.border_radius,
                border_width: style.border_width,
                border_color: style.border_color,
            },
            style
                .background
                .unwrap_or(Background::Color(Color::TRANSPARENT)),
        );
    }
}

impl<'a, Message: Clone, Renderer> From<HoverableContainer<'a, Message, Renderer>>
    for Element<'a, Message, Renderer>
where
    Renderer: 'a + iced_native::Renderer,
    Message: 'a,
{
    fn from(column: HoverableContainer<'a, Message, Renderer>) -> Element<'a, Message, Renderer> {
        Element::new(column)
    }
}
