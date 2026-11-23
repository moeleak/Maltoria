//! Play time: today, this week and in all, the last thirty days, and the
//! games played the most.
//!
//! Days come from each game's daily journal, which the helper records in the
//! local time zone; totals include the cumulative time recorded before the
//! journal existed, which is never spread over invented days.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};
use iced::widget::canvas::{self, Frame, Geometry, Path};
use iced::widget::{Column, Row, container, text};
use iced::{Length, Point, Rectangle, Size, border, mouse};
use maltoria_core::i18n::{fmt, s};
use maltoria_core::model::Game;
use material::widget::page;
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;

/// The days the chart shows, today included.
pub const CHART_DAYS: usize = 30;
/// The games the ranking lists.
pub const RANKING_LENGTH: usize = 10;

/// The play time of a library, by day and by game.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Journal {
    /// Seconds played on each day.
    days: BTreeMap<NaiveDate, f64>,
    /// Seconds played in all, cumulative totals included.
    lifetime: u64,
    /// Games by their total play time, the most played first, as indices.
    ranking: Vec<(usize, u64)>,
}

impl Journal {
    /// The journal of `games` up to `today`; launchers are not games.
    pub fn build(games: &[Game], today: NaiveDate) -> Self {
        let mut journal = Self::default();

        for (index, game) in games
            .iter()
            .enumerate()
            .filter(|(_, game)| !game.is_launcher)
        {
            let mut journaled = 0.0;

            for (key, &seconds) in &game.daily_play_time {
                let Ok(date) = NaiveDate::parse_from_str(key, "%Y-%m-%d") else {
                    continue;
                };

                if date > today || !seconds.is_finite() || seconds <= 0.0 {
                    continue;
                }

                *journal.days.entry(date).or_default() += seconds;
                journaled += seconds;
            }

            // The cumulative total covers the journal, and the time before it.
            let total = game.play_time.unwrap_or(0).max(journaled.round() as u64);
            journal.lifetime = journal.lifetime.saturating_add(total);

            if total > 0 {
                journal.ranking.push((index, total));
            }
        }

        journal
            .ranking
            .sort_by(|(a, first), (b, second)| second.cmp(first).then(a.cmp(b)));

        journal
    }

    /// Seconds played on `date`.
    pub fn on(&self, date: NaiveDate) -> f64 {
        self.days.get(&date).copied().unwrap_or(0.0)
    }

    /// Seconds played on the `days` days up to `today`.
    pub fn last(&self, today: NaiveDate, days: usize) -> f64 {
        self.recent(today, days).map(|(_, seconds)| seconds).sum()
    }

    /// The days with play among the `days` days up to `today`.
    pub fn active_days(&self, today: NaiveDate, days: usize) -> usize {
        self.recent(today, days)
            .filter(|(_, seconds)| *seconds > 0.0)
            .count()
    }

    /// The `days` days up to `today`, the oldest first, with their seconds.
    pub fn recent(
        &self,
        today: NaiveDate,
        days: usize,
    ) -> impl Iterator<Item = (NaiveDate, f64)> + '_ {
        (0..days)
            .rev()
            .map(move |back| today - Duration::days(back as i64))
            .map(|date| (date, self.on(date)))
    }

    /// Seconds played in all.
    pub fn lifetime(&self) -> u64 {
        self.lifetime
    }

    /// The most played games, as indices into the library, with their seconds.
    pub fn ranking(&self) -> &[(usize, u64)] {
        &self.ranking
    }
}

/// `seconds` as the library writes durations, with a word for less than a minute.
pub fn duration(seconds: f64) -> String {
    let minutes = (seconds.max(0.0) / 60.0).floor() as u64;

    if seconds > 0.0 && minutes == 0 {
        s().statistics.under_one_minute.to_owned()
    } else {
        fmt::statistics_duration(minutes)
    }
}

pub fn view(games: &[Game], today: NaiveDate) -> material::Element<'_, Message> {
    let journal = Journal::build(games, today);
    let statistics = &s().statistics;
    let app = &s().app;

    let summary = Row::new()
        .push(stat(
            statistics.today,
            duration(journal.on(today)),
            statistics.today_sub,
        ))
        .push(stat(
            statistics.last_7_days,
            duration(journal.last(today, 7)),
            statistics.last_7_days_sub,
        ))
        .push(stat(
            statistics.active_days,
            statistics.statistics_active_days(journal.active_days(today, CHART_DAYS)),
            app.last_30_days,
        ))
        .push(stat(
            statistics.total_playtime,
            duration(journal.lifetime() as f64),
            statistics.total_playtime_sub,
        ))
        .spacing(16)
        .width(Length::Fill);

    let chart = canvas::Canvas::new(Chart {
        days: journal.recent(today, CHART_DAYS).collect(),
    })
    .width(Length::Fill)
    .height(Length::Fixed(CHART_HEIGHT));

    page::surface(
        page::header(app.activity_title, statistics.page_subtitle),
        page::sections([
            summary.into(),
            page::section(app.last_30_days, container(chart).width(Length::Fill)).into(),
            page::section(app.most_played, ranking(games, &journal)).into(),
        ]),
    )
    .into()
}

