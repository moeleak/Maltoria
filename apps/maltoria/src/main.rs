//! Maltoria, in Material 3: a library of visual novels, how long each was
//! played, and the one container they all run in.
//!
//! The Swift helper does the work, as it does for the GPUI app: this is its
//! interface, built on `maltoria-core` and `material-ui-rs`.

mod activity;
mod bridge;
mod container;
mod library;
mod prefs;
mod settings;

use std::collections::HashMap;
use std::sync::Arc;

use iced::time::Instant;
use iced::widget::{Column, container as iced_container};
use iced::{Length, Size, Subscription, Task};
use maltoria_core::bridge::dto::{Container, FoldersDto, SetupStatusDto};
use maltoria_core::bridge::{
    BottleCreateUpdate, BridgeEvent, BridgeStatus, BridgeSupervisor, ScanUpdate, SetupUpdate,
};
use maltoria_core::i18n::s;
use maltoria_core::model::Game;
use maltoria_core::runtime::RuntimeTable;
use material::widget::button::{self, ButtonVariant};
use material::widget::theme_picker::{MaterialColor, ThemeAction, ThemeController};
use material::widget::{image, navigation, progress_bar, snackbar};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::container::Choice;
use crate::prefs::Prefs;
use crate::settings::Appearance;

const WINDOW_SIZE: Size = Size::new(1280.0, 840.0);
const MIN_WINDOW_SIZE: Size = Size::new(720.0, 560.0);

pub fn main() -> iced::Result {
    material::application(boot, update, view)
        .title(title)
        .theme(theme)
        .subscription(subscription)
        .window(material::window_with_min_size(WINDOW_SIZE, MIN_WINDOW_SIZE))
        .default_font(default_font())
        .run()
}

