/// `~/` resolved the same way `soap::soap_env_path` / `lib.rs::
/// windows_soap_env_path` resolve it: `USERPROFILE` first (Windows), then
/// `HOME` (non-Windows / dev). `None` when neither is set. Shared home-dir
/// resolution for every native reader under `~/.dml/...` (client-path,
/// wowhead-cache, …) — task D1a's brief: "Home dir: ~/.dml resolves via
/// USERPROFILE on Windows … reuse an existing home-resolution helper."
pub fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
}

/// `~/.dml` — the DML state dir root (`client-path`, `wowhead-cache/`, …).
pub fn dml_home_dir() -> Option<std::path::PathBuf> {
    home_dir().map(|h| h.join(".dml"))
}
