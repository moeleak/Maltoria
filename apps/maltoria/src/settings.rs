//! Settings: the Wine runtime, the container, and the look.
//!
//! Only what a visual novel needs is here; the container's Wine tools cover
//! the rest.

use iced::widget::{Column, Row, Space, button as iced_button, container};
use iced::{Alignment, Length};
use maltoria_core::bridge::dto::{Container, SetupStatusDto};
use maltoria_core::i18n::s;
use material::widget::button::{self, ButtonVariant};
use material::widget::theme_picker::MaterialColor;
use material::widget::{page, progress_bar, segmented_button, toggler};
use material::{Theme, tokens};
use material_ui_rs as material;

use crate::Message;

/// The runtime installer's progress, while it runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Install {
    pub stage: Option<String>,
    /// Bytes received and expected while the archive downloads.
    pub progress: Option<(u64, u64)>,
}

/// The appearance the interface follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn from_dark_mode(dark_mode: Option<bool>) -> Self {
        match dark_mode {
            None => Self::System,
            Some(false) => Self::Light,
            Some(true) => Self::Dark,
        }
    }

    pub fn dark_mode(self) -> Option<bool> {
        match self {
            Self::System => None,
            Self::Light => Some(false),
            Self::Dark => Some(true),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::System => s().app.appearance_system,
            Self::Light => s().theme.light,
            Self::Dark => s().theme.dark,
        }
    }
}

/// The container's cap on frames per second, as its `frameRateLimit` stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRateLimit {
    Off,
    Fps30,
    Fps60,
}

impl FrameRateLimit {
    pub const ALL: [Self; 3] = [Self::Off, Self::Fps30, Self::Fps60];

    /// The value the configuration stores.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fps30 => "30",
            Self::Fps60 => "60",
        }
    }

    pub fn from_wire(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|limit| limit.wire() == value)
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|limit| *limit == self)
            .unwrap_or(0)
    }

    fn label(self) -> &'static str {
        let fallback = match self {
            Self::Off => "Unlimited",
            Self::Fps30 => "30 fps",
            Self::Fps60 => "60 fps",
        };
        maltoria_core::i18n::config::option_title("frameRateLimit", self.wire(), fallback)
    }
}

/// What the settings page shows.
pub struct View<'a> {
    pub setup: Option<&'a SetupStatusDto>,
    pub install: Option<&'a Install>,
    pub container: Option<&'a Container>,
    /// The container's Metal HUD setting, once its configuration arrived;
    /// `None` also where the platform has no Metal.
    pub metal_hud: Option<bool>,
    /// The container's frame rate limit, once its configuration arrived;
    /// `None` also where the bridge has no such setting.
    pub frame_rate_limit: Option<FrameRateLimit>,
    /// The frame rate limit's segmented button, whose selection animates.
    pub frame_rate: &'a segmented_button::State,
    pub color: MaterialColor,
    /// The appearance's segmented button, whose selection animates.
    pub appearance: &'a segmented_button::State,
}

pub fn view(settings: View<'_>) -> material::Element<'_, Message> {
    let app = &s().app;

    let mut sections: Vec<material::Element<'_, Message>> = vec![
        page::section(
            s().settings.runtime_title,
            runtime(settings.setup, settings.install),
        )
        .into(),
        page::section(
            app.container_section,
            container_section(
                settings.container,
                settings.metal_hud,
                settings.frame_rate_limit.map(|_| settings.frame_rate),
            ),
        )
        .into(),
    ];

    sections.push(
        page::section(
            s().settings.appearance_label,
            appearance(settings.color, settings.appearance),
        )
        .into(),
    );
    sections.push(page::section(s().dialogs.about_title, about()).into());

    page::surface(
        page::header(app.settings_title, app.settings_subtitle),
        page::sections(sections),
    )
    .into()
}

fn row<'a>(
    title: impl Into<String>,
    detail: impl Into<String>,
    trailing: impl Into<material::Element<'a, Message>>,
) -> material::Element<'a, Message> {
    let detail = detail.into();
    let mut text = Column::new()
        .push(material::text::type_scale(
            title.into(),
            tokens::typography::BODY_LARGE,
        ))
        .spacing(2)
        .width(Length::Fill);

    // An empty line would pull the title above the trailing control's center.
    if !detail.is_empty() {
        text = text.push(
            material::text::type_scale(detail, tokens::typography::BODY_MEDIUM)
                .style(material::text::surface_variant),
        );
    }

    Row::new()
        .push(text)
        .push(trailing)
        .spacing(16)
        .align_y(Alignment::Center)
        .into()
}

fn runtime<'a>(
    setup: Option<&'a SetupStatusDto>,
    install: Option<&'a Install>,
) -> material::Element<'a, Message> {
    let app = &s().app;

    if let Some(install) = install {
        let fraction = install
            .progress
            .filter(|(_, total)| *total > 0)
            .map(|(received, total)| received as f32 / total as f32);
        let bar = match fraction {
            Some(progress) => {
                progress_bar::linear(progress_bar::LinearProgressMode::determinate(progress, 0.0))
            }
            None => progress_bar::linear(progress_bar::LinearProgressMode::indeterminate(0.0)),
        };

        return container(
            Column::new()
                .push(row(
                    app.runtime_installing,
                    install.stage.clone().unwrap_or_default(),
                    Space::new(),
                ))
                .push(bar)
                .spacing(12),
        )
        .width(Length::Fill)
        .into();
    }

    let installed = setup.is_some_and(|setup| setup.managed_installed.unwrap_or(setup.installed));
    let version = setup.and_then(|setup| setup.version.clone());

    let (detail, action): (String, material::Element<'a, Message>) = match (setup, installed) {
        (None, _) => (s().common.loading.to_owned(), Space::new().into()),
        (Some(_), true) => (
            version.map_or_else(
                || s().common.installed.to_owned(),
                |version| s().settings.runtime_installed_version(&version),
            ),
            button::button(s().settings.check_updates, ButtonVariant::Text)
                .on_press(Message::CheckRuntimeUpdate)
                .into(),
        ),
        (Some(_), false) => (
            s().common.not_installed.to_owned(),
            button::button(app.runtime_install, ButtonVariant::Filled)
                .on_press(Message::InstallRuntime)
                .into(),
        ),
    };

    container(row(app.runtime_name, detail, action))
        .width(Length::Fill)
        .into()
}

