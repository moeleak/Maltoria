//! Covers, summaries and characters from the metadata platforms: synced for
//! the whole library, and searched and linked game by game in the editor.
//!
//! The helper does the fetching. A sync matches every game it can; the editor
//! links a game to a platform's work by its ID, which saving commits and the
//! helper then fetches. Opening the editor fetches the saved links again, in
//! the background. A local image can replace the cover at once.

use iced::widget::{Column, Row, container, text};
use iced::{Alignment, Length};
use maltoria_core::bridge::dto::{MetadataCandidate, MetadataSearchGroupDto};
use maltoria_core::i18n::s;
use maltoria_core::metadata_draft::{self, MetadataDraft};
use maltoria_core::model::Game;
use maltoria_core::onboarding::{self, MetadataPreferences};
use material::tokens;
use material::widget::button::{self, ChipVariant};
use material::widget::{dialog, image, text_input, tooltip};
use material_ui_rs as material;

use crate::Message;
use crate::details;

/// The image types a custom cover may be.
pub const COVER_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "heic", "webp"];

/// The sources a sync fetches from: the enabled ones, the default first.
pub fn sync_sources(preferences: &MetadataPreferences) -> Vec<String> {
    let mut sources = preferences.enabled_sources.clone();
    sources.sort_by_key(|source| source != &preferences.default_source);
    sources
}

/// Whether the library has games a sync could tell something about: scanned
/// folders known by nothing but their folder's name.
pub fn needs_sync(games: &[Game]) -> bool {
    games.iter().any(|game| {
        game.is_folder
            && game.studio.is_none()
            && game.description.is_none()
            && game.release.is_none()
    })
}

/// One game's metadata editor.
#[derive(Debug, Clone)]
pub struct Editor {
    pub game_id: String,
    pub draft: MetadataDraft,
    pub query: String,
    /// The platform ID or work URL typed to link directly.
    pub link: String,
    pub results: Vec<MetadataCandidate>,
    pub searching: bool,
    /// A save is running.
    pub busy: bool,
    /// The saved links are being fetched again.
    pub refreshing: bool,
    /// Opened from the game's details, which closing it returns to.
    pub returns_to_details: bool,
    pub message: Option<String>,
}

impl Editor {
    /// An editor for `game`, searching its title on the default source.
    pub fn new(game: &Game, preferences: &MetadataPreferences, returns_to_details: bool) -> Self {
        Self {
            game_id: game.id.clone(),
            draft: MetadataDraft::searching(&preferences.default_source),
            query: game.title.clone(),
            link: String::new(),
            results: Vec::new(),
            searching: false,
            busy: false,
            refreshing: false,
            returns_to_details,
            message: None,
        }
    }

    /// Takes the results of a search on the current source.
    pub fn accept_results(&mut self, groups: Vec<MetadataSearchGroupDto>) {
        let source = self.draft.search_source.clone();
        let mut errors = Vec::new();

        self.searching = false;
        self.results = groups
            .into_iter()
            .filter(|group| group.source == source)
            .flat_map(|group| {
                if let Some(error) = group.error {
                    errors.push(error);
                }

                group.games
            })
            .filter_map(MetadataCandidate::from_value)
            .collect();
        self.message = (!errors.is_empty()).then(|| errors.join("\n"));
    }

    /// Whether the result at `index` is the work the draft links.
    pub fn is_linked(&self, candidate: &MetadataCandidate) -> bool {
        candidate.raw["sourceID"].as_str().is_some()
            && self.draft.binding(&self.draft.search_source) == candidate.raw["sourceID"].as_str()
    }

    /// The linked platforms and their work IDs.
    pub fn bindings(&self) -> Vec<(&'static str, &str)> {
        metadata_draft::PROVIDERS
            .iter()
            .filter_map(|source| Some((*source, self.draft.binding(source)?)))
            .collect()
    }

    /// The sources to fetch again once the saved record is loaded: its
    /// links, if it has any.
    pub fn refresh_sources(&self) -> Option<Vec<String>> {
        let sources: Vec<String> = self
            .bindings()
            .into_iter()
            .map(|(source, _)| source.to_owned())
            .collect();

        (!sources.is_empty()).then_some(sources)
    }
}

