//! The library: every game in the container as a grid of covers, with a
//! search and a few filters; a cover opens the game's details.
//!
//! Covers load as they come into view and are kept for the frames after; a
//! cover's context menu holds what a game's details offer.

use std::cmp::Ordering;

use iced::time::{Duration, Instant};
use iced::widget::{Column, Row, Space, Stack, container, mouse_area, scrollable, sensor, text};
use iced::{Alignment, Length, Padding};
use maltoria_core::i18n::s;
use maltoria_core::library_view::{matches_query, natural_cmp};
use maltoria_core::model::{Game, format_play_time};
use maltoria_core::program_settings::{ProgramSettingsDto, locale_label};
use maltoria_core::runtime::{PrimaryAction, RuntimeAction, RuntimeTable, primary_action_for};
use material::widget::button::{self, ButtonVariant, ChipVariant, IconButtonVariant};
use material::widget::{grid, image, menu, progress_bar, search, select};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;
use crate::playing;

/// The width of a cover. Covers keep it as the window resizes, which only
/// changes how many fit in a row.
pub const COVER_WIDTH: f32 = 168.0;
/// Covers are 3:4, the shape of most visual novel packages; others are cropped.
pub const COVER_RATIO: f32 = 3.0 / 4.0;
/// The longest side covers are decoded to: twice the widest cover, for Retina.
pub const COVER_DECODE_SIZE: u32 = 720;
/// The longest side portraits and thumbnails are decoded to.
pub const PICTURE_DECODE_SIZE: u32 = 512;
/// The covers kept decoded; more than a screen of them.
pub const COVER_CACHE_SIZE: usize = 240;

/// The locales a game can run in, the container's own first.
pub const LOCALES: [&str; 5] = [
    "",
    "ja_JP.UTF-8",
    "zh_CN.UTF-8",
    "zh_TW.UTF-8",
    "en_US.UTF-8",
];

/// Which games the grid shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    /// Played at least once.
    Played,
    /// Never played.
    Unplayed,
    /// Hidden from the others.
    Hidden,
}

impl Filter {
    pub const ALL: [Self; 4] = [Self::All, Self::Played, Self::Unplayed, Self::Hidden];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => s().library.all_games,
            Self::Played => s().app.filter_played,
            Self::Unplayed => s().app.filter_unplayed,
            Self::Hidden => s().app.filter_hidden,
        }
    }

    pub fn matches(self, game: &Game) -> bool {
        match self {
            Self::All => !game.hidden,
            Self::Played => !game.hidden && is_played(game),
            Self::Unplayed => !game.hidden && !is_played(game),
            Self::Hidden => game.hidden,
        }
    }
}

/// The order of the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// The most recently finished session first.
    #[default]
    Recent,
    Name,
    /// The most played first.
    PlayTime,
}

impl Sort {
    pub const ALL: [Self; 3] = [Self::Recent, Self::Name, Self::PlayTime];
}

impl std::fmt::Display for Sort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Recent => s().library.sort_recent,
            Self::Name => s().library.sort_name,
            Self::PlayTime => s().app.sort_play_time,
        })
    }
}

fn is_played(game: &Game) -> bool {
    game.play_time.is_some_and(|seconds| seconds > 0) || game.last_played.is_some()
}

/// The indices of the `games` the grid shows, in its order: those matching
/// `query` and `filter`, launchers after the games.
pub fn visible(games: &[Game], query: &str, filter: Filter, sort: Sort) -> Vec<usize> {
    let mut indices: Vec<usize> = games
        .iter()
        .enumerate()
        .filter(|(_, game)| filter.matches(game) && matches_query(game, query))
        .map(|(index, _)| index)
        .collect();

    indices.sort_by(|&a, &b| {
        let (first, second) = (&games[a], &games[b]);

        first
            .is_launcher
            .cmp(&second.is_launcher)
            .then_with(|| order(sort, first, second))
            .then_with(|| natural_cmp(&first.title, &second.title))
            .then_with(|| first.id.cmp(&second.id))
    });

    indices
}