fn container_section<'a>(
    current: Option<&'a Container>,
    metal_hud: Option<bool>,
    frame_rate: Option<&segmented_button::State>,
) -> material::Element<'a, Message> {
    let app = &s().app;

    let Some(current) = current else {
        return row(app.container_none, app.container_none_detail, Space::new());
    };

    let tool = |label: &'static str, tool: &'static str| {
        button::button(label, ButtonVariant::Outlined).on_press(Message::OpenTool(tool))
    };

    let mut section = Column::new()
        .push(row(
            current.name.as_str(),
            maltoria_core::create_bottle::display_bottle_location(&current.path),
            button::icon_button("folder_open", button::IconButtonVariant::Standard)
                .on_press(Message::RevealContainer),
        ))
        .push(
            Row::new()
                .push(tool(s().bottle.wine_config, "winecfg"))
                .push(tool(s().bottle.regedit, "regedit"))
                .push(tool(s().bottle.control_panel, "control"))
                .push(
                    button::button(app.install_fonts, ButtonVariant::Text)
                        .on_press(Message::InstallFonts),
                )
                .spacing(8)
                .wrap(),
        )
        .spacing(16);

    if let Some(enabled) = metal_hud {
        section = section.push(row(
            maltoria_core::i18n::config::field_title("", "metalHud", "Metal HUD"),
            app.metal_hud_detail,
            toggler::control(enabled).on_toggle(Message::SetMetalHud),
        ));
    }

    if let Some(state) = frame_rate {
        let limits = segmented_button::group(segmented_button::animated_label_actions(
            state,
            FrameRateLimit::ALL.map(|limit| (limit.label(), Message::SetFrameRateLimit(limit))),
        ));
        section = section.push(row(
            maltoria_core::i18n::config::field_title("", "frameRateLimit", "Frame Rate Limit"),
            app.frame_rate_limit_detail,
            limits,
        ));
    }

    section.into()
}

fn appearance(
    selected: MaterialColor,
    appearance: &segmented_button::State,
) -> material::Element<'static, Message> {
    let app = &s().app;

    let modes = segmented_button::group(segmented_button::animated_label_actions(
        appearance,
        Appearance::ALL.map(|mode| (mode.label(), Message::SetAppearance(mode))),
    ));

    let swatches = MaterialColor::ALL
        .iter()
        .fold(Row::new().spacing(12), |row, &color| {
            row.push(swatch(color, color == selected))
        });

    container(
        Column::new()
            .push(row(app.appearance_mode, "", modes))
            .push(row(app.appearance_color, "", swatches))
            .spacing(16),
    )
    .width(Length::Fill)
    .into()
}

const SWATCH_SIZE: f32 = 32.0;

fn swatch(color: MaterialColor, selected: bool) -> material::Element<'static, Message> {
    let fill = color.color_scheme(false).primary.color;
    let mark: material::Element<'static, Message> = if selected {
        material::fonts::icon("check", 20.0)
            .style(move |_: &Theme| iced::widget::text::Style {
                color: Some(color.color_scheme(false).primary.text),
            })
            .into()
    } else {
        Space::new().into()
    };

    iced_button(
        container(mark)
            .center_x(Length::Fixed(SWATCH_SIZE))
            .center_y(Length::Fixed(SWATCH_SIZE)),
    )
    .padding(0)
    .style(move |theme: &Theme, status| iced_button::Style {
        background: Some(fill.into()),
        border: iced::border::rounded(tokens::shape::CORNER_FULL)
            .width(if selected { 2.0 } else { 0.0 })
            .color(
                if matches!(status, iced_button::Status::Hovered) || selected {
                    theme.colors().surface.text
                } else {
                    fill
                },
            ),
        ..iced_button::Style::default()
    })
    .on_press(Message::SelectColor(color))
    .into()
}

fn about() -> material::Element<'static, Message> {
    let dialogs = &s().dialogs;
    let tagline = if cfg!(target_os = "macos") {
        dialogs.about_tagline_mac
    } else {
        dialogs.about_tagline_linux
    };

    container(
        Column::new()
            .push(row("Maltoria", tagline, Space::new()))
            .push(row(
                dialogs.about_version,
                env!("CARGO_PKG_VERSION"),
                Space::new(),
            ))
            .spacing(16),
    )
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_follows_the_remembered_dark_mode() {
        for appearance in Appearance::ALL {
            assert_eq!(
                Appearance::from_dark_mode(appearance.dark_mode()),
                appearance
            );
        }

        assert_eq!(Appearance::from_dark_mode(None), Appearance::System);
    }

    #[test]
    fn frame_rate_limit_reads_back_what_it_stores() {
        for (index, limit) in FrameRateLimit::ALL.into_iter().enumerate() {
            assert_eq!(FrameRateLimit::from_wire(limit.wire()), Some(limit));
            assert_eq!(limit.index(), index);
        }

        // A value this interface has no segment for is not shown as one.
        assert_eq!(FrameRateLimit::from_wire("45"), None);
    }
}
