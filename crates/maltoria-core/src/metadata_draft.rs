//! A game's metadata draft: which work on each platform it is linked to, and
//! which platform supplies each kind of metadata.
//!
//! Search results never write storage. Linking a result stages its platform
//! ID in the draft; only saving it (`metadata.configuration.save`) commits the
//! links, and the helper then fetches the linked works' details.

use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use crate::i18n::s;

/// The platforms a game can be linked to, in the editor's order.
pub const PROVIDERS: [&str; 5] = ["steam", "bangumi", "vndb", "ymgal", "nextmoe"];

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// A fresh generation, so that replies for an editor closed since are dropped.
pub fn next_generation() -> u64 {
    NEXT_GENERATION.fetch_add(1, Ordering::Relaxed)
}

/// The platform ID in `input`, a work's page URL on `source` or its ID; never
/// search text, nor a URL whose host merely contains the platform's name.
pub fn platform_id(source: &str, input: &str) -> Option<String> {
    let input = input.trim();
    let id = if let Some(url) = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
    {
        let (host, path) = url.split_once('/')?;
        let path = path.split(['?', '#']).next()?.trim_end_matches('/');
        match source {
            "steam" if matches!(host, "store.steampowered.com" | "steamcommunity.com") => {
                path.strip_prefix("app/")?.split('/').next()?
            }
            "bangumi" if matches!(host, "bgm.tv" | "bangumi.tv" | "chii.in") => {
                path.strip_prefix("subject/")?
            }
            "vndb" if matches!(host, "vndb.org" | "www.vndb.org") => path,
            "ymgal" if matches!(host, "ymgal.games" | "www.ymgal.games") => {
                path.strip_prefix("ga")?
            }
            _ => return None,
        }
    } else {
        input
    };
    let digits = match source {
        "vndb" => id.strip_prefix('v').unwrap_or(id),
        "ymgal" => id.strip_prefix("ga").unwrap_or(id),
        "steam" | "bangumi" | "nextmoe" => id,
        _ => return None,
    };

    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || digits.bytes().all(|byte| byte == b'0')
    {
        return None;
    }

    let digits = digits.trim_start_matches('0');
    Some(if source == "vndb" {
        format!("v{digits}")
    } else {
        digits.into()
    })
}

/// One game's draft, as the helper's `metadata.configuration.*` record.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataDraft {
    /// Replies carry it back; those of another generation are stale.
    pub generation: u64,
    /// Monotonic within the generation, allocated before each search.
    pub search_request: u64,
    pub record: Value,
    /// The record as last loaded or saved, to tell unsaved changes.
    pub saved: Value,
    pub capabilities: Value,
    pub loaded: bool,
    pub search_source: String,
}

impl Default for MetadataDraft {
    fn default() -> Self {
        Self {
            generation: next_generation(),
            search_request: 0,
            record: json!({}),
            saved: json!({}),
            capabilities: json!({}),
            loaded: false,
            search_source: "bangumi".into(),
        }
    }
}

impl MetadataDraft {
    /// A draft that searches `source` first.
    pub fn searching(source: &str) -> Self {
        Self {
            search_source: source.into(),
            ..Self::default()
        }
    }

    /// Takes a `get` or `save` reply's record and capabilities.
    pub fn accept_reply(&mut self, reply: &Value) -> Result<(), String> {
        let record = &reply["record"];

        if !record.is_object() || !record["defaultSource"].is_string() {
            return Err(s().metadata.err_invalid_reply.into());
        }

        self.capabilities = reply["capabilities"].clone();
        self.record = record.clone();
        self.saved = self.record.clone();
        self.loaded = true;
        Ok(())
    }

    /// Whether the draft has changes that were not saved.
    pub fn is_dirty(&self) -> bool {
        self.loaded && self.record != self.saved
    }

    /// The sources a save may fetch from: the `allowed` ones, and those the
    /// game is already linked to, whose data is local.
    pub fn policy_sources(&self, mut allowed: Vec<String>) -> Vec<String> {
        for &source in &PROVIDERS {
            if (self.record["bindings"][source].is_string()
                || self.record["snapshots"][source].is_object())
                && !allowed.iter().any(|enabled| enabled == source)
            {
                allowed.push(source.into());
            }
        }

        allowed
    }

    /// The platform supplying `category`, or `"default"`'s for the default.
    pub fn source(&self, category: &str) -> &str {
        if category == "default" {
            self.record["defaultSource"].as_str().unwrap_or("steam")
        } else {
            self.record["categorySources"][category]
                .as_str()
                .unwrap_or("")
        }
    }

    /// The platform that actually supplies `category`.
    pub fn effective_source(&self, category: &str) -> &str {
        let source = self.source(category);

        if source.is_empty() {
            self.source("default")
        } else {
            source
        }
    }

    pub fn set_source(&mut self, category: &str, source: &str) {
        if category == "default" {
            self.record["defaultSource"] = json!(source);
        } else if source.is_empty() {
            if let Some(overrides) = self.record["categorySources"].as_object_mut() {
                overrides.remove(category);
            }
        } else {
            self.record["categorySources"][category] = json!(source);
        }
    }

    /// The ID of the work `source` is linked to, if any.
    pub fn binding(&self, source: &str) -> Option<&str> {
        self.record["bindings"][source].as_str()
    }