fn order(sort: Sort, first: &Game, second: &Game) -> Ordering {
    match sort {
        // Dated sessions first, the newest first, as the Swift library does.
        Sort::Recent => match (first.last_session_ended_at, second.last_session_ended_at) {
            (Some(a), Some(b)) => b.cmp(&a),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        },
        Sort::Name => Ordering::Equal,
        Sort::PlayTime => second
            .play_time
            .unwrap_or(0)
            .cmp(&first.play_time.unwrap_or(0)),
    }
}

/// What the library holds besides its games.
#[derive(Debug)]
pub struct State {
    pub query: String,
    pub filter: Filter,
    pub sort: Sort,
    /// The game last clicked, by id, which the grid highlights.
    pub selected: Option<String>,
    /// The run settings of the game whose details are open, once the helper
    /// sent them.
    pub settings: Option<ProgramSettingsDto>,
    pub covers: image::Cache<String>,
    /// Portraits and search thumbnails, by file path: trimmed of their plain
    /// background and cropped from the top, so that a figure fills its cell
    /// with its face in view.
    pub pictures: image::Cache<String>,
    /// The refresh indicator over the library, while a metadata sync runs
    /// and as it goes away.
    pub syncing: Option<Syncing>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            filter: Filter::default(),
            sort: Sort::default(),
            selected: None,
            settings: None,
            syncing: None,
            covers: image::Cache::new(COVER_CACHE_SIZE)
                .aspect_ratio(COVER_RATIO)
                .max_dimension(COVER_DECODE_SIZE),
            pictures: image::Cache::new(COVER_CACHE_SIZE)
                .trim_borders()
                .aspect_ratio(COVER_RATIO)
                .crop_from_top()
                .max_dimension(PICTURE_DECODE_SIZE),
        }
    }
}

/// What a game's primary button does now; "Launch Game" while it is idle,
/// and before the helper first reports it.
pub fn primary_action(runtime: &RuntimeTable, game: &Game) -> PrimaryAction {
    let primary = match runtime.entry(&game.id) {
        Some(entry) => primary_action_for(entry.state, entry.capabilities),
        None => PrimaryAction {
            action: Some(RuntimeAction::Launch),
            enabled: true,
            ..PrimaryAction::unavailable()
        },
    };

    if primary.action == Some(RuntimeAction::Launch) {
        PrimaryAction {
            label: s().library.launch_game,
            ..primary
        }
    } else {
        primary
    }
}

/// The icon of a primary action: play to launch, stop, or cancel.
pub fn action_icon(primary: &PrimaryAction) -> &'static str {
    match primary.action {
        Some(RuntimeAction::Launch) => "play_arrow",
        Some(RuntimeAction::Stop) => "stop",
        Some(RuntimeAction::Cancel) => "close",
        None if primary.busy => "hourglass_top",
        None => "play_arrow",
    }
}

/// The message for a primary action on `game`.
pub fn action_message(action: Option<RuntimeAction>, game: &Game) -> Option<Message> {
    let id = game.id.clone();

    action.map(|action| match action {
        RuntimeAction::Launch => Message::Play(id),
        RuntimeAction::Cancel => Message::Cancel(id),
        RuntimeAction::Stop => Message::Stop(id),
    })
}

/// How long the refresh indicator grows in: SwipeRefreshLayout's
/// `config_mediumAnimTime`.
const SYNC_SCALE_UP: Duration = Duration::from_millis(400);
/// How long it shrinks away: SwipeRefreshLayout's `SCALE_DOWN_DURATION`.
const SYNC_SCALE_DOWN: Duration = Duration::from_millis(150);
/// The gap between the indicator and the top of the games.
const SYNC_INDICATOR_TOP: f32 = 16.0;

/// Android's refresh indicator while metadata syncs: it grows in at the top
/// of the games, spins until the sync ends, and shrinks away.
#[derive(Debug, Clone)]
pub struct Syncing {
    animation: progress_bar::IndeterminateState,
    running: bool,
    /// The scale the current growing or shrinking started from, and when.
    from: f32,
    since: Instant,
    scale: f32,
}