/// The widest the editor dialog gets.
pub const MAX_WIDTH: f32 = 720.0;

/// The editor dialog.
pub fn view<'a>(
    editor: &'a Editor,
    preferences: &'a MetadataPreferences,
    images: &'a image::Cache<String>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let dialogs = &s().dialogs;
    let options = dialog::AlphaOptions::default().alpha(alpha);

    let mut body = Column::new()
        .push(search(editor, preferences, images, alpha))
        .push(linked(editor, alpha))
        .push(cover(editor, alpha))
        .spacing(28);

    let status = if editor.busy || !editor.draft.loaded {
        Some(s().common.processing.to_owned())
    } else {
        editor.message.clone()
    };

    if let Some(status) = status {
        body = body.push(details::muted(
            status,
            tokens::typography::BODY_SMALL,
            alpha,
        ));
    }

    let ready = editor.draft.loaded && !editor.busy;
    let (dismiss_icon, dismiss) = if editor.returns_to_details {
        ("arrow_back", s().common.back)
    } else {
        ("close", s().common.close)
    };

    dialog::content_with_max_width(
        dialogs.sheet_game_info,
        body.padding(iced::Padding {
            right: details::SCROLLBAR_ROOM,
            ..iced::Padding::ZERO
        }),
        dialog::actions([
            dialog::action_with_icon(dismiss_icon, dismiss, options)
                .on_press(Message::CloseEditor)
                .into(),
            dialog::action_with_icon("save", dialogs.save_metadata, options)
                .on_press_maybe((ready && editor.draft.is_dirty()).then_some(Message::EditorSave))
                .into(),
        ]),
        options,
        MAX_WIDTH,
    )
    .into()
}

fn heading<'a>(title: &'a str, tip: &'a str, alpha: f32) -> Column<'a, Message, material::Theme> {
    Column::new()
        .push(details::faded(
            title,
            tokens::typography::TITLE_SMALL,
            alpha,
        ))
        .push(details::muted(tip, tokens::typography::BODY_SMALL, alpha))
        .spacing(4)
}

fn search<'a>(
    editor: &'a Editor,
    preferences: &'a MetadataPreferences,
    images: &'a image::Cache<String>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let dialogs = &s().dialogs;

    if preferences.enabled_sources.is_empty() {
        return details::muted(
            dialogs.no_metadata_sources,
            tokens::typography::BODY_MEDIUM,
            alpha,
        )
        .into();
    }

    let sources = preferences
        .enabled_sources
        .iter()
        .fold(Row::new().spacing(8), |row, source| {
            let variant = if *source == editor.draft.search_source {
                ChipVariant::SelectedFilter
            } else {
                ChipVariant::Filter
            };

            row.push(
                button::chip(onboarding::source_label(source), variant)
                    .alpha(alpha)
                    .on_press(Message::EditorSource(source.clone())),
            )
        });

    let can_search = !editor.searching && !editor.query.trim().is_empty();

    let query = Row::new()
        .push(
            text_input::outlined(s().metadata.search_placeholder, &editor.query)
                .on_input(Message::EditorQuery)
                .on_submit(Message::EditorSearch)
                .alpha(alpha)
                .width(Length::Fill),
        )
        .push(icon_action(
            "search",
            if editor.searching {
                s().common.searching
            } else {
                s().common.search
            },
            can_search.then_some(Message::EditorSearch),
            alpha,
        ))
        .spacing(8)
        .align_y(Alignment::Center);

    let link = Row::new()
        .push(
            text_input::outlined(dialogs.link_by_id_url, &editor.link)
                .on_input(Message::EditorLinkInput)
                .on_submit(Message::EditorLinkById)
                .alpha(alpha)
                .width(Length::Fill),
        )
        .push(icon_action(
            "link",
            dialogs.link_to_draft,
            (!editor.link.trim().is_empty()).then_some(Message::EditorLinkById),
            alpha,
        ))
        .spacing(8)
        .align_y(Alignment::Center);

    let mut column = Column::new()
        .push(heading(
            dialogs.online_match_link,
            dialogs.search_or_link_tip,
            alpha,
        ))
        .push(sources)
        .push(query)
        .push(link)
        .spacing(12);

    for (index, candidate) in editor.results.iter().enumerate() {
        column = column.push(result(editor, images, index, candidate, alpha));
    }

    column.into()
}