/// The interface font: the system's CJK sans serif, which also covers Latin,
/// so Japanese and Chinese titles render in one face.
fn default_font() -> iced::Font {
    iced::Font::with_name(if cfg!(target_os = "macos") {
        "PingFang SC"
    } else {
        "Noto Sans CJK SC"
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Library,
    Activity,
    Settings,
}

fn destinations() -> [navigation::Destination<Page>; 3] {
    [
        navigation::Destination::new(Page::Library, "stadia_controller", s().library.title),
        navigation::Destination::new(Page::Activity, "schedule", s().app.activity_title),
        navigation::Destination::new(Page::Settings, "settings", s().app.settings_title),
    ]
}

#[derive(Debug, Clone)]
pub enum Message {
    Bridge(Arc<BridgeEvent>),
    Navigate(Page),
    Frame(Instant),
    WindowResized(Size),
    SystemTheme(iced::theme::Mode),
    Retry,
    DismissNotice,
    // The library.
    Query(String),
    Filter(library::Filter),
    Sort(library::Sort),
    CoverShown(String),
    CoverLoaded(image::Loaded<String>),
    Select(String),
    CloseDetails,
    Play(String),
    Cancel(String),
    Stop(String),
    Reveal(String),
    SetHidden(String, bool),
    SetLocale(String),
    AddFolder,
    AddProgram,
    Rescan,
    // Settings.
    InstallRuntime,
    CheckRuntimeUpdate,
    CreateContainer,
    OpenTool(&'static str),
    RevealContainer,
    InstallFonts,
    SetAppearance(Appearance),
    SelectColor(MaterialColor),
}

pub struct App {
    /// `None` in demo mode, which shows sample games without the helper.
    bridge: Option<BridgeSupervisor>,
    status: BridgeStatus,
    destinations: [navigation::Destination<Page>; 3],
    navigation: navigation::NavigationState<Page>,
    window_size: Size,
    now: Instant,
    prefs: Prefs,
    containers: Vec<Container>,
    /// How many games each container has, as their entries arrive.
    counts: HashMap<String, usize>,
    /// The container the library shows.
    container: Option<String>,
    /// A container is being created for the library.
    creating: bool,
    games: Vec<Game>,
    runtime: RuntimeTable,
    library: library::State,
    folders: Option<FoldersDto>,
    /// Scan the folders again once the helper confirms the new ones.
    scan_after_folders: bool,
    /// Re-read the library once the helper confirms a pinned program.
    reload_after_pin: bool,
    setup: Option<SetupStatusDto>,
    install: Option<settings::Install>,
    /// `defaultBottleLocation`, where new containers go.
    default_location: Option<String>,
    notice: Option<String>,
    snackbar: snackbar::Transition,
    theme: ThemeController,
    system_dark: bool,
}

fn boot() -> (App, Task<Message>) {
    let demo = std::env::args().any(|arg| arg == "--demo")
        || maltoria_core::model::demo_requested(std::env::var("MALTORIA_DEMO").ok().as_deref());
    let prefs = if demo {
        Prefs::default()
    } else {
        Prefs::load()
    };
    let color = MaterialColor::ALL
        .get(prefs.color)
        .copied()
        .unwrap_or(MaterialColor::Purple);
    let dark_mode = prefs.dark_mode.unwrap_or(false);

    let mut app = App {
        bridge: (!demo).then(bridge::start),
        status: BridgeStatus::Locating,
        destinations: destinations(),
        navigation: navigation::NavigationState::new(Page::Library),
        window_size: WINDOW_SIZE,
        now: Instant::now(),
        containers: Vec::new(),
        counts: HashMap::new(),
        container: prefs.container.clone(),
        creating: false,
        games: Vec::new(),
        runtime: RuntimeTable::default(),
        library: library::State::default(),
        folders: None,
        scan_after_folders: false,
        reload_after_pin: false,
        setup: None,
        install: None,
        default_location: None,
        notice: None,
        snackbar: snackbar::Transition::default(),
        theme: ThemeController::new(color, dark_mode),
        system_dark: false,
        prefs,
    };

    if demo {
        app.games = maltoria_core::model::demo_data().to_vec();
        app.container = Some("demo".into());
    }

    (app, iced::system::theme().map(Message::SystemTheme))
}

fn title(_app: &App) -> String {
    "Maltoria".to_owned()
}

fn theme(app: &App) -> Theme {
    app.theme.theme("Maltoria")
}

fn subscription(app: &App) -> Subscription<Message> {
    let mut subscriptions = vec![
        bridge::events(),
        iced::window::resize_events().map(|(_, size)| Message::WindowResized(size)),
        iced::system::theme_changes().map(Message::SystemTheme),
    ];

    if app.navigation.is_animating() || app.theme.is_animating() || app.snackbar.is_active() {
        subscriptions.push(iced::window::frames().map(Message::Frame));
    }

    Subscription::batch(subscriptions)
}

impl App {
    fn container(&self) -> Option<&Container> {
        let path = self.container.as_deref()?;
        self.containers
            .iter()
            .find(|container| container.path == path)
    }

    fn game(&self, id: &str) -> Option<&Game> {
        self.games.iter().find(|game| game.id == id)
    }

    fn runtime_installed(&self) -> Option<bool> {
        self.setup
            .as_ref()
            .map(|setup| setup.any_engine_available.unwrap_or(setup.installed))
    }

    fn notify(&mut self, notice: impl Into<String>) {
        self.notice = Some(notice.into());
        self.snackbar.show(Instant::now());
    }

    /// Calls the helper about the library's container, if there are both.
    fn with_container(&self, call: impl FnOnce(&BridgeSupervisor, &str)) {
        if let (Some(bridge), Some(container)) = (&self.bridge, self.container.as_deref()) {
            call(bridge, container);
        }
    }

    /// Settles which container the library shows, once the entries allow it.
    fn choose_container(&mut self) {
        if self.creating {
            return;
        }

        match container::choose(
            &self.containers,
            self.prefs.container.as_deref(),
            &self.counts,
        ) {
            Choice::Use(path) => self.use_container(path),
            Choice::Waiting => {}
            Choice::Create => {
                self.container = None;
                self.games.clear();

                // A container needs a runtime first; setup creates it after.
                if self.runtime_installed() == Some(true) {
                    self.create_container();
                }
            }
        }
    }

    fn use_container(&mut self, path: String) {
        if self.prefs.container.as_deref() != Some(path.as_str()) {
            self.prefs.container = Some(path.clone());
            self.prefs.save();
        }

        if self.container.as_deref() == Some(path.as_str()) && !self.games.is_empty() {
            return;
        }

        self.container = Some(path.clone());
        self.runtime = RuntimeTable::default();

        if let Some(bridge) = &self.bridge {
            bridge.configure_runtime(&path);
            bridge.load_folders(&path);
            bridge.load_entries(&path);
        }
    }

    fn create_container(&mut self) {
        let Some(bridge) = &self.bridge else {
            return;
        };

        self.creating = true;
        bridge.create_bottle(
            container::NAME,
            &container::parent_directory(self.default_location.as_deref()),
            container::WINDOWS_VERSION,
        );
    }

    fn set_games(&mut self, container: &str, entries: Vec<maltoria_core::bridge::dto::EntryDto>) {
        self.counts.insert(container.to_owned(), entries.len());

        if self.container.as_deref() != Some(container) {
            return;
        }

        self.games = entries
            .into_iter()
            .map(|entry| {
                let mut game = entry.into_game(1);
                game.container = container.to_owned();
                game
            })
            .collect();

        // Covers whose file changed show again from scratch.
        self.library.covers.clear();
    }

    fn load_settings(&mut self, id: &str) {
        self.library.settings = None;

        if let Some(executable) = self.game(id).and_then(library::executable) {
            self.with_container(|bridge, container| {
                bridge.load_program_settings(container, &executable)
            });
        }
    }

    fn apply_theme(&mut self) {
        let dark_mode = self.prefs.dark_mode.unwrap_or(self.system_dark);

        if dark_mode != self.theme.dark_mode() {
            let origin =
                iced::Point::new(self.window_size.width / 2.0, self.window_size.height / 2.0);
            self.theme.update(
                ThemeAction::SetDarkMode { dark_mode, origin },
                self.window_size,
                0.0,
                Instant::now(),
            );
        }
    }
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::Bridge(event) => match Arc::try_unwrap(event) {
            Ok(event) => return bridge_event(app, event),
            Err(_) => {}
        },
        Message::Navigate(page) => {
            app.navigation.select_now_for_size(page, app.window_size);
        }
        Message::Frame(now) => {
            app.now = now;
            app.navigation.advance_frame(now);
            let _ = app.theme.advance(now);
            let _ = app.snackbar.advance(now);
        }
        Message::WindowResized(size) => app.window_size = size,
        Message::SystemTheme(mode) => {
            app.system_dark = matches!(mode, iced::theme::Mode::Dark);
            app.apply_theme();
        }
        Message::Retry => {
            if let Some(bridge) = &app.bridge {
                bridge.retry();
            }
        }
        Message::DismissNotice => app.snackbar.dismiss(Instant::now()),
        Message::Query(query) => app.library.query = query,
        Message::Filter(filter) => app.library.filter = filter,
        Message::Sort(sort) => app.library.sort = sort,
        Message::CoverShown(id) => {
            if let Some(art) = app.game(&id).and_then(|game| game.art.clone()) {
                return app.library.covers.request(
                    id,
                    image::Source::Path(art),
                    Message::CoverLoaded,
                );
            }
        }
        Message::CoverLoaded(loaded) => app.library.covers.insert(loaded),
        Message::Select(id) => {
            if app.library.selected.as_deref() != Some(id.as_str()) {
                app.library.selected = Some(id.clone());
                app.load_settings(&id);
            }
        }
        Message::CloseDetails => {
            app.library.selected = None;
            app.library.settings = None;
        }
        Message::Play(id) => {
            let executable = app.game(&id).and_then(|game| game.last_executable.clone());
            app.with_container(|bridge, container| {
                bridge.launch(container, &id, executable.as_deref())
            });
        }
        Message::Cancel(id) => {
            app.with_container(|bridge, container| bridge.cancel(container, &id))
        }
        Message::Stop(id) => app.with_container(|bridge, container| bridge.stop(container, &id)),
        Message::Reveal(id) => {
            if let Some(path) = app.game(&id).and_then(library::reveal_path) {
                app.with_container(|bridge, container| bridge.reveal(container, &path));
            }
        }
        Message::SetHidden(id, hidden) => {
            app.with_container(|bridge, container| bridge.set_hidden(container, &id, hidden));
        }
        Message::SetLocale(locale) => {
            if let Some(settings) = app.library.settings.as_mut() {
                settings.locale = locale;
                let patch = maltoria_core::program_settings::settings_patch(
                    settings,
                    &settings.arguments,
                    &settings.environment,
                );
                let executable = settings.executable.clone();
                app.with_container(|bridge, container| {
                    bridge.save_program_settings(container, &executable, patch);
                });
            }
        }
        Message::AddFolder => {
            if let Some(bridge) = &app.bridge {
                let home = std::env::var("HOME").unwrap_or_default();
                bridge.choose_folders("scan-roots", &home);
            }
        }
        Message::AddProgram => {
            app.with_container(|bridge, container| {
                bridge.choose_path(
                    "pin",
                    "file",
                    Some(&format!("{container}/drive_c")),
                    None,
                    &["exe"],
                );
            });
        }
        Message::Rescan => app.with_container(|bridge, container| bridge.start_scan(container)),
        Message::InstallRuntime => {
            if let Some(bridge) = &app.bridge {
                app.install = Some(settings::Install::default());
                bridge.install_setup();
            }
        }
        Message::CheckRuntimeUpdate => {
            if let Some(bridge) = &app.bridge {
                bridge.check_wine_update();
            }
        }
        Message::CreateContainer => app.create_container(),
        Message::OpenTool(tool) => {
            app.with_container(|bridge, container| bridge.open_bottle_tool(container, tool));
        }
        Message::RevealContainer => {
            app.with_container(|bridge, container| bridge.reveal(container, container));
        }
        Message::InstallFonts => {
            app.with_container(|bridge, container| {
                bridge.install_winetricks(container, &[container::FONTS_VERB.to_owned()]);
            });
            app.notify(s().app.installing_fonts);
        }
        Message::SetAppearance(appearance) => {
            app.prefs.dark_mode = appearance.dark_mode();
            app.prefs.save();
            app.apply_theme();
        }
        Message::SelectColor(color) => {
            app.prefs.color = MaterialColor::ALL
                .iter()
                .position(|c| *c == color)
                .unwrap_or(0);
            app.prefs.save();
            app.theme.update(
                ThemeAction::SelectColor(color),
                app.window_size,
                0.0,
                Instant::now(),
            );
        }
    }

    Task::none()
}

fn bridge_event(app: &mut App, event: BridgeEvent) -> Task<Message> {
    match event {
        BridgeEvent::Status(status) => {
            let connected = matches!(status, BridgeStatus::Connected { .. });
            app.status = status;

            if connected && let Some(bridge) = &app.bridge {
                bridge.load_preferences();
                bridge.load_setup_status();
            } else {
                app.setup = None;
            }
        }
        BridgeEvent::Containers(containers) | BridgeEvent::ContainersRefreshed(containers) => {
            app.counts.clear();

            if let Some(bridge) = &app.bridge {
                for container in containers.iter().filter(|container| container.available) {
                    bridge.load_entries(&container.path);
                }
            }

            app.containers = containers;
            app.choose_container();
        }
        BridgeEvent::Entries { container, entries } => {
            app.set_games(&container, entries);
            app.choose_container();
        }
        BridgeEvent::Runtime(update) => {
            if app.container.as_deref() == Some(update.container.as_str()) {
                app.runtime.apply(update);
            }
        }
        BridgeEvent::Preferences { values, .. } => {
            app.default_location = values
                .get(maltoria_core::preferences::DEFAULT_BOTTLE_LOCATION)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
        }
        BridgeEvent::Setup { update } => match update {
            SetupUpdate::Status {
                installed,
                managed_installed,
                any_engine_available,
                version,
                rosetta_installed,
                apple_silicon,
            } => {
                app.setup = Some(SetupStatusDto {
                    installed,
                    managed_installed: Some(managed_installed.unwrap_or(installed)),
                    any_engine_available: Some(any_engine_available.unwrap_or(installed)),
                    version,
                    path: String::new(),
                    rosetta_installed,
                    apple_silicon,
                });
                app.choose_container();
            }
            SetupUpdate::Installing { stage, progress } => {
                app.install = Some(settings::Install { stage, progress });
            }
            SetupUpdate::Installed { version } => {
                app.install = None;
                if let Some(setup) = app.setup.as_mut() {
                    setup.installed = true;
                    setup.managed_installed = Some(true);
                    setup.any_engine_available = Some(true);
                    setup.version = version;
                }
                app.choose_container();
            }
            SetupUpdate::WineUpdate { local, remote } => {
                app.notify(s().app.runtime_update_available(&local, &remote));
            }
            SetupUpdate::Failed { reason, .. } => {
                app.install = None;
                app.notify(reason);
            }
            _ => {}
        },
        BridgeEvent::BottleCreate { update } => match update {
            BottleCreateUpdate::Completed {
                path, containers, ..
            } => {
                app.creating = false;
                app.containers = containers;
                app.use_container(path.clone());

                // Menus and messages in Japanese and Chinese need the fonts.
                if let Some(bridge) = &app.bridge {
                    bridge.install_winetricks(&path, &[container::FONTS_VERB.to_owned()]);
                }
            }
            BottleCreateUpdate::Failed { reason, .. } => {
                app.creating = false;
                app.notify(reason);
            }
            _ => {}
        },
        BridgeEvent::FoldersChosen { purpose, paths } if purpose == "scan-roots" => {
            let paths: Vec<String> = paths
                .into_iter()
                .filter(|path| path.starts_with('/'))
                .collect();

            if !paths.is_empty() {
                app.scan_after_folders = true;
                app.with_container(|bridge, container| bridge.add_scan_roots(container, &paths));
            }
        }
        BridgeEvent::Folders { container, folders } => {
            if app.container.as_deref() == Some(container.as_str()) {
                app.folders = Some(folders);

                if std::mem::take(&mut app.scan_after_folders) {
                    app.with_container(|bridge, container| bridge.start_scan(container));
                }
            }
        }
        BridgeEvent::Scan { container, update } => match update {
            ScanUpdate::Completed { entries } => app.set_games(&container, entries),
            ScanUpdate::Failed { reason, .. } => app.notify(reason),
            _ => {}
        },
        BridgeEvent::PathChosen {
            purpose,
            path: Some(path),
        } if purpose == "pin" => {
            let name = std::path::Path::new(&path)
                .file_stem()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| s().state.fallback_program.to_owned());

            app.reload_after_pin = true;
            app.with_container(|bridge, container| {
                bridge.set_program_pinned(container, &path, &name, true)
            });
        }
        BridgeEvent::Programs { .. } => {
            if std::mem::take(&mut app.reload_after_pin)
                && let Some(bridge) = &app.bridge
            {
                bridge.reload_library();
            }
        }
        BridgeEvent::EntryHidden {
            container,
            entry_id,
            hidden,
        } => {
            if app.container.as_deref() == Some(container.as_str())
                && let Some(game) = app.games.iter_mut().find(|game| game.id == entry_id)
            {
                game.hidden = hidden;
            }
        }
        BridgeEvent::ProgramSettings { settings } => {
            let selected = app
                .library
                .selected
                .as_deref()
                .and_then(|id| app.game(id))
                .and_then(library::executable);

            if selected.as_deref() == Some(settings.executable.as_str()) {
                app.library.settings = Some(settings);
            }
        }
        BridgeEvent::ActionFailed { reason }
        | BridgeEvent::RuntimeFailed { reason, .. }
        | BridgeEvent::LoadFailed { reason, .. }
        | BridgeEvent::ProgramSettingsFailed { reason, .. } => app.notify(reason),
        _ => {}
    }

    Task::none()
}

fn view(app: &App) -> material::Element<'_, Message> {
    let page: material::Element<'_, Message> = match app.navigation.selected() {
        Page::Library => match setup_view(app) {
            Some(setup) => setup,
            None => library::view(&app.games, &app.library, &app.runtime),
        },
        Page::Activity => activity::view(&app.games, chrono::Local::now().date_naive()),
        Page::Settings => settings::view(settings::View {
            setup: app.setup.as_ref(),
            install: app.install.as_ref(),
            container: app.container(),
            folders: app.folders.as_ref(),
            color: app.theme.selected_color(),
            appearance: Appearance::from_dark_mode(app.prefs.dark_mode),
        }),
    };

    let content = navigation::suite(&app.destinations, &app.navigation)
        .window_size(app.window_size)
        .view(Message::Navigate, page);

    match &app.notice {
        Some(notice) if app.snackbar.is_active() => snackbar::host(
            content,
            &app.snackbar,
            app.now,
            notice.as_str(),
            s().common.close,
            Message::DismissNotice,
        ),
        _ => content,
    }
}