fn stat<'a>(title: &'a str, value: String, caption: &'a str) -> material::Element<'a, Message> {
    container(
        Column::new()
            .push(
                material::text::type_scale(title, tokens::typography::LABEL_LARGE)
                    .style(material::text::surface_variant),
            )
            .push(material::text::type_scale(
                value,
                tokens::typography::HEADLINE_SMALL,
            ))
            .push(
                material::text::type_scale(caption, tokens::typography::BODY_SMALL)
                    .style(material::text::surface_variant),
            )
            .spacing(4),
    )
    .width(Length::FillPortion(1))
    .into()
}

fn ranking<'a>(games: &'a [Game], journal: &Journal) -> material::Element<'a, Message> {
    let ranking = journal.ranking();

    let Some(&(_, most)) = ranking.first() else {
        return Column::new()
            .push(material::text::type_scale(
                s().statistics.empty_day_title,
                tokens::typography::TITLE_SMALL,
            ))
            .push(
                material::text::type_scale(
                    s().statistics.empty_day_desc,
                    tokens::typography::BODY_MEDIUM,
                )
                .style(material::text::surface_variant),
            )
            .spacing(4)
            .into();
    };

    ranking
        .iter()
        .take(RANKING_LENGTH)
        .filter_map(|&(index, seconds)| Some((games.get(index)?, seconds)))
        .fold(Column::new().spacing(16), |column, (game, seconds)| {
            let share = seconds as f32 / most.max(1) as f32;

            column.push(
                Column::new()
                    .push(
                        Row::new()
                            .push(text(game.title.as_str()).width(Length::Fill))
                            .push(
                                material::text::type_scale(
                                    duration(seconds as f64),
                                    tokens::typography::LABEL_LARGE,
                                )
                                .style(material::text::surface_variant),
                            ),
                    )
                    .push(share_bar(share))
                    .spacing(6),
            )
        })
        .into()
}

/// A flat bar `share` (0 to 1) of the way along its track.
fn share_bar<'a>(share: f32) -> material::Element<'a, Message> {
    const STEPS: u16 = 1000;
    let filled = (share.clamp(0.0, 1.0) * f32::from(STEPS)).round() as u16;

    let segment = |portion: u16, filled: bool| {
        iced::widget::container(iced::widget::Space::new())
            .width(Length::FillPortion(portion.max(1)))
            .height(Length::Fixed(6.0))
            .style(move |theme: &Theme| {
                let colors = theme.colors();
                let color = if filled {
                    colors.primary.color
                } else {
                    colors.surface.container.highest
                };

                iced::widget::container::Style {
                    background: Some(color.into()),
                    border: border::rounded(tokens::shape::CORNER_FULL),
                    ..iced::widget::container::Style::default()
                }
            })
    };

    let mut row = Row::new().spacing(4).width(Length::Fill);

    if filled > 0 {
        row = row.push(segment(filled, true));
    }

    if filled < STEPS {
        row = row.push(segment(STEPS - filled, false));
    }

    row.into()
}

const CHART_HEIGHT: f32 = 180.0;
/// The room under the bars for the dates.
const LABEL_HEIGHT: f32 = 20.0;
const BAR_GAP: f32 = 4.0;

/// Bars of the seconds played on each day, the oldest on the left, with the
/// date under the first of each week and under today.
#[derive(Debug)]
struct Chart {
    days: Vec<(NaiveDate, f64)>,
}

impl<Message> canvas::Program<Message, Theme> for Chart {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let colors = theme.colors();
        let mut frame = Frame::new(renderer, bounds.size());
        let plot = bar_area(bounds.size());
        let most = self
            .days
            .iter()
            .map(|(_, seconds)| *seconds)
            .fold(0.0, f64::max);

        for (index, rect) in bar_rects(plot, self.days.len()).enumerate() {
            let (date, seconds) = self.days[index];
            let height = bar_height(seconds, most, plot.height);
            let is_today = index + 1 == self.days.len();

            // A faint track keeps empty days visible.
            frame.fill(
                &Path::rounded_rectangle(rect.position(), rect.size(), border::radius(4.0)),
                colors.surface.container.highest,
            );

            if height > 0.0 {
                frame.fill(
                    &Path::rounded_rectangle(
                        Point::new(rect.x, rect.y + rect.height - height),
                        Size::new(rect.width, height),
                        border::radius(4.0),
                    ),
                    if is_today {
                        colors.tertiary.color
                    } else {
                        colors.primary.color
                    },
                );
            }

            if is_today || date.weekday() == chrono::Weekday::Mon {
                frame.fill_text(canvas::Text {
                    content: format!("{}/{}", date.month(), date.day()),
                    position: Point::new(rect.center_x(), plot.y + plot.height + 4.0),
                    color: colors.surface.text_variant,
                    size: 11.0.into(),
                    align_x: iced::widget::text::Alignment::Center,
                    ..canvas::Text::default()
                });
            }
        }

