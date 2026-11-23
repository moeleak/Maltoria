//! What is running now: a badge on the cover of a game that is starting or
//! running. A launch's clipboard question is here too.

use iced::Alignment;
use iced::widget::{Row, container};
use maltoria_core::model::Game;
use maltoria_core::runtime::{ClipboardDecision, ClipboardPrompt, RuntimeState, RuntimeTable};
use material::widget::dialog;
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;

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
            dialog::action_with_icon("close", maltoria_core::i18n::s().common.cancel, options)
                .on_press(Message::ClipboardDismiss)
                .into(),
            dialog::action_with_icon("content_paste", strings.clipboard_keep_launch, options)
                .on_press(Message::ClipboardDecide(ClipboardDecision::Keep))
                .into(),
            dialog::action_with_icon("content_paste_off", strings.clipboard_clear_launch, options)
                .on_press(Message::ClipboardDecide(ClipboardDecision::Clear))
                .into(),
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
