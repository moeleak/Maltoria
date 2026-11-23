//! The library: every game in the container as a grid of covers, with a
//! search, a few filters, and a sheet of the selected game's details.
//!
//! Covers load as they come into view and are kept for the frames after; a
//! cover's context menu holds what a game's details offer.

use std::cmp::Ordering;

use iced::widget::{Column, Row, Space, container, mouse_area, scrollable, sensor, text};
use iced::{Alignment, Length};
use maltoria_core::i18n::s;
use maltoria_core::library_view::{matches_query, natural_cmp};
use maltoria_core::model::{Game, format_last_played, format_play_time};
use maltoria_core::program_settings::{ProgramSettingsDto, locale_label};
use maltoria_core::runtime::{PrimaryAction, RuntimeAction, RuntimeTable, primary_action_for};
use material::widget::button::{self, ButtonVariant, ChipVariant, IconButtonVariant};
use material::widget::{grid, image, menu, search, select, sheet};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;

/// The widest a cover gets before the grid adds a column.
pub const MAX_COVER_WIDTH: f32 = 176.0;
/// Covers are 3:4, the shape of most visual novel packages; others are cropped.
pub const COVER_RATIO: f32 = 3.0 / 4.0;
/// The longest side covers are decoded to: twice the widest cover, for Retina.
pub const COVER_DECODE_SIZE: u32 = 720;
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
    /// The game whose details are open, by id.
    pub selected: Option<String>,
    /// The selected game's run settings, once the helper sent them.
    pub settings: Option<ProgramSettingsDto>,
    pub covers: image::Cache<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            query: String::new(),
            filter: Filter::default(),
            sort: Sort::default(),
            selected: None,
            settings: None,
            covers: image::Cache::new(COVER_CACHE_SIZE)
                .aspect_ratio(COVER_RATIO)
                .max_dimension(COVER_DECODE_SIZE),
        }
    }
}

/// What a game's primary button does now.
pub fn primary_action(runtime: &RuntimeTable, game: &Game) -> PrimaryAction {
    match runtime.entry(&game.id) {
        Some(entry) => primary_action_for(entry.state, entry.capabilities),
        None => PrimaryAction::unavailable(),
    }
}

/// The message for a primary action on `game`.
fn action_message(action: Option<RuntimeAction>, game: &Game) -> Option<Message> {
    let id = game.id.clone();

    action.map(|action| match action {
        RuntimeAction::Launch => Message::Play(id),
        RuntimeAction::Cancel => Message::Cancel(id),
        RuntimeAction::Stop => Message::Stop(id),
    })
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
            button::extended_fab_with_icon("add", s().app.add_game, button::FabVariant::Primary)
                .on_press(Message::AddFolder),
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
        grid::grid(
            MAX_COVER_WIDTH,
            indices
                .iter()
                .map(|&index| cell(&games[index], state, runtime)),
        )
        .into()
    };

    let library = Column::new()
        .push(toolbar)
        .push(filters)
        .push(body)
        .spacing(20)
        .padding(24)
        .width(Length::Fill);

    let library = scrollable(library).height(Length::Fill).width(Length::Fill);

    let selected = state
        .selected
        .as_deref()
        .and_then(|id| games.iter().find(|game| game.id == id));

    // The grid keeps its place in the tree whether or not the details show:
    // moving it would reset its widgets' state, such as a context menu that
    // selecting the game just opened.
    let details: material::Element<'a, Message> = match selected {
        Some(game) => details(game, state, runtime),
        None => Space::new().width(0).into(),
    };

    Row::new()
        .push(library)
        .push(details)
        .height(Length::Fill)
        .into()
}

fn cover<'a>(game: &'a Game, state: &'a State, corner: f32) -> material::Element<'a, Message> {
    let art: material::Element<'a, Message> = if game.art.is_some() {
        image::cached(&state.covers, &game.id, corner)
    } else {
        image::placeholder(image::Placeholder::Missing, corner).into()
    };

    let id = game.id.clone();

    sensor(grid::aspect_ratio(art, COVER_RATIO))
        .on_show(move |_| Message::CoverShown(id.clone()))
        .into()
}

