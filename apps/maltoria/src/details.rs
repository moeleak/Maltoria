//! A game's details, as a Material 3 dialog over the library: its cover beside
//! how much it was played, then its summary and its characters.
//!
//! The dialog fades in as it grows, as the dialogs of `material-ui-rs` do:
//! every part of it takes the transition's `alpha`.

use iced::widget::{Column, Row, Space, Stack, container, mouse_area, scrollable, sensor, text};
use iced::{Alignment, Length};
use maltoria_core::bridge::dto::CharacterDto;
use maltoria_core::i18n::s;
use maltoria_core::library_view::next_rating;
use maltoria_core::model::{Game, format_last_played, format_play_time};
use maltoria_core::program_settings::ProgramSettingsDto;
use maltoria_core::runtime::RuntimeTable;
use material::tokens::typography::TypeScale;
use material::widget::{dialog, grid, image, select};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;
use crate::library::{self, LocaleOption};

const COVER_WIDTH: f32 = 148.0;
const CHARACTER_WIDTH: f32 = 112.0;
const STAR_SIZE: f32 = 20.0;

/// The widest the details dialog gets.
pub const MAX_WIDTH: f32 = 820.0;
/// The space kept clear beside a dialog's body for its scrollbar.
pub const SCROLLBAR_ROOM: f32 = 16.0;

/// The style of a scrollbar in a dialog, faded with it by `alpha`.
pub fn faded_scrollbar(theme: &Theme, status: scrollable::Status, alpha: f32) -> scrollable::Style {
    let mut style = material::style::scrollable::default(theme, status);
    // The dialog owns the surface; a scrollable's own would outlast its fade.
    style.container = iced::widget::container::Style::default();
    style.gap = style.gap.map(|background| background.scale_alpha(alpha));

    for rail in [&mut style.vertical_rail, &mut style.horizontal_rail] {
        rail.background = rail
            .background
            .map(|background| background.scale_alpha(alpha));
        rail.border.color = rail.border.color.scale_alpha(alpha);
        rail.scroller.background = rail.scroller.background.scale_alpha(alpha);
        rail.scroller.border.color = rail.scroller.border.color.scale_alpha(alpha);
    }

    style
}

/// `content` in `scale`, its color the surface's text, faded by `alpha`.
pub fn faded<'a>(
    content: impl text::IntoFragment<'a>,
    scale: TypeScale,
    alpha: f32,
) -> iced::widget::Text<'a, Theme> {
    material::text::type_scale(content, scale).style(move |theme: &Theme| text::Style {
        color: Some(theme.colors().surface.text.scale_alpha(alpha)),
    })
}

/// `content` in `scale`, in the surface's secondary text color, faded by `alpha`.
pub fn muted<'a>(
    content: impl text::IntoFragment<'a>,
    scale: TypeScale,
    alpha: f32,
) -> iced::widget::Text<'a, Theme> {
    material::text::type_scale(content, scale).style(move |theme: &Theme| text::Style {
        color: Some(theme.colors().surface.text_variant.scale_alpha(alpha)),
    })
}

/// `game`'s cover from `covers`, at the library's ratio and faded by `alpha`,
/// loaded when it comes into view.
pub fn cover<'a>(
    game: &'a Game,
    covers: &'a image::Cache<String>,
    corner: f32,
    alpha: f32,
) -> material::Element<'a, Message> {
    let art: material::Element<'a, Message> = match covers.status(&game.id) {
        Some(image::Status::Ready(handle)) => {
            image::shaped(handle.clone(), corner).opacity(alpha).into()
        }
        Some(status) if game.art.is_some() => {
            image::faded_placeholder(status.into(), corner, alpha).into()
        }
        _ if game.art.is_some() => {
            image::faded_placeholder(image::Placeholder::Loading, corner, alpha).into()
        }
        _ => image::faded_placeholder(image::Placeholder::Missing, corner, alpha).into(),
    };

    let id = game.id.clone();

    sensor(grid::aspect_ratio(art, library::COVER_RATIO))
        .key(library::cover_key(game, covers))
        .on_show(move |_| Message::CoverShown(id.clone()))
        .into()
}

