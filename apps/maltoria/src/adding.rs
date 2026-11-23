//! Adding a game by its executable. The helper finds metadata only for games
//! in folders, so the executable's folder becomes a game of its own, launched
//! by that executable; a folder that holds other games stays as it is, and
//! the executable is pinned instead.

use std::path::Path;

use maltoria_core::bridge::dto::FoldersDto;

/// What adding an executable asks of the helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adding {
    /// The executable's folder is the game; `assign` unless the library
    /// already knows it as one.
    Folder {
        folder: String,
        assign: bool,
        executable: String,
    },
    /// The executable alone, named after its file.
    Pin { executable: String, name: String },
}

/// How to add `executable`, given the library's folders and the home folder.
pub fn of(executable: &Path, folders: Option<&FoldersDto>, home: Option<&Path>) -> Adding {
    let path = executable.to_string_lossy().into_owned();
    let pin = || Adding::Pin {
        executable: path.clone(),
        name: executable
            .file_stem()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };

    let Some(folder) = executable
        .parent()
        .filter(|folder| folder.parent().is_some() && Some(*folder) != home)
    else {
        return pin();
    };

    let listed = |paths: Option<&Vec<String>>| {
        paths.is_some_and(|paths| paths.iter().any(|known| Path::new(known) == folder))
    };

    // Assigning a library folder would make all of its games one.
    if listed(folders.map(|folders| &folders.roots)) {
        return pin();
    }

    let known = listed(folders.map(|folders| &folders.assigned_folders))
        || listed(folders.map(|folders| &folders.known_folders));

    Adding::Folder {
        folder: folder.to_string_lossy().into_owned(),
        assign: !known,
        executable: path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = "/Users/me/Games/WHITE ALBUM2/WA2_chs.exe";

    #[test]
    fn the_folder_of_a_new_game_becomes_the_game() {
        assert_eq!(
            of(Path::new(EXE), Some(&FoldersDto::default()), None),
            Adding::Folder {
                folder: "/Users/me/Games/WHITE ALBUM2".into(),
                assign: true,
                executable: EXE.into(),
            }
        );
    }

    #[test]
    fn a_known_game_folder_only_takes_the_executable() {
        let folders = FoldersDto {
            known_folders: vec!["/Users/me/Games/WHITE ALBUM2".into()],
            ..FoldersDto::default()
        };

        assert!(matches!(
            of(Path::new(EXE), Some(&folders), None),
            Adding::Folder { assign: false, .. }
        ));
    }

    #[test]
    fn an_executable_in_a_library_folder_or_home_is_pinned() {
        let folders = FoldersDto {
            roots: vec!["/Users/me/Games".into()],
            ..FoldersDto::default()
        };
        let loose = "/Users/me/Games/game.exe";

        assert_eq!(
            of(Path::new(loose), Some(&folders), None),
            Adding::Pin {
                executable: loose.into(),
                name: "game".into(),
            }
        );
        assert!(matches!(
            of(
                Path::new("/Users/me/game.exe"),
                None,
                Some(Path::new("/Users/me"))
            ),
            Adding::Pin { .. }
        ));
        assert!(matches!(
            of(Path::new("/game.exe"), None, None),
            Adding::Pin { .. }
        ));
    }
}
