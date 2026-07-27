//! Embedded static registries (cargo-workspace refactor, Task 8).
//!
//! Native mode used to shell the bash `dml` CLI once per session to fetch
//! three STATIC registries — the config registry, the module-tuning
//! registry, and the module catalog — cache them in Tauri `AppState`, and
//! then read every LIVE value itself in Rust (`config`, `tuning`, `modules`).
//! None of these three ever changes at runtime: they are hand-authored
//! catalogs of what settings/modules exist, not values. Baking them into the
//! binary removes the last bash dependency from the native read path.
//!
//! The three JSON files under `data/` are SNAPSHOTS taken from the bash
//! oracle (the ground truth — see `cli/src/40-config.sh` and
//! `cli/src/90-main.sh` for the arms that build them):
//!
//! - `data/config-registry.json` — `dml wow config registry --json` →
//!   `.data.settings`
//! - `data/tuning-registry.json` — `dml wow config tuning-registry --json` →
//!   `.data.settings`
//! - `data/module-catalog.json` — `dml wow module catalog --json` → `.data`
//!
//! They are committed source, not build output — regenerate them from the
//! bash oracle (never hand-edit) if the CLI's registries ever change. The
//! `crates/dml-wow/tests/{config,tuning,module}_parity.rs` suites deep-equal
//! the embedded-registry-fed readers against live bash output and will fail
//! loudly if a snapshot goes stale.

use std::sync::LazyLock;

static CONFIG_REGISTRY_JSON: &str = include_str!("../data/config-registry.json");
static TUNING_REGISTRY_JSON: &str = include_str!("../data/tuning-registry.json");
static MODULE_CATALOG_JSON: &str = include_str!("../data/module-catalog.json");

static CONFIG_REGISTRY: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| {
    serde_json::from_str(CONFIG_REGISTRY_JSON)
        .expect("embedded config-registry.json must parse as a JSON array")
});

static TUNING_REGISTRY: LazyLock<Vec<serde_json::Value>> = LazyLock::new(|| {
    serde_json::from_str(TUNING_REGISTRY_JSON)
        .expect("embedded tuning-registry.json must parse as a JSON array")
});

static MODULE_CATALOG: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_str(MODULE_CATALOG_JSON)
        .expect("embedded module-catalog.json must parse as JSON")
});

/// The static config registry rows (`dml wow config registry --json` →
/// `.data.settings`). Never changes at runtime.
pub fn config_registry_rows() -> &'static [serde_json::Value] {
    &CONFIG_REGISTRY
}

/// The static module-tuning registry rows (`dml wow config tuning-registry
/// --json` → `.data.settings`, 13 rows). Never changes at runtime.
pub fn tuning_registry_rows() -> &'static [serde_json::Value] {
    &TUNING_REGISTRY
}

/// The static module catalog (`dml wow module catalog --json` → `.data`).
/// Never changes at runtime.
pub fn module_catalog() -> &'static serde_json::Value {
    &MODULE_CATALOG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_registry_parses_and_has_known_key() {
        let rows = config_registry_rows();
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r.get("key").is_some()));
        assert!(rows.iter().any(|r| r["key"] == "server.motd"));
    }

    // Pins the exact row count so an unnoticed add/remove in the bash
    // oracle's `_cfg_rows` heredoc (`cli/src/40-config.sh`) fails HERE, on
    // every machine, rather than only in `config_parity.rs`, which SKIPS on
    // any box without the native runtime at C:/Users/perzi/dml-native.
    #[test]
    fn config_registry_is_66_rows() {
        assert_eq!(config_registry_rows().len(), 66);
    }

    #[test]
    fn tuning_registry_is_13_rows() {
        assert_eq!(tuning_registry_rows().len(), 13);
    }

    #[test]
    fn module_catalog_parses() {
        assert!(module_catalog().is_object() || module_catalog().is_array());
    }

    // Same COUNT-drift guard as `config_registry_is_66_rows`, for the three
    // family arrays sourced from `cli/src/70-modules.sh`'s
    // `_module_registry_{cpp,lua,sql}` heredocs -- catches an unnoticed
    // add/remove on every machine, not just where `module_parity.rs` runs.
    #[test]
    fn module_catalog_family_counts() {
        let cat = module_catalog();
        let families = &cat["families"];
        assert_eq!(families["cpp"].as_array().expect("families.cpp[]").len(), 19);
        assert_eq!(families["lua"].as_array().expect("families.lua[]").len(), 9);
        assert_eq!(families["sql"].as_array().expect("families.sql[]").len(), 10);
    }
}
