//! What is running now: a badge on the cover of a game that is starting or
//! running, and a floating toolbar at the bottom of the library for each such
//! game, with a way to stop it. A launch's clipboard question is here too.

use iced::widget::{Column, Row, container, text};
use iced::{Alignment, Length};
use maltoria_core::model::Game;
use maltoria_core::runtime::{
    ClipboardDecision, ClipboardPrompt, RuntimeAction, RuntimeState, RuntimeTable,
};
use material::widget::{dialog, image, toolbar};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;
use crate::library;

fn state(runtime: &RuntimeTable, game: &Game) -> Option<RuntimeState> {
    runtime.entry(&game.id).map(|entry| entry.state)
}

/// The badge on the cover of a starting or running game.
pub fn badge<'a>(runtime: &RuntimeTable, game: &Game) -> Option<material::Element<'a, Message>> {
    let state = state(runtime, game).filter(|state| state.is_active())?;
    let icon = if state == RuntimeState::Running {
        "play_circle"
    } else {
        "hourglass_top"
    };

    Some(
        container(
            Row::new()
                .push(material::fonts::filled_icon(icon, 16.0))
                .push(material::text::type_scale(
                    state.phase_label(),
                    tokens::typography::LABEL_MEDIUM,
                ))
                .spacing(4)
                .align_y(Alignment::Center),
        )
        .padding([4, 10])
        .style(|theme: &Theme| {
            let colors = theme.colors();

            container::Style {
                background: Some(colors.primary.color.into()),
                text_color: Some(colors.primary.text),
                border: iced::border::rounded(tokens::shape::CORNER_FULL),
                ..container::Style::default()
            }
        })
        .into(),
    )
}

/// A floating toolbar for each starting or running game, or nothing.
pub fn bars<'a>(
    games: &'a [Game],
    runtime: &'a RuntimeTable,
    covers: &'a image::Cache<String>,
) -> Option<material::Element<'a, Message>> {
    let bars: Vec<material::Element<'a, Message>> = games
        .iter()
        .filter(|game| state(runtime, game).is_some_and(RuntimeState::is_active))
        .map(|game| bar(game, runtime, covers))
        .collect();

    (!bars.is_empty()).then(|| Column::with_children(bars).spacing(12).into())
}

const THUMBNAIL: (f32, f32) = (30.0, 40.0);

fn bar<'a>(
    game: &'a Game,
    runtime: &'a RuntimeTable,
    covers: &'a image::Cache<String>,
) -> material::Element<'a, Message> {
    let thumbnail: material::Element<'a, Message> = match covers.handle(&game.id) {
        Some(handle) => image::shaped(handle.clone(), tokens::shape::CORNER_EXTRA_SMALL).into(),
        None => image::placeholder(
            image::Placeholder::Missing,
            tokens::shape::CORNER_EXTRA_SMALL,
        )
        .into(),
    };

    let title = text(game.title.as_str())
        .size(tokens::typography::LABEL_LARGE.size)
        .wrapping(text::Wrapping::None);

    let primary = library::primary_action(runtime, game);
    let mut actions = vec![
        container(thumbnail)
            .width(Length::Fixed(THUMBNAIL.0))
            .height(Length::Fixed(THUMBNAIL.1))
            .into(),
        container(title).padding([0, 8]).into(),
    ];

    if let Some(message) = library::action_message(primary.action, game) {
        let icon = match primary.action {
            Some(RuntimeAction::Stop) => "stop",
            Some(RuntimeAction::Cancel) => "close",
            _ => "play_arrow",
        };

        actions.push(toolbar::vibrant_icon_action(icon, message));
    }

    toolbar::floating_vibrant(actions).into()
}

/// The size of what the clipboard holds, as the GPUI app writes it.
pub fn clipboard_size(bytes: i64) -> String {
    if bytes <= 0 {
        return maltoria_core::i18n::s().dialogs.unknown_size.to_owned();
    }

    if bytes < 1024 {
        return maltoria_core::i18n::s().common.bytes_human(bytes as u64);
    }

    format!("{:.1} KB", bytes as f64 / 1024.0)
}

/// The question the helper asks before a launch while the clipboard holds
/// something the game could read: clear it, keep it, or not launch.
pub fn clipboard_dialog<'a>(
    prompt: &ClipboardPrompt,
    alpha: f32,
) -> material::Element<'a, Message> {
    let strings = &maltoria_core::i18n::s().dialogs;
    let content = if prompt.content_type.is_empty() {
        strings.clipboard_unknown_content.to_owned()
    } else {
        prompt.content_type.clone()
    };
    let options = dialog::AlphaOptions::default().alpha(alpha);

    dialog::alert_with(
        strings.clipboard_title,
        strings.clipboard_prompt_body(&content, &clipboard_size(prompt.size_bytes)),
        dialog::actions([
            dialog::action_button_with(
                maltoria_core::i18n::s().common.cancel,
                Message::ClipboardDismiss,
                options,
            ),
            dialog::action_button_with(
                strings.clipboard_keep_launch,
                Message::ClipboardDecide(ClipboardDecision::Keep),
                options,
            ),
            dialog::action_button_with(
                strings.clipboard_clear_launch,
                Message::ClipboardDecide(ClipboardDecision::Clear),
                options,
            ),
        ]),
        dialog::AlertOptions::default()
            .icon("content_paste")
            .alpha(alpha),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_sizes_read_as_the_gpui_app_wrote_them() {
        let strings = maltoria_core::i18n::s();

        assert_eq!(clipboard_size(0), strings.dialogs.unknown_size);
        assert_eq!(clipboard_size(512), strings.common.bytes_human(512));
        assert_eq!(clipboard_size(2048), "2.0 KB");
    }
}