fn cell<'a>(
    game: &'a Game,
    state: &'a State,
    runtime: &'a RuntimeTable,
) -> material::Element<'a, Message> {
    let selected = state.selected.as_deref() == Some(game.id.as_str());
    let caption = caption(game, runtime);

    let body = Column::new()
        .push(cover(game, state, tokens::shape::CORNER_MEDIUM))
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
        .on_press(Message::Select(id.clone()))
        .on_double_click(Message::Play(id.clone()))
        .interaction(iced::mouse::Interaction::Pointer);

    let primary = primary_action(runtime, game);
    let primary_icon = match primary.action {
        Some(RuntimeAction::Stop) => "stop_circle",
        Some(RuntimeAction::Cancel) => "cancel",
        _ => "play_arrow",
    };

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
                .on_press(Message::Select(id.clone())),
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
                .on_press(Message::SetHidden(id, false))
        } else {
            menu::item(s().library.hide_game)
                .icon("visibility_off")
                .destructive()
                .on_press(Message::SetHidden(id, true))
        })
        .into()
}

/// The line under a cover: what the game is doing, or its play time.
fn caption(game: &Game, runtime: &RuntimeTable) -> String {
    if let Some(entry) = runtime.entry(&game.id)
        && entry.state.is_active()
    {
        return entry.state.phase_label().to_owned();
    }

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
            Row::new()
                .push(
                    button::button(s().app.add_folder, ButtonVariant::Filled)
                        .on_press(Message::AddFolder),
                )
                .push(
                    button::button(s().library.add_program_btn, ButtonVariant::Outlined)
                        .on_press(Message::AddProgram),
                )
                .spacing(12),
        );
    }

    container(column)
        .width(Length::Fill)
        .padding(48)
        .center_x(Length::Fill)
        .into()
}

/// The width of the details sheet.
const DETAILS_WIDTH: f32 = 360.0;

fn details<'a>(
    game: &'a Game,
    state: &'a State,
    runtime: &'a RuntimeTable,
) -> material::Element<'a, Message> {
    let primary = primary_action(runtime, game);
    let play = button::button(primary.label, ButtonVariant::Filled)
        .on_press_maybe(action_message(primary.action, game))
        .width(Length::Fill);

    let header = Row::new().push(Space::new().width(Length::Fill)).push(
        button::icon_button("close", IconButtonVariant::Standard).on_press(Message::CloseDetails),
    );

    let mut facts = Column::new().spacing(8);

    for (label, value) in [
        (
            s().statistics.total_playtime,
            format_play_time(game.play_time).unwrap_or_else(|| s().app.not_played_yet.to_owned()),
        ),
        (
            s().library.last_played,
            format_last_played(game.last_played).unwrap_or_else(|| "—".to_owned()),
        ),
    ] {
        facts = facts.push(
            Row::new()
                .push(
                    material::text::type_scale(label, tokens::typography::BODY_MEDIUM)
                        .style(material::text::surface_variant)
                        .width(Length::Fill),
                )
                .push(material::text::type_scale(
                    value,
                    tokens::typography::BODY_MEDIUM,
                )),
        );
    }

    let mut column = Column::new()
        .push(header)
        .push(cover(game, state, tokens::shape::CORNER_LARGE))
        .push(material::text::type_scale(
            game.title.as_str(),
            tokens::typography::HEADLINE_SMALL,
        ))
        .push(
            material::text::type_scale(game.studio(), tokens::typography::BODY_MEDIUM)
                .style(material::text::surface_variant),
        )
        .push(play)
        .push(facts)
        .spacing(16)
        .padding(16)
        .width(Length::Fill);

    if let Some(settings) = state
        .settings
        .as_ref()
        .filter(|settings| Some(settings.executable.as_str()) == executable(game).as_deref())
    {
        column = column.push(
            Column::new()
                .push(material::text::type_scale(
                    s().program_settings.locale_label,
                    tokens::typography::TITLE_SMALL,
                ))
                .push(
                    select::outlined(
                        LOCALES.map(LocaleOption),
                        Some(LocaleOption(
                            LOCALES
                                .iter()
                                .find(|locale| **locale == settings.locale)
                                .copied()
                                .unwrap_or(""),
                        )),
                        |LocaleOption(locale)| Message::SetLocale(locale.to_owned()),
                    )
                    .width(Length::Fill),
                )
                .spacing(8),
        );
    }

    if let Some(description) = game.description.as_deref() {
        column = column.push(
            material::text::type_scale(description, tokens::typography::BODY_MEDIUM)
                .style(material::text::surface_variant),
        );
    }

    sheet::standard_side(scrollable(column).height(Length::Fill))
        .width(Length::Fixed(DETAILS_WIDTH))
        .height(Length::Fill)
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
struct LocaleOption(&'static str);

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
}