impl Syncing {
    pub fn new(now: Instant) -> Self {
        Self {
            animation: progress_bar::IndeterminateState::new(now),
            running: true,
            from: 0.0,
            since: now,
            scale: 0.0,
        }
    }

    /// A sync started again, perhaps while the indicator was going away.
    pub fn start(&mut self, now: Instant) {
        if !self.running {
            self.running = true;
            self.from = self.scale;
            self.since = now;
        }
    }

    /// The sync ended: the indicator shrinks away from where it is.
    pub fn finish(&mut self, now: Instant) {
        if self.running {
            self.running = false;
            self.from = self.scale;
            self.since = now;
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Moves the indicator to `now`; false once it is gone.
    pub fn advance(&mut self, now: Instant) -> bool {
        self.animation.advance(now);

        let (target, duration) = if self.running {
            (1.0, SYNC_SCALE_UP)
        } else {
            (0.0, SYNC_SCALE_DOWN)
        };
        let progress = (now.saturating_duration_since(self.since).as_secs_f32()
            / duration.as_secs_f32())
        .min(1.0);
        // Android's DecelerateInterpolator.
        let eased = 1.0 - (1.0 - progress).powi(2);
        self.scale = self.from + (target - self.from) * eased;

        self.running || progress < 1.0
    }
}

/// The refresh indicator at its current size, centred at the top.
fn sync_indicator<'a>(syncing: &Syncing) -> material::Element<'a, Message> {
    let full = tokens::component::loading_indicator::CONTAINER_WIDTH;
    let side = full * syncing.scale;

    let indicator: material::Element<'a, Message> = if side < 1.0 {
        Space::new().into()
    } else {
        progress_bar::loading(progress_bar::LoadingIndicatorMode::contained_indeterminate(
            syncing.animation.loading_phase(),
        ))
        .width(Length::Fixed(side))
        .height(Length::Fixed(side))
        .into()
    };

    container(indicator)
        .width(Length::Fill)
        .height(Length::Fixed(full))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .padding(Padding::ZERO.top(SYNC_INDICATOR_TOP))
        .into()
}

pub fn view<'a>(
    games: &'a [Game],
    state: &'a State,
    runtime: &'a RuntimeTable,
) -> material::Element<'a, Message> {
    let indices = visible(games, &state.query, state.filter, state.sort);

    let toolbar = Row::new()
        .push(
            search::bar(s().library.search_placeholder, &state.query, Message::Query)
                .width(Length::Fill),
        )
        .push(
            select::outlined(Sort::ALL, Some(state.sort), Message::Sort)
                .width(Length::Fixed(160.0)),
        )
        .push(
            button::icon_button("sync", IconButtonVariant::Standard).on_press_maybe(
                state
                    .syncing
                    .as_ref()
                    .is_none_or(|syncing| !syncing.is_running())
                    .then_some(Message::SyncMetadata),
            ),
        )
        .push(
            button::extended_fab_with_icon("add", s().app.add_game, button::FabVariant::Primary)
                .on_press(Message::AddProgram),
        )
        .spacing(16)
        .align_y(Alignment::Center);

    let filters = Filter::ALL
        .iter()
        .fold(Row::new().spacing(8), |row, &filter| {
            let variant = if filter == state.filter {
                ChipVariant::SelectedFilter
            } else {
                ChipVariant::Filter
            };

            row.push(button::chip(filter.label(), variant).on_press(Message::Filter(filter)))
        });

    let body: material::Element<'a, Message> = if games.is_empty() {
        empty(
            s().library.library_empty_title,
            s().library.library_empty_desc,
            true,
        )
    } else if indices.is_empty() {
        empty(
            s().library.search_empty_title,
            s().library.filter_empty_desc,
            false,
        )
    } else {
        grid::fixed(
            COVER_WIDTH,
            indices
                .iter()
                .map(|&index| cell(&games[index], state, runtime)),
        )
        .into()
    };

    // The search and filters stay put; the games scroll beneath them, with
    // the refresh indicator over their top.
    let header = Column::new()
        .push(toolbar)
        .push(filters)
        .spacing(20)
        .padding(Padding::new(24.0).bottom(0.0));

    let games = scrollable(
        container(body)
            .width(Length::Fill)
            .padding(Padding::new(24.0).top(20.0)),
    )
    .height(Length::Fill)
    .width(Length::Fill);

    let games: material::Element<'a, Message> = match &state.syncing {
        Some(syncing) => Stack::new()
            .push(games)
            .push(sync_indicator(syncing))
            .into(),
        None => games.into(),
    };

    Column::new()
        .push(header)
        .push(games)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// `game`'s cover at the library's ratio, loaded when it comes into view.
