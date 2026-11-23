//! Where the last press landed, in window coordinates, for animations that
//! grow from the control that started them, such as the theme's reveal.
//!
//! The tracker wraps the whole window, where the cursor is not translated by
//! any scrolling, and records presses without producing messages.

use std::sync::Mutex;

use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse, touch};

static LAST_PRESS: Mutex<Option<Point>> = Mutex::new(None);

/// Where the last press landed.
pub fn last_press() -> Option<Point> {
    LAST_PRESS.lock().ok().and_then(|point| *point)
}

fn record(point: Point) {
    if let Ok(mut last) = LAST_PRESS.lock() {
        *last = Some(point);
    }
}

/// `content`, with its presses recorded for [`last_press`].
pub fn track<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    Element::new(Tracker {
        content: content.into(),
    })
}

struct Tracker<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Tracker<'_, Message, Theme, Renderer>
where
    Renderer: iced::advanced::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_)) => {
                if let Some(position) = cursor.position() {
                    record(position);
                }
            }
            Event::Touch(touch::Event::FingerPressed { position, .. }) => record(*position),
            _ => {}
        }

        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }
}
