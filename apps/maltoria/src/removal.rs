//! Deleting a game from the library. A game added by its executable is
//! unpinned, and a game found in a folder has that folder taken out of the
//! library; the game's files stay where they are.

use std::path::Path;

use maltoria_core::bridge::FolderEdit;
use maltoria_core::bridge::dto::FoldersDto;
use maltoria_core::i18n::s;
use maltoria_core::model::Game;
use material::widget::dialog;
use material_ui_rs as material;

use crate::Message;

/// What deleting a game asks of the helper.
#[derive(Debug, Clone)]
pub enum Removal {
    /// A program added by its executable.
    Unpin { path: String, name: String },
    /// A folder the library found under one of its folders, or was given as
    /// a game.
    Folder(FolderEdit),
}

/// Whether `game` can be deleted: one the library neither pinned nor found
/// in a folder, such as a Steam app, cannot.
pub fn removable(game: &Game) -> bool {
    if game.is_folder {
        game.folder_path.is_some()
    } else {
        game.executable.is_some()
    }
}

/// How to delete `game`, given the library's folders.
pub fn of(game: &Game, folders: Option<&FoldersDto>) -> Option<Removal> {
    if game.is_folder {
        let folder = game.folder_path.as_deref()?;
        let path = folder.to_string_lossy().into_owned();
        let assigned = folders.is_some_and(|folders| {
            folders
                .assigned_folders
                .iter()
                .any(|assigned| Path::new(assigned) == folder)
        });

        return Some(Removal::Folder(if assigned {
            FolderEdit::Assign {
                path,
                assigned: false,
            }
        } else {
            FolderEdit::Exclude {
                path,
                excluded: true,
            }
        }));
    }

    let executable = game.executable.as_deref()?;

    Some(Removal::Unpin {
        path: executable.to_string_lossy().into_owned(),
        name: game.title.clone(),
    })
}

/// The question before a game is deleted.
pub fn dialog<'a>(title: &str, alpha: f32) -> material::Element<'a, Message> {
    let options = dialog::AlphaOptions::default().alpha(alpha);

    dialog::alert_with(
        s().app.remove_game_title(title),
        s().app.remove_game_detail,
        dialog::actions([
            dialog::action_button_with(s().common.cancel, Message::RemoveDismiss, options),
            dialog::action_button_with(s().common.delete, Message::RemoveConfirm, options),
        ]),
        dialog::AlertOptions::default().icon("delete").alpha(alpha),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn game() -> Game {
        let mut game = maltoria_core::model::demo_data()[0].clone();
        game.is_folder = false;
        game.executable = None;
        game.folder_path = None;
        game
    }

    #[test]
    fn a_program_is_unpinned() {
        let mut game = game();
        game.executable = Some(PathBuf::from("/Games/WA2/WA2_chs.exe"));

        assert!(removable(&game));
        assert!(matches!(
            of(&game, None),
            Some(Removal::Unpin { path, name })
                if path == "/Games/WA2/WA2_chs.exe" && name == game.title
        ));
    }

    #[test]
    fn a_found_folder_is_excluded() {
        let mut game = game();
        game.is_folder = true;
        game.folder_path = Some(PathBuf::from("/Games/WA2"));
        game.executable = Some(PathBuf::from("/Games/WA2/WA2_chs.exe"));

        assert!(removable(&game));
        assert!(matches!(
            of(&game, Some(&FoldersDto::default())),
            Some(Removal::Folder(FolderEdit::Exclude { path, excluded: true }))
                if path == "/Games/WA2"
        ));
    }

    #[test]
    fn an_assigned_folder_is_unassigned() {
        let mut game = game();
        game.is_folder = true;
        game.folder_path = Some(PathBuf::from("/Games/WA2"));
        let folders = FoldersDto {
            assigned_folders: vec!["/Games/WA2".to_owned()],
            ..FoldersDto::default()
        };

        assert!(matches!(
            of(&game, Some(&folders)),
            Some(Removal::Folder(FolderEdit::Assign { path, assigned: false }))
                if path == "/Games/WA2"
        ));
    }

    #[test]
    fn neither_a_program_nor_a_folder_stays() {
        let game = game();

        assert!(!removable(&game));
        assert!(of(&game, None).is_none());
    }
}
