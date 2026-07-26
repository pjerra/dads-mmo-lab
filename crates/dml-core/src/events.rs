//! Game-agnostic NDJSON stream event constructors — shared shapes emitted by
//! every streamed Tauri command regardless of which section (module-install/
//! -update/-remove, backup-restore, …) is producing them. `section_start`/
//! `section_end`/`done_event`/`error_event` DO carry caller-supplied section
//! names/data — the section name CONSTANTS themselves stay with their
//! game-specific owning module (e.g. `dml::modmgr::SECTION_INSTALL`).

use serde_json::Value;

pub fn line_event(level: &str, text: impl Into<String>) -> Value {
    serde_json::json!({"event": "line", "level": level, "text": text.into()})
}

pub fn section_start(name: &str) -> Value {
    serde_json::json!({"event": "section_start", "name": name})
}
pub fn section_end(name: &str, status: &str) -> Value {
    serde_json::json!({"event": "section_end", "name": name, "status": status})
}
pub fn done_event(data: Value) -> Value {
    serde_json::json!({"event": "done", "data": data})
}
pub fn error_event(code: &str, message: impl Into<String>, hint: &str) -> Value {
    serde_json::json!({"event": "error", "error": {"code": code, "message": message.into(), "hint": hint}})
}