/// The image file at `path` from `pictures`, at 3:4 and faded by `alpha`,
/// decoded in the background when it comes into view.
pub fn picture<'a>(
    pictures: &'a image::Cache<String>,
    path: &str,
    corner: f32,
    alpha: f32,
) -> material::Element<'a, Message> {
    let art: material::Element<'a, Message> = match pictures.status(&path.to_owned()) {
        Some(image::Status::Ready(handle)) => {
            image::shaped(handle.clone(), corner).opacity(alpha).into()
        }
        Some(status) => image::faded_placeholder(status.into(), corner, alpha).into(),
        None => image::faded_placeholder(image::Placeholder::Loading, corner, alpha).into(),
    };

    let path = path.to_owned();
    let cached = pictures.status(&path).is_some();

    sensor(grid::aspect_ratio(art, library::COVER_RATIO))
        .key((path.clone(), cached))
        .on_show(move |_| Message::ImageShown(path.clone()))
        .into()
}

/// The details dialog of `game`.
pub fn view<'a>(
    game: &'a Game,
    runtime: &'a RuntimeTable,
    covers: &'a image::Cache<String>,
    pictures: &'a image::Cache<String>,
    settings: Option<&'a ProgramSettingsDto>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let mut body = Column::new()
        .push(hero(game, covers, settings, alpha))
        .spacing(24);

    if let Some(summary) = game.description.as_deref() {
        let source = game
            .metadata_sources
            .get("summary")
            .map(|source| muted(source.as_str(), tokens::typography::LABEL_MEDIUM, alpha));

        let mut title = Row::new().push(
            faded(s().library.summary, tokens::typography::TITLE_SMALL, alpha).width(Length::Fill),
        );

        if let Some(source) = source {
            title = title.push(source);
        }

        body = body.push(
            Column::new()
                .push(title)
                .push(muted(summary, tokens::typography::BODY_MEDIUM, alpha))
                .spacing(8),
        );
    }

    if !game.characters.is_empty() {
        body = body.push(characters(game, pictures, alpha));
    }

    let options = dialog::AlphaOptions::default().alpha(alpha);
    let primary = library::primary_action(runtime, game);

    dialog::content_with_max_width(
        game.title.as_str(),
        // Room on the right for the dialog's scrollbar.
        body.padding(iced::Padding {
            right: SCROLLBAR_ROOM,
            ..iced::Padding::ZERO
        }),
        dialog::actions([
            dialog::action_with_icon("edit", s().library.game_info_cover_ellipsis, options)
                .on_press(Message::OpenEditor(game.id.clone()))
                .into(),
            dialog::action_with_icon("close", s().common.close, options)
                .on_press(Message::CloseDetails)
                .into(),
            dialog::action_with_icon(library::action_icon(&primary), primary.label, options)
                .on_press_maybe(library::action_message(primary.action, game))
                .into(),
        ]),
        options,
        MAX_WIDTH,
    )
    .into()
}

fn hero<'a>(
    game: &'a Game,
    covers: &'a image::Cache<String>,
    settings: Option<&'a ProgramSettingsDto>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let mut info = Column::new().spacing(10).width(Length::Fill);

    let byline: Vec<String> = [game.studio_name(), game.release.clone()]
        .into_iter()
        .flatten()
        .collect();

    if !byline.is_empty() {
        info = info.push(muted(
            byline.join(" · "),
            tokens::typography::BODY_MEDIUM,
            alpha,
        ));
    }

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
        info = info.push(
            Column::new()
                .push(muted(label, tokens::typography::LABEL_MEDIUM, alpha))
                .push(faded(value, tokens::typography::TITLE_MEDIUM, alpha))
                .spacing(2),
        );
    }

    info = info.push(rating(game, alpha));

    if let Some(settings) = settings {
        info = info.push(
            Column::new()
                .push(muted(
                    s().program_settings.locale_label,
                    tokens::typography::LABEL_MEDIUM,
                    alpha,
                ))
                .push(
                    select::outlined(
                        library::LOCALES.map(LocaleOption),
                        Some(LocaleOption::of(&settings.locale)),
                        |LocaleOption(locale)| Message::SetLocale(locale.to_owned()),
                    )
                    .alpha(alpha)
                    .width(Length::Fill),
                )
                .spacing(6),
        );
    }

    Row::new()
        .push(
            container(cover(game, covers, tokens::shape::CORNER_MEDIUM, alpha))
                .width(Length::Fixed(COVER_WIDTH)),
        )
        .push(info)
        .spacing(20)
        .into()
}

