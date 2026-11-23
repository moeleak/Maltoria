//! Maltoria, in Material 3: a library of visual novels, how long each was
//! played, and the one container they all run in.
//!
//! The Swift helper does the work, as it does for the GPUI app: this is its
//! interface, built on `maltoria-core` and `material-ui-rs`.

mod activity;
mod bridge;
mod container;
mod details;
mod dev;
mod library;
mod metadata;
mod playing;
mod pointer;
mod prefs;
mod removal;
mod settings;
mod wine;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use iced::time::Instant;
use iced::widget::{Column, Space, container as iced_container};
use iced::{Length, Size, Subscription, Task};
use maltoria_core::bridge::dto::{Container, FoldersDto, SetupStatusDto};
use maltoria_core::bridge::{
    BottleCreateUpdate, BridgeEvent, BridgeStatus, BridgeSupervisor, MetadataUpdate, ScanUpdate,
    SetupUpdate,
};
use maltoria_core::i18n::s;
use maltoria_core::model::Game;
use maltoria_core::onboarding::MetadataPreferences;
use maltoria_core::runtime::RuntimeTable;
use material::widget::button::{self, ButtonVariant};
use material::widget::theme_picker::{MaterialColor, ThemeAction, ThemeController};
use material::widget::{dialog, image, navigation, progress_bar, segmented_button, snackbar};
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
    DevPoll,
    /// Answers the helper's clipboard question before a launch.
    ClipboardDecide(maltoria_core::runtime::ClipboardDecision),
    ClipboardDismiss,
    DevShot(String, iced::window::Screenshot),
    // The library.
    Query(String),
    Filter(library::Filter),
    Sort(library::Sort),
    CoverShown(String),
    /// A portrait or thumbnail file came into view.
    ImageShown(String),
    PictureLoaded(image::Loaded<String>),
    CoverLoaded(image::Loaded<String>),
    Select(String),
    OpenDetails(String),
    CloseDetails,
    Escape,
    Rate(String, Option<u8>),
    Play(String),
    Cancel(String),
    Stop(String),
    Reveal(String),
    SetHidden(String, bool),
    /// Asks before the game leaves the library.
    Remove(String),
    RemoveConfirm,
    RemoveDismiss,
    SetLocale(String),
    AddProgram,
    Rescan,
    SyncMetadata,
    // The metadata editor.
    OpenEditor(String),
    CloseEditor,
    EditorQuery(String),
    EditorSearch,
    EditorSource(String),
    EditorLink(usize),
    EditorLinkInput(String),
    EditorLinkById,
    EditorUnlink(String),
    EditorSave,
    EditorRefresh,
    ChooseCover(String),
    ResetCover(String),
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
    /// The clipboard question a launch is waiting on, while it shows.
    clipboard: Option<maltoria_core::runtime::ClipboardPrompt>,
    clipboard_transition: dialog::Transition,
    /// The game being asked about deleting, by id and title, while the
    /// question shows.
    removing: Option<(String, String)>,
    removing_transition: dialog::Transition,
    library: library::State,
    /// The game whose details dialog is open, or closing.
    details: Option<String>,
    details_transition: dialog::Transition,
    /// The metadata editor, shown in the details dialog.
    editor: Option<metadata::Editor>,
    /// The game a chosen cover image is for.
    cover_for: Option<String>,
    /// The metadata sources, once the helper's preferences arrived.
    metadata_preferences: Option<MetadataPreferences>,
    /// Containers synced automatically this session.
    auto_synced: HashSet<String>,
    folders: Option<FoldersDto>,
    /// Scan the folders again once the helper confirms a change to them.
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
    /// Whether the theme took the system's mode once: the first time it
    /// changes without a reveal, before the window shows.
    theme_settled: bool,
    appearance: segmented_button::State,
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

    // Before the helper starts, which runs games with the installed runtime.
    if !demo {
        wine::install_carried();
    }

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
        clipboard: None,
        clipboard_transition: dialog::Transition::default(),
        removing: None,
        removing_transition: dialog::Transition::default(),
        library: library::State::default(),
        details: None,
        details_transition: dialog::Transition::default(),
        editor: None,
        cover_for: None,
        metadata_preferences: None,
        auto_synced: HashSet::new(),
        folders: None,
        scan_after_folders: false,
        reload_after_pin: false,
        setup: None,
        install: None,
        default_location: None,
        notice: None,
        snackbar: snackbar::Transition::default(),
        theme: ThemeController::new(color, dark_mode),
        theme_settled: false,
        appearance: segmented_button::State::new(
            Appearance::ALL
                .iter()
                .position(|appearance| *appearance == Appearance::from_dark_mode(prefs.dark_mode))
                .unwrap_or(0),
        ),
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
        iced::event::listen_with(|event, status, _| match (event, status) {
            (
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }),
                iced::event::Status::Ignored,
            ) => Some(Message::Escape),
            _ => None,
        }),
        dev::requests(),
    ];

    if app.navigation.is_animating()
        || app.theme.is_animating()
        || app.snackbar.is_active()
        || app.details_transition.is_animating()
        || app.clipboard_transition.is_animating()
        || app.removing_transition.is_animating()
        || app.appearance.is_animating()
    {
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

        let games: Vec<Game> = entries
            .into_iter()
            .map(|entry| {
                let mut game = entry.into_game(1);
                game.container = container.to_owned();
                game
            })
            .collect();

        // A cover whose file changed is decoded again; the others stay.
        for game in &games {
            if self
                .game(&game.id)
                .is_some_and(|known| known.art != game.art)
            {
                self.library.covers.remove(&game.id);
            }
        }

        self.games = games;
    }

    /// Syncs the metadata of the library's container once per session, if
    /// it has games known by nothing but their folder's name.
    fn auto_sync(&mut self) {
        let (Some(bridge), Some(container), Some(preferences)) = (
            &self.bridge,
            self.container.clone(),
            self.metadata_preferences.as_ref(),
        ) else {
            return;
        };
        let sources = metadata::sync_sources(preferences);

        if sources.is_empty()
            || self.auto_synced.contains(&container)
            || !metadata::needs_sync(&self.games)
        {
            return;
        }

        bridge.sync_metadata(&container, sources, true, true);
        self.auto_synced.insert(container);
    }

    /// Takes `id` out of the library: its card goes at once, and the
    /// helper's reply has the library read again.
    fn remove_game(&mut self, id: &str) {
        let Some(removal) = self
            .game(id)
            .and_then(|game| removal::of(game, self.folders.as_ref()))
        else {
            return;
        };

        self.games.retain(|game| game.id != id);

        if self.details.as_deref() == Some(id) {
            self.details_transition.dismiss(Instant::now());
        }

        match removal {
            removal::Removal::Unpin { path, name } => {
                self.reload_after_pin = true;
                self.with_container(|bridge, container| {
                    bridge.set_program_pinned(container, &path, &name, false);
                });
            }
            removal::Removal::Folder(edit) => {
                self.scan_after_folders = true;
                self.with_container(|bridge, container| bridge.edit_folders(container, edit));
            }
        }
    }

    fn open_details(&mut self, id: String) {
        if self.details.as_deref() != Some(id.as_str()) {
            self.load_settings(&id);
        }

        self.library.selected = Some(id.clone());
        self.details = Some(id);

        if !self.details_transition.is_active() {
            self.details_transition.show(Instant::now());
        }
    }

    fn load_settings(&mut self, id: &str) {
        self.library.settings = None;

        if let Some(executable) = self.game(id).and_then(library::executable) {
            self.with_container(|bridge, container| {
                bridge.load_program_settings(container, &executable)
            });
        }
    }

    fn center(&self) -> iced::Point {
        iced::Point::new(self.window_size.width / 2.0, self.window_size.height / 2.0)
    }

    /// Follows the dark mode the preferences or the system ask for, with a
    /// reveal from `origin` once the window shows.
    fn apply_theme(&mut self, origin: Option<iced::Point>) {
        let dark_mode = self.prefs.dark_mode.unwrap_or(self.system_dark);

        if !std::mem::replace(&mut self.theme_settled, true) {
            self.theme = ThemeController::new(self.theme.selected_color(), dark_mode);
            return;
        }

        if dark_mode != self.theme.dark_mode() {
            let origin = origin.unwrap_or_else(|| self.center());
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
            let _ = app.details_transition.advance(now);
            let _ = app.clipboard_transition.advance(now);

            if !app.clipboard_transition.is_active() {
                app.clipboard = None;
            }
            let _ = app.removing_transition.advance(now);

            if !app.removing_transition.is_active() {
                app.removing = None;
            }
            let _ = app.appearance.advance(now);

            // A dismissed dialog lets go of its game once it is gone.
            if !app.details_transition.is_active() && app.details.is_some() {
                app.details = None;
                app.editor = None;
                app.library.settings = None;
            }
        }
        Message::WindowResized(size) => app.window_size = size,
        Message::SystemTheme(mode) => {
            app.system_dark = matches!(mode, iced::theme::Mode::Dark);
            app.apply_theme(None);
        }
        Message::Retry => {
            if let Some(bridge) = &app.bridge {
                bridge.retry();
            }
        }
        Message::DismissNotice => app.snackbar.dismiss(Instant::now()),
        Message::DevPoll => return dev_requests(app),
        Message::ClipboardDecide(decision) => {
            if let (Some(bridge), Some(prompt)) = (&app.bridge, app.clipboard.clone()) {
                bridge.resolve_clipboard(prompt, decision);
            }
            app.clipboard_transition.dismiss(Instant::now());
        }
        Message::ClipboardDismiss => app.clipboard_transition.dismiss(Instant::now()),
        Message::DevShot(name, screenshot) => {
            if let Some(directory) = dev::directory()
                && let Err(error) = dev::save(&directory, &name, &screenshot)
            {
                eprintln!("maltoria: could not save the {name} snapshot: {error}");
            }
        }
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
        Message::ImageShown(path) => {
            return app.library.pictures.request(
                path.clone(),
                image::Source::Path(path.into()),
                Message::PictureLoaded,
            );
        }
        Message::PictureLoaded(loaded) => app.library.pictures.insert(loaded),
        Message::CoverLoaded(loaded) => app.library.covers.insert(loaded),
        Message::Select(id) => app.library.selected = Some(id),
        Message::OpenDetails(id) => {
            app.editor = None;
            app.open_details(id);
        }
        Message::CloseDetails => app.details_transition.dismiss(Instant::now()),
        Message::Escape => {
            if app.removing_transition.is_active() {
                app.removing_transition.dismiss(Instant::now());
            } else if app.editor.is_some() {
                app.editor = None;
            } else if app.details_transition.is_active() {
                app.details_transition.dismiss(Instant::now());
            }
        }
        Message::Rate(id, rating) => {
            if let Some(game) = app.games.iter_mut().find(|game| game.id == id) {
                game.rating = rating;
            }
            app.with_container(|bridge, container| {
                bridge.set_rating(container, &id, rating.map(i64::from));
            });
        }
        Message::SyncMetadata => {
            let sources = app
                .metadata_preferences
                .as_ref()
                .map(metadata::sync_sources)
                .unwrap_or_default();

            if sources.is_empty() {
                app.notify(s().dialogs.no_metadata_sources);
            } else {
                app.with_container(|bridge, container| {
                    bridge.sync_metadata(container, sources, false, true);
                });
                app.notify(s().state.scanning_and_syncing);
            }
        }
        Message::OpenEditor(id) => {
            let preferences = app.metadata_preferences.clone().unwrap_or_default();

            if let Some(game) = app.game(&id) {
                let editor = metadata::Editor::new(game, &preferences);
                let generation = editor.draft.generation;

                app.with_container(|bridge, container| {
                    bridge.metadata_record(container, &id, generation, "get", None, None);
                });
                app.editor = Some(editor);
                app.open_details(id);
            }
        }
        Message::CloseEditor => app.editor = None,
        Message::EditorQuery(query) => {
            if let Some(editor) = app.editor.as_mut() {
                editor.query = query;
            }
        }
        Message::EditorSource(source) => {
            if let Some(editor) = app.editor.as_mut() {
                editor.draft.search_source = source;
                editor.results.clear();
            }
            return update(app, Message::EditorSearch);
        }
        Message::EditorSearch => {
            if let Some(editor) = app.editor.as_mut()
                && !editor.query.trim().is_empty()
            {
                editor.draft.search_request += 1;
                editor.searching = true;
                editor.message = None;

                let (id, generation, request) = (
                    editor.game_id.clone(),
                    editor.draft.generation,
                    editor.draft.search_request,
                );
                let (query, source) = (
                    editor.query.trim().to_owned(),
                    editor.draft.search_source.clone(),
                );

                app.with_container(|bridge, container| {
                    bridge.search_metadata(
                        container,
                        &id,
                        generation,
                        request,
                        &query,
                        vec![source],
                    );
                });
            }
        }
        Message::EditorLink(index) => {
            if let Some(editor) = app.editor.as_mut()
                && let Some(candidate) = editor.results.get(index)
            {
                let source = editor.draft.search_source.clone();
                let raw = candidate.raw.clone();

                editor.message = editor.draft.bind(&source, raw).err();
            }
        }
        Message::EditorLinkInput(link) => {
            if let Some(editor) = app.editor.as_mut() {
                editor.link = link;
            }
        }
        Message::EditorLinkById => {
            if let Some(editor) = app.editor.as_mut() {
                let source = editor.draft.search_source.clone();

                match maltoria_core::metadata_draft::platform_id(&source, &editor.link) {
                    Some(id) => {
                        editor.draft.bind_id(&source, &id);
                        editor.link.clear();
                        editor.message = Some(s().metadata.msg_id_staged.to_owned());
                    }
                    None => editor.message = Some(s().metadata.err_invalid_id_or_url.to_owned()),
                }
            }
        }
        Message::EditorUnlink(source) => {
            if let Some(editor) = app.editor.as_mut() {
                editor.draft.unbind(&source);
            }
        }
        Message::EditorSave | Message::EditorRefresh => {
            let enabled = app
                .metadata_preferences
                .as_ref()
                .map(|preferences| preferences.enabled_sources.clone())
                .unwrap_or_default();

            if let Some(editor) = app.editor.as_mut() {
                let (operation, record, sources) = if matches!(message, Message::EditorSave) {
                    (
                        "save",
                        Some(editor.draft.record.clone()),
                        editor.draft.policy_sources(enabled),
                    )
                } else {
                    let bound = editor
                        .bindings()
                        .into_iter()
                        .map(|(source, _)| source.to_owned())
                        .collect();
                    ("refresh", None, bound)
                };

                editor.busy = true;
                editor.message = None;

                let (id, generation) = (editor.game_id.clone(), editor.draft.generation);

                app.with_container(|bridge, container| {
                    bridge.metadata_record(
                        container,
                        &id,
                        generation,
                        operation,
                        record,
                        Some(sources),
                    );
                });
            }
        }
        Message::ChooseCover(id) => {
            app.cover_for = Some(id);
            if let Some(bridge) = &app.bridge {
                bridge.choose_path(
                    "metadata-cover",
                    "file",
                    None,
                    None,
                    &metadata::COVER_EXTENSIONS,
                );
            }
        }
        Message::ResetCover(id) => {
            app.with_container(|bridge, container| bridge.reset_metadata_cover(container, &id));
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
        Message::Remove(id) => {
            if let Some(game) = app.game(&id).filter(|game| removal::removable(game)) {
                app.removing = Some((id, game.title.clone()));
                app.removing_transition.show(Instant::now());
            }
        }
        Message::RemoveConfirm => {
            if let Some((id, _)) = app.removing.clone() {
                app.remove_game(&id);
            }
            app.removing_transition.dismiss(Instant::now());
        }
        Message::RemoveDismiss => app.removing_transition.dismiss(Instant::now()),
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
        Message::AddProgram => {
            // The picker opens where the last game was picked.
            app.with_container(|bridge, _| {
                bridge.choose_path("pin", "file", None, None, &["exe"]);
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
            let index = Appearance::ALL
                .iter()
                .position(|mode| *mode == appearance)
                .unwrap_or(0);

            app.appearance.select(index, Instant::now());
            app.prefs.dark_mode = appearance.dark_mode();
            app.prefs.save();
            app.apply_theme(pointer::last_press());
        }
        Message::SelectColor(color) => {
            app.prefs.color = MaterialColor::ALL
                .iter()
                .position(|c| *c == color)
                .unwrap_or(0);
            app.prefs.save();

            let origin = pointer::last_press().unwrap_or_else(|| app.center());
            app.theme.select_color_from(color, origin, Instant::now());
        }
    }

    Task::none()
}

/// Follows the developer snapshot requests; see [`dev`].
fn dev_requests(app: &mut App) -> Task<Message> {
    let Some(directory) = dev::directory() else {
        return Task::none();
    };

    let mut tasks = Vec::new();

    for request in dev::take(&directory) {
        match request {
            dev::Request::Page(page) => app.navigation.select_now_for_size(page, app.window_size),
            dev::Request::Select(title) => {
                let id = app
                    .games
                    .iter()
                    .find(|game| game.title.contains(&title))
                    .map(|game| game.id.clone());

                if let Some(id) = id {
                    tasks.push(update(app, Message::OpenDetails(id)));
                }
            }
            dev::Request::Close => tasks.push(update(app, Message::CloseDetails)),
            dev::Request::Dismiss { name, after } => {
                tasks.push(update(app, Message::CloseDetails));
                tasks.push(
                    Task::perform(async move { std::thread::sleep(after) }, |()| ())
                        .then(|()| iced::window::latest())
                        .and_then(iced::window::screenshot)
                        .map(move |screenshot| Message::DevShot(name.clone(), screenshot)),
                );
            }
            dev::Request::Shot(name) => tasks.push(
                iced::window::latest()
                    .and_then(iced::window::screenshot)
                    .map(move |screenshot| Message::DevShot(name.clone(), screenshot)),
            ),
        }
    }

    Task::batch(tasks)
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
            app.auto_sync();
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
            app.metadata_preferences = Some(MetadataPreferences::read(&values));
            app.auto_sync();
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
        BridgeEvent::Folders { container, folders } => {
            if app.container.as_deref() == Some(container.as_str()) {
                app.folders = Some(folders);

                if std::mem::take(&mut app.scan_after_folders) {
                    app.with_container(|bridge, container| bridge.start_scan(container));
                }
            }
        }
        BridgeEvent::Scan { container, update } => match update {
            ScanUpdate::Completed { entries } => {
                // New games earn another automatic pass.
                app.auto_synced.remove(&container);
                app.set_games(&container, entries);
                app.auto_sync();
            }
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

            // A new game earns another automatic metadata pass.
            if let Some(container) = &app.container {
                app.auto_synced.remove(container);
            }
            app.reload_after_pin = true;
            app.with_container(|bridge, container| {
                bridge.set_program_pinned(container, &path, &name, true)
            });
        }
        BridgeEvent::PathChosen {
            purpose,
            path: Some(path),
        } if purpose == "metadata-cover" => {
            if let Some(id) = app.cover_for.take() {
                app.with_container(|bridge, container| {
                    bridge.import_metadata_cover(container, &id, &path);
                });
            }
        }
        BridgeEvent::Metadata { update } => return metadata_event(app, update),
        BridgeEvent::ClipboardPrompt(prompt) => {
            app.clipboard = Some(prompt);
            app.clipboard_transition.show(Instant::now());
        }
        BridgeEvent::EntryRating {
            container,
            entry_id,
            activity,
        } => {
            if app.container.as_deref() == Some(container.as_str())
                && let Some(game) = app.games.iter_mut().find(|game| game.id == entry_id)
            {
                game.rating = activity.rating();
            }
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
                .details
                .as_deref()
                .and_then(|id| app.game(id))
                .and_then(library::executable);

            if selected.as_deref() == Some(settings.executable.as_str()) {
                app.library.settings = Some(settings);
            }
        }
        // What failed to change is read again, as it was.
        BridgeEvent::ProgramFailed { reason, .. } => {
            app.notify(reason);

            if std::mem::take(&mut app.reload_after_pin)
                && let Some(bridge) = &app.bridge
            {
                bridge.reload_library();
            }
        }
        BridgeEvent::FoldersFailed { reason, .. } => {
            app.notify(reason);

            if std::mem::take(&mut app.scan_after_folders)
                && let Some(bridge) = &app.bridge
            {
                bridge.reload_library();
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

fn metadata_event(app: &mut App, update: MetadataUpdate) -> Task<Message> {
    match update {
        MetadataUpdate::Record {
            container,
            entry_id,
            generation,
            operation,
            result,
        } => {
            let Some(editor) = app.editor.as_mut().filter(|editor| {
                editor.game_id == entry_id && editor.draft.generation == generation
            }) else {
                return Task::none();
            };

            editor.busy = false;

            match result {
                Ok(reply) => {
                    let errors: Vec<&str> = reply["errors"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(serde_json::Value::as_str)
                        .collect();

                    editor.message = match editor.draft.accept_reply(&reply) {
                        Err(error) => Some(error),
                        Ok(()) if !errors.is_empty() => Some(errors.join("\n")),
                        Ok(()) if operation == "save" => Some(s().metadata.msg_saved.to_owned()),
                        Ok(()) if operation == "refresh" => {
                            Some(s().metadata.msg_refreshed_by_id.to_owned())
                        }
                        Ok(()) => None,
                    };

                    if operation != "get"
                        && let Some(bridge) = &app.bridge
                    {
                        bridge.load_entries(&container);
                    }
                }
                Err(reason) => editor.message = Some(reason),
            }
        }
        MetadataUpdate::Results {
            entry_id,
            generation,
            request,
            results,
            ..
        } => {
            if let Some(editor) = app.editor.as_mut().filter(|editor| {
                editor.game_id == entry_id && editor.draft.accepts_search(generation, request)
            }) {
                editor.accept_results(results);
            }
        }
        MetadataUpdate::SearchFailed {
            entry_id,
            generation,
            request,
            reason,
            ..
        } => {
            if let Some(editor) = app.editor.as_mut().filter(|editor| {
                editor.game_id == entry_id && editor.draft.accepts_search(generation, request)
            }) {
                editor.searching = false;
                editor.message = Some(reason);
            }
        }
        MetadataUpdate::Applied { container, .. } => {
            if let Some(bridge) = &app.bridge {
                bridge.load_entries(&container);
            }
            app.notify(s().state.metadata_updated);
        }
        MetadataUpdate::EntrySynced { container, entry } => {
            if app.container.as_deref() == Some(container.as_str()) {
                let mut game = entry.into_game(1);
                game.container = container;
                app.library.covers.remove(&game.id);

                match app.games.iter_mut().find(|known| known.id == game.id) {
                    Some(known) => *known = game,
                    None => app.games.push(game),
                }
            }
        }
        MetadataUpdate::Synced {
            container,
            matched,
            failed,
            automatic,
        } => {
            if !automatic {
                if let Some(bridge) = &app.bridge {
                    bridge.load_entries(&container);
                }
                app.notify(maltoria_core::i18n::fmt::notice_sync_done(
                    matched as usize,
                    failed as usize,
                ));
            }
        }
        MetadataUpdate::Failed { reason } => app.notify(reason),
        _ => {}
    }

    Task::none()
}

/// The details dialog, or the metadata editor in its place, while either is
/// open or closing.
fn details_dialog(app: &App, now: Instant) -> Option<material::Element<'_, Message>> {
    let game = app.game(app.details.as_deref()?)?;
    let alpha = app.details_transition.alpha(now);

    Some(match app.editor.as_ref() {
        Some(editor) if editor.game_id == game.id => metadata::view(
            editor,
            app.metadata_preferences
                .as_ref()
                .unwrap_or(&EMPTY_PREFERENCES),
            &app.library.pictures,
            alpha,
        ),
        _ => details::view(
            game,
            &app.runtime,
            &app.library.covers,
            &app.library.pictures,
            app.library.settings.as_ref(),
            alpha,
        ),
    })
}

static EMPTY_PREFERENCES: std::sync::LazyLock<MetadataPreferences> =
    std::sync::LazyLock::new(MetadataPreferences::default);

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
            appearance: &app.appearance,
        }),
    };

    let content = navigation::suite(&app.destinations, &app.navigation)
        .window_size(app.window_size)
        .view(Message::Navigate, page);

    let now = Instant::now();
    let dialog = details_dialog(app, now).unwrap_or_else(|| Space::new().into());
    let content = dialog::modal_animated_with(
        content,
        &app.details_transition,
        now,
        dialog,
        dialog::ModalOptions::default().margin(tokens::component::dialog::WINDOW_MARGIN),
    );

    let removing = app.removing.as_ref().map_or_else(
        || Space::new().into(),
        |(_, title)| removal::dialog(title, app.removing_transition.alpha(now)),
    );
    let content = dialog::modal_animated_with(
        content,
        &app.removing_transition,
        now,
        removing,
        dialog::ModalOptions::default().margin(tokens::component::dialog::WINDOW_MARGIN),
    );

    // A launch's clipboard question can come from the details dialog: it
    // goes over everything.
    let clipboard = app.clipboard.as_ref().map_or_else(
        || Space::new().into(),
        |prompt| playing::clipboard_dialog(prompt, app.clipboard_transition.alpha(now)),
    );
    let content = dialog::modal_animated_with(
        content,
        &app.clipboard_transition,
        now,
        clipboard,
        dialog::ModalOptions::default().margin(tokens::component::dialog::WINDOW_MARGIN),
    );

    let content = match &app.notice {
        Some(notice) if app.snackbar.is_active() => snackbar::host(
            content,
            &app.snackbar,
            now,
            notice.as_str(),
            s().common.close,
            Message::DismissNotice,
        ),
        _ => content,
    };

    pointer::track(app.theme.reveal_over(content, now))
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