const THUMBNAIL: (f32, f32) = (48.0, 64.0);

fn result<'a>(
    editor: &'a Editor,
    images: &'a image::Cache<String>,
    index: usize,
    candidate: &'a MetadataCandidate,
    alpha: f32,
) -> material::Element<'a, Message> {
    let corner = tokens::shape::CORNER_SMALL;
    let thumbnail: material::Element<'a, Message> = match &candidate.thumbnail {
        Some(path) => details::picture(images, &path.to_string_lossy(), corner, alpha),
        None => image::faded_placeholder(image::Placeholder::Missing, corner, alpha).into(),
    };

    let byline = [
        candidate.raw["sourceID"]
            .as_str()
            .map(|id| format!("ID {id}")),
        Some(candidate.developer.clone()),
        Some(candidate.release_date.clone()),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");

    let action = if editor.is_linked(candidate) {
        icon_action("check", s().dialogs.in_draft, None, alpha)
    } else {
        icon_action(
            "link",
            s().dialogs.link_to_draft,
            Some(Message::EditorLink(index)),
            alpha,
        )
    };

    Row::new()
        .push(
            container(thumbnail)
                .width(Length::Fixed(THUMBNAIL.0))
                .height(Length::Fixed(THUMBNAIL.1)),
        )
        .push(
            Column::new()
                .push(
                    details::faded(
                        candidate.title.as_str(),
                        tokens::typography::TITLE_SMALL,
                        alpha,
                    )
                    .wrapping(text::Wrapping::WordOrGlyph),
                )
                .push(details::muted(
                    byline,
                    tokens::typography::BODY_SMALL,
                    alpha,
                ))
                .push(details::muted(
                    clip(&candidate.summary, 120),
                    tokens::typography::BODY_SMALL,
                    alpha,
                ))
                .spacing(4)
                .width(Length::Fill),
        )
        .push(action)
        .spacing(16)
        .align_y(Alignment::Center)
        .padding([6, 0])
        .into()
}

/// The first `chars` characters of `text`, with an ellipsis if cut.
fn clip(text: &str, chars: usize) -> String {
    let mut clipped: String = text.chars().take(chars).collect();

    if text.chars().nth(chars).is_some() {
        clipped.push('…');
    }

    clipped
}

fn linked(editor: &Editor, alpha: f32) -> material::Element<'_, Message> {
    let dialogs = &s().dialogs;
    let bindings = editor.bindings();

    let mut column = Column::new()
        .push(details::faded(
            dialogs.currently_linked_draft,
            tokens::typography::TITLE_SMALL,
            alpha,
        ))
        .spacing(4);

    if bindings.is_empty() {
        column = column.push(details::muted(
            dialogs.not_linked_to_an_online_source,
            tokens::typography::BODY_MEDIUM,
            alpha,
        ));
    }

    for (source, id) in bindings {
        column = column.push(
            Row::new()
                .push(
                    details::faded(
                        format!("{} · {id}", onboarding::source_label(source)),
                        tokens::typography::BODY_LARGE,
                        alpha,
                    )
                    .width(Length::Fill),
                )
                .push(icon_action(
                    "link_off",
                    dialogs.unlink,
                    Some(Message::EditorUnlink(source.to_owned())),
                    alpha,
                ))
                .align_y(Alignment::Center),
        );
    }

    column.into()
}

/// A button that shows only its icon, naming itself in a tooltip, and fades
/// with the dialog.
fn icon_action<'a>(
    icon: &'a str,
    label: &'a str,
    on_press: Option<Message>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let button = button::icon_button(icon, button::IconButtonVariant::Standard)
        .alpha(alpha)
        .on_press_maybe(on_press);

    tooltip::plain(button, label, tooltip::Position::Top).into()
}

