//! What the interface remembers between launches: the one container every
//! game lives in, and the look.
//!
//! The helper's preferences belong to the Swift side and only take the keys its
//! schema lists, so these live in a small JSON file of the app's own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The file name under the app's configuration directory.
const FILE_NAME: &str = "maltoria-ui.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    /// The path of the container the library shows, once one was chosen.
    pub container: Option<String>,
    /// The seed color of the theme, by its index in `MaterialColor::ALL`.
    pub color: usize,
    /// Dark mode, or `None` to follow the system.
    pub dark_mode: Option<bool>,
}

impl Prefs {
    /// The preferences in `path`, or the defaults if there are none or they
    /// cannot be read.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes the preferences to `path`, creating its directory.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    pub fn load() -> Self {
        path()
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }

    /// Writes the preferences, logging rather than failing: losing them
    /// costs a choice, not a game.
    pub fn save(&self) {
        if let Some(path) = path()
            && let Err(error) = self.save_to(&path)
        {
            eprintln!("maltoria: could not save {}: {error}", path.display());
        }
    }
}

/// Where the preferences live: next to the helper's data on macOS, and in the
/// XDG configuration directory elsewhere.
pub fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);

    if cfg!(target_os = "macos") {
        return home.map(|home| {
            home.join("Library/Application Support/com.daydalek.Maltoria")
                .join(FILE_NAME)
        });
    }

    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home.map(|home| home.join(".config")))
        .map(|config| config.join("maltoria").join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_round_trip_through_their_file() {
        let directory = std::env::temp_dir().join(format!("maltoria-prefs-{}", std::process::id()));
        let path = directory.join("nested").join(FILE_NAME);
        let prefs = Prefs {
            container: Some("/bottles/Maltoria".into()),
            color: 3,
            dark_mode: Some(true),
        };

        prefs.save_to(&path).unwrap();
        assert_eq!(Prefs::load_from(&path), prefs);

        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn missing_or_broken_files_give_the_defaults() {
        let directory =
            std::env::temp_dir().join(format!("maltoria-broken-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(FILE_NAME);

        assert_eq!(Prefs::load_from(&path), Prefs::default());
        std::fs::write(&path, b"{ not json").unwrap();
        assert_eq!(Prefs::load_from(&path), Prefs::default());
        // Unknown keys and missing ones are fine.
        std::fs::write(&path, br#"{"color": 2, "later": true}"#).unwrap();
        assert_eq!(Prefs::load_from(&path).color, 2);

        std::fs::remove_dir_all(&directory).unwrap();
    }
}