pub fn cover_art<'a>(
    game: &'a Game,
    covers: &'a image::Cache<String>,
    corner: f32,
) -> material::Element<'a, Message> {
    let art: material::Element<'a, Message> = if game.art.is_some() {
        image::cached(covers, &game.id, corner)
    } else {
        image::placeholder(image::Placeholder::Missing, corner).into()
    };

    let id = game.id.clone();

    sensor(grid::aspect_ratio(art, COVER_RATIO))
        .key(cover_key(game, covers))
        .on_show(move |_| Message::CoverShown(id.clone()))
        .into()
}

/// What a cover's sensor watches: a new file, or a cover the cache let go
/// of, asks for the cover again.
pub fn cover_key(game: &Game, covers: &image::Cache<String>) -> (Option<std::path::PathBuf>, bool) {
    (game.art.clone(), covers.status(&game.id).is_some())
}

fn cell<'a>(
    game: &'a Game,
    state: &'a State,
    runtime: &'a RuntimeTable,
) -> material::Element<'a, Message> {
    let selected = state.selected.as_deref() == Some(game.id.as_str());
    let caption = caption(game);

    let badge: material::Element<'a, Message> = playing::badge(runtime, game).map_or_else(
        || Space::new().into(),
        |badge| container(badge).padding(8).into(),
    );

    let body = Column::new()
        .push(
            Stack::new()
                .push(cover_art(game, &state.covers, tokens::shape::CORNER_MEDIUM))
                .push(badge),
        )
        .push(
            material::text::type_scale(game.title.as_str(), tokens::typography::TITLE_SMALL)
                .wrapping(text::Wrapping::WordOrGlyph),
        )
        .push(
            material::text::type_scale(caption, tokens::typography::BODY_SMALL)
                .style(material::text::surface_variant),
        )
        .spacing(6);

    let body = container(body).padding(6).style(move |theme: &Theme| {
        let colors = theme.colors();

        container::Style {
            background: selected.then(|| colors.secondary.container.into()),
            border: iced::border::rounded(tokens::shape::CORNER_LARGE),
            ..container::Style::default()
        }
    });

    let id = game.id.clone();
    let body = mouse_area(body)
        .on_press(Message::OpenDetails(id.clone()))
        .on_double_click(Message::Play(id.clone()))
        .interaction(iced::mouse::Interaction::Pointer);

    let primary = primary_action(runtime, game);
    let primary_icon = action_icon(&primary);

    menu::context(body)
        .on_open(Message::Select(id.clone()))
        .item(
            menu::item(primary.label)
                .icon(primary_icon)
                .on_press_maybe(action_message(primary.action, game)),
        )
        .item(
            menu::item(s().library.game_details)
                .icon("info")
                .on_press(Message::OpenDetails(id.clone())),
        )
        .item(
            menu::item(s().library.game_info_cover_ellipsis)
                .icon("image_search")
                .on_press(Message::OpenEditor(id.clone())),
        )
        .item(
            menu::item(reveal_label())
                .icon("folder_open")
                .on_press_maybe(reveal_path(game).map(|_| Message::Reveal(id.clone()))),
        )
        .divider()
        .item(if game.hidden {
            menu::item(s().library.unhide_game)
                .icon("visibility")
                .on_press(Message::SetHidden(id.clone(), false))
        } else {
            menu::item(s().library.hide_game)
                .icon("visibility_off")
                .destructive()
                .on_press(Message::SetHidden(id.clone(), true))
        })
        .item(
            menu::item(s().app.remove_game)
                .icon("delete")
                .destructive()
                .on_press_maybe(crate::removal::removable(game).then_some(Message::Remove(id))),
        )
        .into()
}