/// What the library shows before it can show games: the helper connecting
/// or failing, the runtime to install, or the container being created.
fn setup_view(app: &App) -> Option<material::Element<'_, Message>> {
    if app.bridge.is_none() {
        return None;
    }

    let app_strings = &s().app;

    let (title, detail, action): (&str, String, Option<material::Element<'_, Message>>) =
        match &app.status {
            BridgeStatus::Failed { reason, retryable } => (
                s().dialogs.bridge_unavailable,
                reason.clone(),
                retryable.then(|| {
                    button::button(s().common.retry, ButtonVariant::Filled)
                        .on_press(Message::Retry)
                        .into()
                }),
            ),
            BridgeStatus::Stopped => (
                s().dialogs.bridge_unavailable,
                s().state.bridge_not_running.to_owned(),
                None,
            ),
            BridgeStatus::Locating | BridgeStatus::Starting => {
                (s().dialogs.bridge_connecting, String::new(), None)
            }
            BridgeStatus::Connected { .. } if app.install.is_some() => (
                app_strings.runtime_installing,
                app_strings.setup_runtime_detail.to_owned(),
                None,
            ),
            BridgeStatus::Connected { .. } if app.runtime_installed() == Some(false) => (
                app_strings.setup_runtime_title,
                app_strings.setup_runtime_detail.to_owned(),
                Some(
                    button::button(app_strings.runtime_install, ButtonVariant::Filled)
                        .on_press(Message::InstallRuntime)
                        .into(),
                ),
            ),
            BridgeStatus::Connected { .. } if app.creating => (
                app_strings.setup_container_title,
                app_strings.setup_container_detail.to_owned(),
                None,
            ),
            BridgeStatus::Connected { .. }
                if app.container.is_none() && app.runtime_installed() == Some(true) =>
            {
                (
                    app_strings.setup_container_title,
                    app_strings.setup_container_detail.to_owned(),
                    Some(
                        button::button(app_strings.create_container, ButtonVariant::Filled)
                            .on_press(Message::CreateContainer)
                            .into(),
                    ),
                )
            }
            BridgeStatus::Connected { .. } if app.container.is_none() || app.setup.is_none() => {
                (s().library.loading_library, String::new(), None)
            }
            BridgeStatus::Connected { .. } => return None,
        };

    let busy = action.is_none()
        && !matches!(
            app.status,
            BridgeStatus::Failed { .. } | BridgeStatus::Stopped
        );

    let mut column = Column::new()
        .push(material::fonts::icon("stadia_controller", 56.0).style(material::text::primary))
        .push(material::text::type_scale(
            title,
            tokens::typography::HEADLINE_SMALL,
        ))
        .spacing(16)
        .max_width(440)
        .align_x(iced::Alignment::Center);

    if !detail.is_empty() {
        column = column.push(
            material::text::type_scale(detail, tokens::typography::BODY_MEDIUM)
                .style(material::text::surface_variant)
                .align_x(iced::alignment::Horizontal::Center),
        );
    }

    if busy {
        let fraction = app
            .install
            .as_ref()
            .and_then(|install| install.progress)
            .filter(|(_, total)| *total > 0)
            .map(|(received, total)| received as f32 / total as f32);
        column = column.push(match fraction {
            Some(progress) => {
                progress_bar::linear(progress_bar::LinearProgressMode::determinate(progress, 0.0))
            }
            None => progress_bar::linear(progress_bar::LinearProgressMode::indeterminate(0.0)),
        });
    }

    if let Some(action) = action {
        column = column.push(action);
    }

    Some(
        iced_container(column)
            .center(Length::Fill)
            .style(|theme: &Theme| iced_container::Style {
                background: Some(theme.colors().surface.container.low.into()),
                ..iced_container::Style::default()
            })
            .into(),
    )
}