        vec![frame.into_geometry()]
    }
}

/// The part of a chart of `size` the bars take: all but the date labels.
fn bar_area(size: Size) -> Rectangle {
    Rectangle::new(
        Point::ORIGIN,
        Size::new(size.width, (size.height - LABEL_HEIGHT).max(0.0)),
    )
}

/// The tracks of `count` bars across `area`.
fn bar_rects(area: Rectangle, count: usize) -> impl Iterator<Item = Rectangle> {
    let slot = area.width / count.max(1) as f32;
    let width = (slot - BAR_GAP).max(1.0);

    (0..count).map(move |index| {
        Rectangle::new(
            Point::new(area.x + slot * index as f32 + (slot - width) / 2.0, area.y),
            Size::new(width, area.height),
        )
    })
}

/// The height of a bar for `seconds` when the most is `most`; any play shows.
fn bar_height(seconds: f64, most: f64, height: f32) -> f32 {
    if seconds <= 0.0 || most <= 0.0 {
        return 0.0;
    }

    ((seconds / most) as f32 * height).max(3.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn game(title: &str, total: Option<u64>, days: &[(&str, f64)]) -> Game {
        let mut game = maltoria_core::model::demo_data()[0].clone();
        game.title = title.into();
        game.is_launcher = false;
        game.play_time = total;
        game.daily_play_time = days
            .iter()
            .map(|(day, seconds)| ((*day).into(), *seconds))
            .collect();
        game
    }

    #[test]
    fn days_add_up_across_games_and_ignore_the_future() {
        let today = date("2026-11-23");
        let games = [
            game(
                "A",
                Some(7200),
                &[("2026-11-23", 600.0), ("2026-11-20", 1800.0)],
            ),
            game(
                "B",
                None,
                &[("2026-11-23", 300.0), ("2026-11-24", 999.0), ("bad", 5.0)],
            ),
        ];
        let journal = Journal::build(&games, today);

        assert_eq!(journal.on(today), 900.0);
        assert_eq!(journal.on(date("2026-11-24")), 0.0);
        assert_eq!(journal.last(today, 7), 2700.0);
        assert_eq!(journal.last(today, 3), 900.0);
        assert_eq!(journal.active_days(today, 7), 2);
    }

    #[test]
    fn totals_keep_the_time_before_the_journal() {
        let today = date("2026-11-23");
        let games = [
            // 2 hours in all, of which 40 minutes are in the journal.
            game("A", Some(7200), &[("2026-11-22", 2400.0)]),
            // A journal without a total still counts.
            game("B", None, &[("2026-11-22", 600.0)]),
            game("Unplayed", None, &[]),
        ];
        let journal = Journal::build(&games, today);

        assert_eq!(journal.lifetime(), 7800);
        assert_eq!(journal.ranking(), &[(0, 7200), (1, 600)]);
    }

    #[test]
    fn launchers_are_not_games() {
        let today = date("2026-11-23");
        let mut launcher = game("Steam", Some(99_999), &[("2026-11-23", 600.0)]);
        launcher.is_launcher = true;
        let journal = Journal::build(&[launcher], today);

        assert_eq!(journal, Journal::default());
    }

    #[test]
    fn recent_days_run_from_the_oldest_to_today() {
        let today = date("2026-11-23");
        let days: Vec<_> = Journal::default()
            .recent(today, 3)
            .map(|(day, _)| day)
            .collect();

        assert_eq!(days, [date("2026-11-21"), date("2026-11-22"), today]);
    }

    #[test]
    fn bars_share_the_width_and_any_play_shows() {
        let area = Rectangle::new(Point::ORIGIN, Size::new(300.0, 100.0));
        let bars: Vec<_> = bar_rects(area, 30).collect();

        assert_eq!(bars.len(), 30);
        assert_eq!(bars[0].width, 6.0);
        assert!((bars[29].x + bars[29].width - 298.0).abs() < 0.01);
        assert_eq!(bar_height(0.0, 100.0, 100.0), 0.0);
        assert_eq!(bar_height(1.0, 10_000.0, 100.0), 3.0);
        assert_eq!(bar_height(50.0, 100.0, 100.0), 50.0);
        assert_eq!(bar_area(Size::new(300.0, 180.0)).height, 160.0);
    }
}