/// The line under a cover: its play time; the badge on the cover tells what
/// a running game is doing.
fn caption(game: &Game) -> String {
    format_play_time(game.play_time).unwrap_or_else(|| s().app.not_played_yet.to_owned())
}

/// "Reveal in Finder" on macOS, "Show in File Manager" elsewhere.
fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        s().platform.reveal_mac
    } else {
        s().platform.reveal_other
    }
}

/// The folder to show for `game`: its installation, or else its executable's.
pub fn reveal_path(game: &Game) -> Option<String> {
    game.game_directory
        .as_ref()
        .or(game.folder_path.as_ref())
        .or(game.executable.as_ref())
        .map(|path| path.to_string_lossy().into_owned())
}

fn empty<'a>(
    title: &'a str,
    description: &'a str,
    offer_add: bool,
) -> material::Element<'a, Message> {
    let mut column = Column::new()
        .push(
            material::fonts::icon("stadia_controller", 48.0).style(material::text::surface_variant),
        )
        .push(material::text::type_scale(
            title,
            tokens::typography::TITLE_LARGE,
        ))
        .push(
            material::text::type_scale(description, tokens::typography::BODY_MEDIUM)
                .style(material::text::surface_variant)
                .align_x(iced::alignment::Horizontal::Center),
        )
        .spacing(12)
        .max_width(420)
        .align_x(Alignment::Center);

    if offer_add {
        column = column.push(
            button::button(s().app.add_game, ButtonVariant::Filled).on_press(Message::AddProgram),
        );
    }

    container(column)
        .width(Length::Fill)
        .padding(48)
        .center_x(Length::Fill)
        .into()
}

