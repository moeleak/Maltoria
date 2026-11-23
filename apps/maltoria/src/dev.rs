//! Developer snapshots, to see the interface as it renders on another machine.
//!
//! With `MALTORIA_DEV_SNAPSHOTS` naming a directory, the app follows the
//! requests written to `request` in it, one per line, then deletes the file:
//!
//! - `page library`, `page activity`, `page settings` opens a page;
//! - `select <title>` opens the details of the first game whose title
//!   contains `<title>`; `close` closes them;
//! - `shot <name>` saves the window as `<name>.png`;
//! - `dismiss <name> <ms>` closes the details and saves the window `<ms>`
//!   milliseconds into their fade.
//!
//! Nothing of this runs without the variable.

use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::Subscription;
use iced::futures::channel::mpsc;
use iced::futures::{StreamExt, stream};

use crate::{Message, Page};

const VARIABLE: &str = "MALTORIA_DEV_SNAPSHOTS";
const REQUEST: &str = "request";
const POLL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Page(Page),
    Select(String),
    Close,
    Shot(String),
    /// Closes the details, and saves the window `after` into the fade.
    Dismiss {
        name: String,
        after: Duration,
    },
}

/// The snapshot directory, if snapshots are on.
pub fn directory() -> Option<PathBuf> {
    std::env::var_os(VARIABLE)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// A poll for requests every half second, while snapshots are on.
pub fn requests() -> Subscription<Message> {
    if directory().is_none() {
        return Subscription::none();
    }

    Subscription::run(|| {
        let (sender, receiver) = mpsc::unbounded();

        std::thread::spawn(move || {
            while sender.unbounded_send(()).is_ok() {
                std::thread::sleep(POLL);
            }
        });

        receiver.map(|()| Message::DevPoll).chain(stream::pending())
    })
}

/// The requests waiting in `directory`, which are taken from it.
pub fn take(directory: &Path) -> Vec<Request> {
    let path = directory.join(REQUEST);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let _ = std::fs::remove_file(&path);

    text.lines().filter_map(parse).collect()
}

pub fn parse(line: &str) -> Option<Request> {
    let (command, argument) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
    let argument = argument.trim();

    match command {
        "page" => match argument {
            "library" => Some(Request::Page(Page::Library)),
            "activity" => Some(Request::Page(Page::Activity)),
            "settings" => Some(Request::Page(Page::Settings)),
            _ => None,
        },
        "select" if !argument.is_empty() => Some(Request::Select(argument.to_owned())),
        "close" => Some(Request::Close),
        "dismiss" => {
            let (name, after) = argument.split_once(' ')?;
            let after = Duration::from_millis(after.trim().parse().ok()?);

            is_name(name).then(|| Request::Dismiss {
                name: name.to_owned(),
                after,
            })
        }
        "shot" if is_name(argument) => Some(Request::Shot(argument.to_owned())),
        _ => None,
    }
}

/// Snapshot names stay inside the directory.
fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Saves `screenshot` as `<name>.png` in `directory`.
pub fn save(
    directory: &Path,
    name: &str,
    screenshot: &iced::window::Screenshot,
) -> Result<(), String> {
    let image = image::RgbaImage::from_raw(
        screenshot.size.width,
        screenshot.size.height,
        screenshot.rgba.to_vec(),
    )
    .ok_or("the screenshot's size does not match its pixels")?;

    image
        .save(directory.join(format!("{name}.png")))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_parse_pages_selections_and_shots() {
        assert_eq!(parse("page settings"), Some(Request::Page(Page::Settings)));
        assert_eq!(
            parse("  select  WHITE ALBUM2 "),
            Some(Request::Select("WHITE ALBUM2".into()))
        );
        assert_eq!(parse("close"), Some(Request::Close));
        assert_eq!(
            parse("shot details-1"),
            Some(Request::Shot("details-1".into()))
        );
        assert_eq!(parse("shot ../escape"), None);
        assert_eq!(
            parse("dismiss fade 80"),
            Some(Request::Dismiss {
                name: "fade".into(),
                after: Duration::from_millis(80)
            })
        );
        assert_eq!(parse("dismiss fade soon"), None);
        assert_eq!(parse("page elsewhere"), None);
        assert_eq!(parse("select"), None);
        assert_eq!(parse(""), None);
    }
}
