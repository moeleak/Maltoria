//! The one container every game lives in.
//!
//! The helper keeps any number of bottles, but the interface shows one: the
//! one remembered from last time, or else the one with the most games, so an
//! existing setup opens on its library. Other containers are left as they are.

use std::collections::HashMap;

use maltoria_core::bridge::dto::Container;

/// The name of the container created on first launch.
pub const NAME: &str = "Maltoria";

/// The Windows version the container reports: most visual novels expect 10.
pub const WINDOWS_VERSION: &str = "win10";

/// The winetricks verb that installs the CJK fonts the games' menus need.
pub const FONTS_VERB: &str = "cjkfonts";

/// Which container the library shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// This one.
    Use(String),
    /// Wait for the entries of every available container before choosing.
    Waiting,
    /// There is no container to use: create one.
    Create,
}

/// The container to show among `containers`: the `remembered` one if it is
/// still available, or else the available one with the most `games`, once
/// every available container has reported its games.
pub fn choose(
    containers: &[Container],
    remembered: Option<&str>,
    games: &HashMap<String, usize>,
) -> Choice {
    let mut available = containers.iter().filter(|container| container.available);

    if let Some(path) = remembered
        && containers
            .iter()
            .any(|container| container.available && container.path == path)
    {
        return Choice::Use(path.to_owned());
    }

    let Some(first) = available.next() else {
        return Choice::Create;
    };

    let candidates = std::iter::once(first).chain(available);
    let mut best: Option<(&Container, usize)> = None;

    for container in candidates {
        let Some(&count) = games.get(&container.path) else {
            return Choice::Waiting;
        };

        // The first of equal counts wins, as the helper orders containers.
        if best.is_none_or(|(_, most)| count > most) {
            best = Some((container, count));
        }
    }

    best.map_or(Choice::Create, |(container, _)| {
        Choice::Use(container.path.clone())
    })
}

/// Where a new container goes: under the helper's default bottle location if
/// it reported one, or the helper's own default.
pub fn parent_directory(default_location: Option<&str>) -> String {
    if let Some(location) = default_location.filter(|location| location.starts_with('/')) {
        return location.to_owned();
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());

    if cfg!(target_os = "macos") {
        format!("{home}/Library/Containers/com.daydalek.Maltoria/Bottles")
    } else {
        format!("{home}/.local/share/whisky/Bottles")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(path: &str, available: bool) -> Container {
        Container {
            path: path.into(),
            name: path.trim_start_matches('/').into(),
            available,
        }
    }

    fn counts(pairs: &[(&str, usize)]) -> HashMap<String, usize> {
        pairs
            .iter()
            .map(|(path, count)| ((*path).into(), *count))
            .collect()
    }

    #[test]
    fn the_remembered_container_wins_while_it_is_available() {
        let containers = [container("/a", true), container("/b", true)];

        assert_eq!(
            choose(&containers, Some("/b"), &HashMap::new()),
            Choice::Use("/b".into())
        );
    }

    #[test]
    fn otherwise_the_container_with_the_most_games_wins() {
        let containers = [
            container("/a", true),
            container("/b", true),
            container("/c", false),
        ];

        assert_eq!(
            choose(&containers, Some("/c"), &counts(&[("/a", 2), ("/b", 7)])),
            Choice::Use("/b".into())
        );
        // Ties go to the first, in the helper's order.
        assert_eq!(
            choose(&containers, None, &counts(&[("/a", 3), ("/b", 3)])),
            Choice::Use("/a".into())
        );
    }

    #[test]
    fn the_choice_waits_for_every_available_container() {
        let containers = [container("/a", true), container("/b", true)];

        assert_eq!(
            choose(&containers, None, &counts(&[("/a", 9)])),
            Choice::Waiting
        );
    }

    #[test]
    fn without_an_available_container_one_is_created() {
        assert_eq!(choose(&[], None, &HashMap::new()), Choice::Create);
        assert_eq!(
            choose(&[container("/gone", false)], Some("/gone"), &HashMap::new()),
            Choice::Create
        );
    }

    #[test]
    fn new_containers_go_under_the_reported_location() {
        assert_eq!(
            parent_directory(Some("/Volumes/Games/Bottles")),
            "/Volumes/Games/Bottles"
        );
        // A relative or empty location is not one.
        assert!(parent_directory(Some("Bottles")).ends_with("Bottles"));
        assert!(
            parent_directory(Some("Bottles")).starts_with('/') || std::env::var("HOME").is_err()
        );
    }
}