/// The executable whose run settings apply to `game`.
pub fn executable(game: &Game) -> Option<String> {
    game.last_executable
        .clone()
        .or_else(|| {
            game.executable
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .filter(|path| path.to_lowercase().ends_with(".exe"))
}

/// A locale in the details' select, shown by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleOption(pub &'static str);

impl LocaleOption {
    /// The option for `locale`, or the container's own for one not listed.
    pub fn of(locale: &str) -> Self {
        Self(
            LOCALES
                .iter()
                .find(|known| **known == locale)
                .copied()
                .unwrap_or(""),
        )
    }
}

impl std::fmt::Display for LocaleOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(locale_label(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(id: &str, title: &str) -> Game {
        let mut game = maltoria_core::model::demo_data()[0].clone();
        game.id = id.into();
        game.title = title.into();
        game.hidden = false;
        game.is_launcher = false;
        game.play_time = None;
        game.last_played = None;
        game.last_session_ended_at = None;
        game.tags.clear();
        game.studio = None;
        game
    }

    fn titles(games: &[Game], indices: &[usize]) -> Vec<String> {
        indices
            .iter()
            .map(|&index| games[index].title.clone())
            .collect()
    }

    #[test]
    fn filters_split_the_library_by_play_and_visibility() {
        let mut played = game("1", "Played");
        played.play_time = Some(60);
        let unplayed = game("2", "Unplayed");
        let mut hidden = game("3", "Hidden");
        hidden.hidden = true;
        let games = [played, unplayed, hidden];

        let shown = |filter| titles(&games, &visible(&games, "", filter, Sort::Name));

        assert_eq!(shown(Filter::All), ["Played", "Unplayed"]);
        assert_eq!(shown(Filter::Played), ["Played"]);
        assert_eq!(shown(Filter::Unplayed), ["Unplayed"]);
        assert_eq!(shown(Filter::Hidden), ["Hidden"]);
    }

    #[test]
    fn sorts_order_by_session_name_or_play_time() {
        let mut a = game("a", "Alpha");
        a.last_session_ended_at = Some(100);
        a.play_time = Some(10);
        let mut b = game("b", "beta 2");
        b.last_session_ended_at = Some(200);
        let mut c = game("c", "Beta 10");
        c.play_time = Some(500);
        let mut steam = game("s", "Aaa Launcher");
        steam.is_launcher = true;
        steam.play_time = Some(9_999);
        let games = [a, b, c, steam];

        let sorted = |sort| titles(&games, &visible(&games, "", Filter::All, sort));

        assert_eq!(
            sorted(Sort::Recent),
            ["beta 2", "Alpha", "Beta 10", "Aaa Launcher"]
        );
        // Natural order: 2 before 10, regardless of case.
        assert_eq!(
            sorted(Sort::Name),
            ["Alpha", "beta 2", "Beta 10", "Aaa Launcher"]
        );
        assert_eq!(
            sorted(Sort::PlayTime),
            ["Beta 10", "Alpha", "beta 2", "Aaa Launcher"]
        );
    }

    #[test]
    fn the_query_matches_titles_and_studios() {
        let mut white = game("1", "WHITE ALBUM2");
        white.studio = Some("Leaf".into());
        let other = game("2", "Other");
        let games = [white, other];

        assert_eq!(
            titles(&games, &visible(&games, "album", Filter::All, Sort::Name)),
            ["WHITE ALBUM2"]
        );
        assert_eq!(
            titles(&games, &visible(&games, "leaf", Filter::All, Sort::Name)),
            ["WHITE ALBUM2"]
        );
        assert!(visible(&games, "nothing", Filter::All, Sort::Name).is_empty());
    }

    #[test]
    fn settings_follow_the_executable_last_launched() {
        let mut game = game("1", "Game");
        game.executable = Some("/games/wa2/WA2.exe".into());
        assert_eq!(executable(&game).as_deref(), Some("/games/wa2/WA2.exe"));

        game.last_executable = Some("/games/wa2/WA2_chs.exe".into());
        assert_eq!(executable(&game).as_deref(), Some("/games/wa2/WA2_chs.exe"));

        // A folder entry without a chosen executable has none.
        game.last_executable = None;
        game.executable = Some("/games/folder".into());
        assert_eq!(executable(&game), None);
    }

    #[test]
    fn folders_open_at_the_installation() {
        let mut game = game("1", "Game");
        game.game_directory = None;
        game.folder_path = None;
        game.executable = Some("/games/wa2/WA2.exe".into());
        assert_eq!(reveal_path(&game).as_deref(), Some("/games/wa2/WA2.exe"));

        game.game_directory = Some("/games/wa2".into());
        assert_eq!(reveal_path(&game).as_deref(), Some("/games/wa2"));
    }

    #[test]
    fn the_refresh_indicator_grows_in_and_shrinks_away() {
        let start = Instant::now();
        let at = |ms| start + Duration::from_millis(ms);
        let mut syncing = Syncing::new(start);

        assert!(syncing.advance(at(200)));
        assert!(syncing.scale > 0.0 && syncing.scale < 1.0);
        assert!(syncing.advance(at(400)));
        assert_eq!(syncing.scale, 1.0);

        // Done: it shrinks from where it is, then is gone.
        syncing.finish(at(1000));
        assert!(!syncing.is_running());
        assert!(syncing.advance(at(1075)));
        let shrinking = syncing.scale;
        assert!(shrinking > 0.0 && shrinking < 1.0);

        // Another sync grows it back from that size, not from nothing.
        syncing.start(at(1075));
        assert!(syncing.is_running());
        assert!(syncing.advance(at(1075)));
        assert_eq!(syncing.scale, shrinking);

        syncing.finish(at(2000));
        assert!(!syncing.advance(at(2150)));
        assert_eq!(syncing.scale, 0.0);
    }
}