    /// Links `source` to the work of a search result; the platform ID, not
    /// its translated title, identifies the work.
    pub fn bind(&mut self, source: &str, snapshot: Value) -> Result<(), String> {
        let m = &s().metadata;
        let id = snapshot["sourceID"]
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or(m.err_missing_platform_id)?
            .to_owned();

        if snapshot["source"].as_str() != Some(source) {
            return Err(m.err_platform_mismatch.into());
        }

        // Search hits are summaries: linking the same work again must not
        // replace its fetched details, characters especially.
        if self.binding(source) == Some(id.as_str())
            && self.record["snapshots"][source]["sourceID"].as_str() == Some(id.as_str())
        {
            return Ok(());
        }

        self.record["bindings"][source] = json!(id);
        self.record["snapshots"][source] = snapshot;
        Ok(())
    }

    /// Links `source` to the work with `id`, fetched on save.
    pub fn bind_id(&mut self, source: &str, id: &str) {
        self.record["bindings"][source] = json!(id);

        if let Some(snapshots) = self.record["snapshots"].as_object_mut() {
            snapshots.remove(source);
        }
    }

    /// Unlinks `source`.
    pub fn unbind(&mut self, source: &str) {
        for field in ["bindings", "snapshots"] {
            if let Some(map) = self.record[field].as_object_mut() {
                map.remove(source);
            }
        }
    }

    /// Whether the search results of `request` for this draft are current.
    pub fn accepts_search(&self, generation: u64, request: u64) -> bool {
        generation == self.generation && request == self.search_request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> MetadataDraft {
        let mut draft = MetadataDraft::searching("vndb");
        draft
            .accept_reply(&json!({
                "record": {"defaultSource": "bangumi", "bindings": {"bangumi": "1234"}},
                "capabilities": {"sources": ["bangumi", "vndb"]}
            }))
            .unwrap();
        draft
    }

    #[test]
    fn platform_ids_come_from_work_urls_or_ids_only() {
        assert_eq!(platform_id("vndb", "https://vndb.org/v211"), Some("v211".into()));
        assert_eq!(platform_id("vndb", "https://vndb.org/v211/chars"), None);
        assert_eq!(platform_id("vndb", "v0211"), Some("v211".into()));
        assert_eq!(platform_id("bangumi", "https://bgm.tv/subject/51/"), Some("51".into()));
        assert_eq!(platform_id("steam", "https://store.steampowered.com/app/1407200/x"), Some("1407200".into()));
        assert_eq!(platform_id("ymgal", "https://www.ymgal.games/ga30044"), Some("30044".into()));
        assert_eq!(platform_id("bangumi", "WHITE ALBUM2"), None);
        assert_eq!(platform_id("bangumi", "https://bgm.tv.example.com/subject/51"), None);
        assert_eq!(platform_id("vndb", "v000"), None);
        assert_eq!(platform_id("elsewhere", "12"), None);
    }

    #[test]
    fn replies_load_the_record_and_reset_the_changes() {
        let mut draft = loaded();

        assert!(draft.loaded);
        assert!(!draft.is_dirty());
        assert_eq!(draft.source("default"), "bangumi");
        assert_eq!(draft.binding("bangumi"), Some("1234"));

        draft.set_source("cover", "vndb");
        assert!(draft.is_dirty());
        assert_eq!(draft.effective_source("cover"), "vndb");
        assert_eq!(draft.effective_source("summary"), "bangumi");

        draft.set_source("cover", "");
        assert_eq!(draft.effective_source("cover"), "bangumi");

        assert!(draft.accept_reply(&json!({"record": {}})).is_err());
        assert!(draft.loaded);
    }

    #[test]
    fn binding_stages_the_platform_id_of_matching_results() {
        let mut draft = loaded();
        let hit = json!({"source": "vndb", "sourceID": "v211", "title": "WHITE ALBUM2"});

        draft.bind("vndb", hit.clone()).unwrap();
        assert_eq!(draft.binding("vndb"), Some("v211"));
        assert!(draft.is_dirty());

        // Linking it again keeps the details fetched since.
        draft.record["snapshots"]["vndb"]["characters"] = json!([{"name": "Setsuna"}]);
        draft.bind("vndb", hit).unwrap();
        assert!(draft.record["snapshots"]["vndb"]["characters"].is_array());

        assert!(draft.bind("vndb", json!({"source": "bangumi", "sourceID": "1"})).is_err());
        assert!(draft.bind("vndb", json!({"source": "vndb"})).is_err());

        draft.unbind("vndb");
        assert_eq!(draft.binding("vndb"), None);
        assert!(draft.record["snapshots"]["vndb"].is_null());
    }

    #[test]
    fn linked_sources_may_always_be_saved_from() {
        let draft = loaded();

        assert_eq!(draft.policy_sources(vec!["vndb".into()]), ["vndb", "bangumi"]);
        assert_eq!(draft.policy_sources(vec!["bangumi".into()]), ["bangumi"]);
    }

    #[test]
    fn only_the_latest_search_is_current() {
        let mut draft = loaded();
        draft.search_request = 3;

        assert!(draft.accepts_search(draft.generation, 3));
        assert!(!draft.accepts_search(draft.generation, 2));
        assert!(!draft.accepts_search(draft.generation + 1, 3));
        assert_ne!(MetadataDraft::default().generation, draft.generation);
    }
}
