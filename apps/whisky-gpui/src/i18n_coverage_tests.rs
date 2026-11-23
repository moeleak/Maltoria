//! Every runtime ID the GPUI app defines must have a map entry in every locale.

use crate::i18n::{Locale, LocaleGuard, lookup, s};

#[test]
fn provider_spec_kinds_have_labels_and_tips_in_every_locale() {
    for loc in Locale::ALL {
        let _guard = LocaleGuard::new(loc);
        let maps = &s().maps;
        for spec in crate::translation::PROVIDER_SPECS {
            assert!(
                lookup(maps.provider_label, spec.kind).is_some(),
                "{loc:?}: provider_label missing {}",
                spec.kind
            );
            assert!(
                lookup(maps.provider_short_label, spec.kind).is_some(),
                "{loc:?}: provider_short_label missing {}",
                spec.kind
            );
            assert!(
                !spec.display_tip().is_empty(),
                "{loc:?}: display_tip empty for {}",
                spec.kind
            );
        }
    }
}

#[test]
fn provider_presets_have_names_and_tags_in_every_locale() {
    for loc in Locale::ALL {
        let _guard = LocaleGuard::new(loc);
        let maps = &s().maps;
        for preset in crate::translation::PROVIDER_PRESETS {
            assert!(
                lookup(maps.provider_preset_name, preset.id).is_some(),
                "{loc:?}: provider_preset_name missing {}",
                preset.id
            );
            assert!(
                lookup(maps.provider_preset_tag, preset.id).is_some(),
                "{loc:?}: provider_preset_tag missing {}",
                preset.id
            );
        }
    }
}

#[test]
fn metadata_source_ids_have_labels_in_every_locale() {
    for loc in Locale::ALL {
        let _guard = LocaleGuard::new(loc);
        let map = s().maps.source_label;
        let mut ids: std::collections::BTreeSet<&str> =
            crate::onboarding::SOURCES.iter().copied().collect();
        ids.extend(crate::metadata_editor::PROVIDERS.iter().copied());
        ids.extend(
            crate::state::METADATA_SOURCES
                .iter()
                .map(|(_, id)| *id),
        );
        for id in ids {
            assert!(
                lookup(map, id).is_some(),
                "{loc:?}: source_label missing {id}"
            );
        }
    }
}