/// Five stars of two halves each: the rating is out of ten.
fn rating(game: &Game, alpha: f32) -> material::Element<'_, Message> {
    let current = game.rating;
    let mut stars = Row::new().spacing(2);

    for slot in 0..5u8 {
        let covered = current.map_or(0, |rating| rating.saturating_sub(slot * 2).min(2));
        let star = |filled: bool| {
            material::fonts::filled_icon("star", STAR_SIZE).style(move |theme: &Theme| {
                let colors = theme.colors();
                let color = if filled {
                    colors.primary.color
                } else {
                    colors.surface.container.highest
                };

                text::Style {
                    color: Some(color.scale_alpha(alpha)),
                }
            })
        };

        let half = |points: u8| {
            mouse_area(Space::new().width(STAR_SIZE / 2.0).height(STAR_SIZE))
                .on_press(Message::Rate(game.id.clone(), next_rating(current, points)))
                .interaction(iced::mouse::Interaction::Pointer)
        };

        stars = stars.push(
            Stack::new()
                .push(star(false))
                .push(
                    container(star(true))
                        .width(Length::Fixed(STAR_SIZE * f32::from(covered) / 2.0))
                        .clip(true),
                )
                .push(Row::new().push(half(slot * 2 + 1)).push(half(slot * 2 + 2))),
        );
    }

    let label = current.map_or_else(
        || s().library.unrated.to_owned(),
        |rating| format!("{rating} / 10"),
    );

    Row::new()
        .push(stars)
        .push(muted(label, tokens::typography::BODY_SMALL, alpha))
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
}

fn characters<'a>(
    game: &'a Game,
    images: &'a image::Cache<String>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let mut ordered: Vec<&CharacterDto> = game.characters.iter().collect();
    // Characters with portraits first, as the GPUI app shows them.
    ordered.sort_by_key(|character| !game.character_images.contains_key(&character.id));

    let cards = ordered
        .into_iter()
        .fold(Row::new().spacing(12), |row, character| {
            row.push(character_card(
                character,
                game.character_images.get(&character.id),
                images,
                alpha,
            ))
        });

    Column::new()
        .push(
            Row::new()
                .push(faded(
                    s().library.characters_title,
                    tokens::typography::TITLE_SMALL,
                    alpha,
                ))
                .push(muted(
                    game.characters.len().to_string(),
                    tokens::typography::LABEL_MEDIUM,
                    alpha,
                ))
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .push(
            scrollable(cards)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::default().spacing(4),
                ))
                .style(move |theme, status| faded_scrollbar(theme, status, alpha)),
        )
        .spacing(8)
        .into()
}

fn character_card<'a>(
    character: &'a CharacterDto,
    portrait: Option<&'a String>,
    images: &'a image::Cache<String>,
    alpha: f32,
) -> material::Element<'a, Message> {
    let corner = tokens::shape::CORNER_MEDIUM;
    let portrait: material::Element<'a, Message> = match portrait {
        Some(path) => picture(images, path, corner, alpha),
        None => grid::aspect_ratio(
            image::faded_placeholder(image::Placeholder::Missing, corner, alpha),
            library::COVER_RATIO,
        )
        .into(),
    };

    let mut column = Column::new()
        .push(portrait)
        .push(
            faded(
                character.name.as_str(),
                tokens::typography::LABEL_LARGE,
                alpha,
            )
            .wrapping(text::Wrapping::WordOrGlyph),
        )
        .spacing(4)
        .width(Length::Fixed(CHARACTER_WIDTH));

    let original = character
        .original_name
        .as_deref()
        .filter(|original| *original != character.name);

    for line in [original, character.voice_actor.as_deref()]
        .into_iter()
        .flatten()
        .filter(|line| !line.trim().is_empty())
    {
        column = column.push(
            muted(line, tokens::typography::BODY_SMALL, alpha)
                .wrapping(text::Wrapping::WordOrGlyph),
        );
    }

    column.into()
}
