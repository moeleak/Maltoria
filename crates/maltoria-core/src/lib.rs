//! Maltoria's GUI-independent core: the supervisor for the Swift helper
//! (`whisky-bridge`), the library model built from its catalogue, runtime,
//! configuration and program settings state, and the localized strings
//! generated from `locales/*.toml`.
//!
//! The Material app (`apps/maltoria`) builds on it.

pub mod bridge;
pub mod config;
pub mod container_import;
pub mod create_bottle;
pub mod desktop_open;
pub mod host;
pub mod i18n;
pub mod library_layout;
pub mod library_view;
pub mod metadata_draft;
pub mod model;
pub mod onboarding;
pub mod platform;
pub mod preferences;
pub mod processes;
pub mod program_settings;
pub mod programs;
pub mod runtime;
pub mod scan;
pub mod winetricks;