fn cover(editor: &Editor, alpha: f32) -> material::Element<'_, Message> {
    let dialogs = &s().dialogs;
    let options = dialog::AlphaOptions::default().alpha(alpha);

    Column::new()
        .push(heading(
            dialogs.custom_cover,
            dialogs.local_image_priority_tip,
            alpha,
        ))
        .push(
            Row::new()
                .push(
                    dialog::action_with_icon("image", dialogs.choose_local_image, options)
                        .on_press(Message::ChooseCover(editor.game_id.clone())),
                )
                .push(
                    dialog::action_with_icon(
                        "restart_alt",
                        dialogs.reset_to_default_cover,
                        options,
                    )
                    .on_press(Message::ResetCover(editor.game_id.clone())),
                )
                .spacing(8),
        )
        .spacing(12)
        .into()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn game(is_folder: bool, studio: Option<&str>) -> Game {
        let mut game = maltoria_core::model::demo_data()[0].clone();
        game.is_folder = is_folder;
        game.studio = studio.map(str::to_owned);
        game.description = None;
        game.release = None;
        game
    }

    #[test]
    fn syncs_start_from_the_default_source() {
        let preferences = MetadataPreferences {
            default_source: "vndb".into(),
            enabled_sources: vec!["bangumi".into(), "vndb".into(), "ymgal".into()],
            ..MetadataPreferences::default()
        };

        assert_eq!(sync_sources(&preferences), ["vndb", "bangumi", "ymgal"]);
    }

    #[test]
    fn only_unknown_scanned_folders_need_a_sync() {
        assert!(needs_sync(&[game(true, None)]));
        assert!(!needs_sync(&[game(true, Some("Leaf"))]));
        // Pinned programs and Steam apps carry their own names.
        assert!(!needs_sync(&[game(false, None)]));
    }

    #[test]
    fn results_keep_the_current_source_and_report_its_errors() {
        let mut editor = Editor::new(&game(true, None), &MetadataPreferences::default(), true);
        editor.draft.search_source = "vndb".into();

        editor.accept_results(vec![
            MetadataSearchGroupDto {
                source: "vndb".into(),
                error: None,
                games: vec![
                    json!({"title": "WHITE ALBUM2", "source": "vndb", "sourceID": "v211"}),
                    json!({"untitled": true}),
                ],
            },
            MetadataSearchGroupDto {
                source: "bangumi".into(),
                error: None,
                games: vec![json!({"title": "白色相簿2", "source": "bangumi", "sourceID": "51"})],
            },
        ]);

        assert_eq!(editor.results.len(), 1);
        assert_eq!(editor.results[0].title, "WHITE ALBUM2");
        assert_eq!(editor.message, None);
        assert!(!editor.is_linked(&editor.results[0]));

        editor
            .draft
            .bind("vndb", editor.results[0].raw.clone())
            .unwrap();
        assert!(editor.is_linked(&editor.results[0]));
        assert_eq!(editor.bindings(), [("vndb", "v211")]);

        editor.accept_results(vec![MetadataSearchGroupDto {
            source: "vndb".into(),
            error: Some("rate limited".into()),
            games: Vec::new(),
        }]);
        assert!(editor.results.is_empty());
        assert_eq!(editor.message.as_deref(), Some("rate limited"));
    }

    #[test]
    fn only_linked_records_are_fetched_again() {
        let mut editor = Editor::new(&game(true, None), &MetadataPreferences::default(), false);
        assert_eq!(editor.refresh_sources(), None);

        editor.draft.bind_id("vndb", "v211");
        editor.draft.bind_id("bangumi", "51");
        assert_eq!(
            editor.refresh_sources(),
            Some(vec!["bangumi".to_owned(), "vndb".to_owned()])
        );
    }

    #[test]
    fn long_summaries_are_clipped() {
        assert_eq!(clip("abc", 5), "abc");
        assert_eq!(clip("abcdef", 3), "abc…");
        assert_eq!(clip("白色相簿", 2), "白色…");
    }
}
