//! The Wine runtime a build carries, installed where the helper runs it.
//!
//! The helper runs games only with the runtime in Maltoria's Application
//! Support, which has to be writable, as what it deploys (GPTK) goes into the
//! runtime. A build may carry a runtime of its own, such as one built for an
//! older macOS than upstream's, by naming its directory in
//! `MALTORIA_WINE_RUNTIME` when it is compiled. That runtime is copied there
//! before the helper starts, whenever the installed one is another version.

use std::fs;
use std::io;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};

/// The file whose contents name the version a runtime is.
const VERSION_FILE: &str = "WhiskyWineVersion.plist";

/// Installs the runtime this build carries, if it carries one. A failure is
/// reported, and the helper runs with whatever runtime is installed.
pub fn install_carried() {
    let Some(runtime) = option_env!("MALTORIA_WINE_RUNTIME").map(Path::new) else {
        return;
    };
    let Some(target) = target() else {
        return;
    };

    if let Err(error) = install(runtime, &target) {
        eprintln!(
            "maltoria: could not install the Wine runtime {} in {}: {error}",
            runtime.display(),
            target.display()
        );
    }
}

/// Where the helper runs the runtime from, on macOS.
fn target() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }

    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home).join("Library/Application Support/com.daydalek.Maltoria/Libraries")
    })
}

/// Copies `runtime` to `target`, unless `target` is the same version already;
/// whether it did.
///
/// The copy is made beside `target` and swapped in, so that an interrupted
/// copy leaves the installed runtime as it was, and games still running keep
/// the files they opened.
pub fn install(runtime: &Path, target: &Path) -> io::Result<bool> {
    let version = fs::read(runtime.join(VERSION_FILE))?;

    if fs::read(target.join(VERSION_FILE)).is_ok_and(|installed| installed == version) {
        return Ok(false);
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let staging = beside(target, "installing");
    let previous = beside(target, "previous");
    remove(&staging)?;
    remove(&previous)?;

    copy(&fs::canonicalize(runtime)?, &staging)?;

    if fs::symlink_metadata(target).is_ok() {
        fs::rename(target, &previous)?;
    }
    fs::rename(&staging, target)?;
    remove(&previous)?;

    Ok(true)
}

fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

fn remove(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Copies a tree, its links as links, and its files writable, as the runtime
/// a build carries may be read only.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(from)?;
    let kind = metadata.file_type();

    if kind.is_symlink() {
        symlink(fs::read_link(from)?, to)
    } else if kind.is_dir() {
        fs::create_dir(to)?;

        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy(&entry.path(), &to.join(entry.file_name()))?;
        }

        Ok(())
    } else {
        fs::copy(from, to)?;

        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o200);
        fs::set_permissions(to, permissions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("maltoria-wine-{}-{name}", std::process::id()));
            let _ = remove(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = remove(&self.0);
        }
    }

    /// A runtime of `version`, with a read-only executable and a link.
    fn runtime(root: &Path, version: &str) -> PathBuf {
        let runtime = root.join("runtime");
        fs::create_dir_all(runtime.join("Wine/bin")).unwrap();
        fs::write(runtime.join(VERSION_FILE), version).unwrap();
        fs::write(runtime.join("Wine/bin/wine"), "wine").unwrap();
        fs::set_permissions(
            runtime.join("Wine/bin/wine"),
            fs::Permissions::from_mode(0o555),
        )
        .unwrap();
        symlink("bin/wine", runtime.join("Wine/wine")).unwrap();
        runtime
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn installs_where_there_is_none() {
        let scratch = Scratch::new("none");
        let runtime = runtime(&scratch.0, "11.15.11");
        let target = scratch.0.join("Application Support/Libraries");

        assert!(install(&runtime, &target).unwrap());

        assert_eq!(
            fs::read_to_string(target.join(VERSION_FILE)).unwrap(),
            "11.15.11"
        );
        assert_eq!(mode(&target.join("Wine/bin/wine")), 0o755);
        assert_eq!(
            fs::read_link(target.join("Wine/wine")).unwrap(),
            Path::new("bin/wine")
        );
    }

    #[test]
    fn keeps_the_same_version_with_what_was_deployed_into_it() {
        let scratch = Scratch::new("same");
        let runtime = runtime(&scratch.0, "11.15.11");
        let target = scratch.0.join("Libraries");

        install(&runtime, &target).unwrap();
        fs::write(target.join("gptk"), "deployed").unwrap();

        assert!(!install(&runtime, &target).unwrap());
        assert!(target.join("gptk").exists());
    }

    #[test]
    fn replaces_another_version_whole() {
        let scratch = Scratch::new("other");
        let runtime = runtime(&scratch.0, "11.15.11");
        let target = scratch.0.join("Libraries");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join(VERSION_FILE), "11.0.3").unwrap();
        fs::write(target.join("gptk"), "deployed").unwrap();

        assert!(install(&runtime, &target).unwrap());

        assert_eq!(
            fs::read_to_string(target.join(VERSION_FILE)).unwrap(),
            "11.15.11"
        );
        assert!(!target.join("gptk").exists());
        assert!(!scratch.0.join("Libraries.installing").exists());
        assert!(!scratch.0.join("Libraries.previous").exists());
    }

    #[test]
    fn leaves_the_installed_one_for_a_runtime_without_a_version() {
        let scratch = Scratch::new("broken");
        let runtime = scratch.0.join("runtime");
        fs::create_dir_all(&runtime).unwrap();
        let target = scratch.0.join("Libraries");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join(VERSION_FILE), "11.0.3").unwrap();

        assert!(install(&runtime, &target).is_err());
        assert_eq!(
            fs::read_to_string(target.join(VERSION_FILE)).unwrap(),
            "11.0.3"
        );
    }
}
