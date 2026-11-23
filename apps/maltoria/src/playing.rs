//! What is running now: a badge on the cover of a game that is starting or
//! running, and a floating toolbar at the bottom of the library for each such
//! game, with how long it has run and a way to stop it.

use std::collections::HashMap;
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc;
use iced::futures::{StreamExt, stream};
use iced::time::Instant;
use iced::widget::{Column, Row, container, text};
use iced::{Alignment, Length};
use maltoria_core::i18n::fmt;
use maltoria_core::model::Game;
use maltoria_core::runtime::{RuntimeAction, RuntimeState, RuntimeTable};
use material::widget::{image, toolbar};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;
use crate::library;

/// How often the elapsed times refresh.
const TICK: Duration = Duration::from_secs(30);

/// When each running game started, as the helper's states showed it.
#[derive(Debug, Default)]
pub struct Sessions {
    since: HashMap<String, Instant>,
}

impl Sessions {
    /// Follows `runtime`: games that became active start a session, and
    /// games that went idle end theirs.
    pub fn update(&mut self, games: &[Game], runtime: &RuntimeTable, now: Instant) {
        let active: Vec<&str> = games
            .iter()
            .filter(|game| state(runtime, game).is_some_and(RuntimeState::is_active))
            .map(|game| game.id.as_str())
            .collect();

        self.since.retain(|id, _| active.contains(&id.as_str()));

        for id in active {
            self.since.entry(id.to_owned()).or_insert(now);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.since.is_empty()
    }

    /// When the session of the game `id` started.
    pub fn since(&self, id: &str) -> Option<Instant> {
        self.since.get(id).copied()
    }
}

fn state(runtime: &RuntimeTable, game: &Game) -> Option<RuntimeState> {
    runtime.entry(&game.id).map(|entry| entry.state)
}

/// A redraw every half minute while games run, for their elapsed times.
pub fn ticker() -> Subscription<Message> {
    Subscription::run(|| {
        let (sender, receiver) = mpsc::unbounded();

        std::thread::spawn(move || {
            while sender.unbounded_send(()).is_ok() {
                std::thread::sleep(TICK);
            }
        });

        receiver.map(|()| Message::Tick).chain(stream::pending())
    })
}

/// What a running game is doing, and for how long it has.
pub fn status(state: RuntimeState, elapsed: Duration) -> String {
    let phase = state.phase_label();

    if state == RuntimeState::Running {
        format!(
            "{phase} · {}",
            fmt::statistics_duration(elapsed.as_secs() / 60)
        )
    } else {
        phase.to_owned()
    }
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
    sessions: &'a Sessions,
    covers: &'a image::Cache<String>,
) -> Option<material::Element<'a, Message>> {
    let now = Instant::now();
    let bars: Vec<material::Element<'a, Message>> = games
        .iter()
        .filter_map(|game| {
            let state = state(runtime, game).filter(|state| state.is_active())?;
            let elapsed = sessions
                .since(&game.id)
                .map_or(Duration::ZERO, |since| now.saturating_duration_since(since));

            Some(bar(game, state, elapsed, runtime, covers))
        })
        .collect();

    (!bars.is_empty()).then(|| Column::with_children(bars).spacing(12).into())
}

const THUMBNAIL: (f32, f32) = (30.0, 40.0);

fn bar<'a>(
    game: &'a Game,
    state: RuntimeState,
    elapsed: Duration,
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

    let info = Column::new()
        .push(
            text(game.title.as_str())
                .size(tokens::typography::LABEL_LARGE.size)
                .wrapping(text::Wrapping::None),
        )
        .push(text(status(state, elapsed)).size(tokens::typography::BODY_SMALL.size))
        .spacing(2)
        .width(Length::Shrink);

    let primary = library::primary_action(runtime, game);
    let mut actions = vec![
        container(thumbnail)
            .width(Length::Fixed(THUMBNAIL.0))
            .height(Length::Fixed(THUMBNAIL.1))
            .into(),
        container(info).padding([0, 8]).into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_games_show_how_long_they_ran() {
        let running = status(RuntimeState::Running, Duration::from_secs(12 * 60 + 30));
        assert!(running.starts_with(RuntimeState::Running.phase_label()));
        assert!(running.ends_with(&fmt::statistics_duration(12)));

        assert_eq!(
            status(RuntimeState::Launching, Duration::from_secs(90)),
            RuntimeState::Launching.phase_label()
        );
    }

    fn update(state: &str) -> maltoria_core::runtime::RuntimeUpdate {
        let entry = serde_json::from_value(serde_json::json!({
            "id": "wa2", "state": state, "capabilities": 7
        }))
        .unwrap();

        maltoria_core::runtime::RuntimeUpdate::resync("/bottle", vec![entry])
    }

    #[test]
    fn sessions_follow_the_runtime() {
        let mut game = maltoria_core::model::demo_data()[0].clone();
        game.id = "wa2".into();
        let games = [game];
        let mut runtime = RuntimeTable::default();
        let mut sessions = Sessions::default();
        let start = Instant::now();

        sessions.update(&games, &runtime, start);
        assert!(sessions.is_empty());

        runtime.apply(update("running"));
        sessions.update(&games, &runtime, start);
        // A later update keeps when the session started.
        sessions.update(&games, &runtime, start + Duration::from_secs(60));
        assert_eq!(sessions.since("wa2"), Some(start));

        runtime.apply(update("idle"));
        sessions.update(&games, &runtime, start + Duration::from_secs(120));
        assert!(sessions.is_empty());
    }
}
