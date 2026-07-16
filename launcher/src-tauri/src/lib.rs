pub mod dml;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use crate::dml::envelope::Envelope;
use crate::dml::runner::{DmlRunner, RunnerError};

pub struct AppState {
    pub runner: std::sync::Arc<DmlRunner>,
}

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
    pub hint: String,
}

impl From<RunnerError> for CmdError {
    fn from(e: RunnerError) -> Self {
        match e {
            RunnerError::Spawn(m) => CmdError {
                code: "WSL_SPAWN".into(),
                message: m,
                hint: "Is WSL installed and the dml-arch distro present? Try: wsl -d dml-arch".into(),
            },
            RunnerError::BadOutput { raw } => CmdError {
                code: "CLI_BAD_OUTPUT".into(),
                message: raw,
                hint: "Is the dml CLI v3.0.0 installed? Run: powershell -File cli\\dev-install.ps1".into(),
            },
        }
    }
}

fn envelope_to_result(env: Envelope) -> Result<serde_json::Value, CmdError> {
    if env.ok {
        Ok(env.data)
    } else {
        let e = env.error.unwrap_or(crate::dml::envelope::ErrorInfo {
            code: "CLI_BAD_OUTPUT".into(),
            message: "ok=false with no error object".into(),
            hint: String::new(),
        });
        Err(CmdError { code: e.code, message: e.message, hint: e.hint })
    }
}

async fn run_json_cmd(
    state: State<'_, AppState>,
    args: Vec<String>,
) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        runner.run_json(&refs)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
    .and_then(envelope_to_result)
}

pub fn validate_game_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn bad_id(id: &str) -> CmdError {
    CmdError {
        code: "BAD_ID".into(),
        message: format!("invalid game id: {id:?}"),
        hint: "Game ids come from games_list".into(),
    }
}

#[tauri::command]
async fn dml_version(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["version"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["games", "list"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_status(id: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["games", "status", &id]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn wow_accounts(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "accounts".into()]).await
}

#[tauri::command]
async fn wow_server_info(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "server-info".into()]).await
}

#[tauri::command]
async fn wow_items_search(
    name: String,
    quality: Option<u32>,
    min_level: Option<u32>,
    max_level: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "items".into(), "search".into(), "--name".into(), name];
    if let Some(q) = quality {
        args.extend(["--quality".into(), q.to_string()]);
    }
    if let Some(l) = min_level {
        args.extend(["--min-level".into(), l.to_string()]);
    }
    if let Some(l) = max_level {
        args.extend(["--max-level".into(), l.to_string()]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_mail_item(
    to: String,
    items: String,
    subject: Option<String>,
    body: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "mail-item".into(), "--to".into(), to, "--items".into(), items];
    if let Some(s) = subject {
        args.extend(["--subject".into(), s]);
    }
    if let Some(b) = body {
        args.extend(["--body".into(), b]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_teleport_list(
    search: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "teleport-list".into()];
    if let Some(s) = search {
        args.extend(["--search".into(), s]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_teleport(
    char_name: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "teleport".into(), "--char".into(), char_name, "--to".into(), to],
    )
    .await
}

#[tauri::command]
async fn wow_paperdoll(
    char_name: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "paperdoll".into(), "--char".into(), char_name]).await
}

#[tauri::command]
async fn wow_config_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "list".into()]).await
}

#[tauri::command]
async fn wow_config_set(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "config".into(), "set".into(), "--key".into(), key, "--value".into(), value],
    )
    .await
}

#[tauri::command]
async fn wow_config_raw_read(
    file: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "raw-read".into(), "--file".into(), file]).await
}

#[tauri::command]
async fn wow_config_raw_write(
    file: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_json_with_stdin(
            &["wow", "config", "raw-write", "--file", &file],
            &content,
        )
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
    .and_then(envelope_to_result)
}

async fn stream_action(
    action: &'static str,
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_stream(&["games", action, &id], |v| {
            let _ = on_event.send(v);
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map(|_exit| ())
    .map_err(CmdError::from)
}

async fn stream_args(
    args: Vec<String>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        runner.run_stream(&refs, |v| { let _ = on_event.send(v); })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map(|_| ())
    .map_err(CmdError::from)
}

#[tauri::command]
async fn wow_party_setup(on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "party-setup".into()], on_event, state).await
}

#[tauri::command]
async fn wow_party_online(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "online".into()]).await
}

#[tauri::command]
async fn wow_party_add(player: String, class: String, gender: Option<String>, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let mut a: Vec<String> = vec!["wow".into(),"party".into(),"add".into(),"--player".into(),player,"--class".into(),class];
    if let Some(g) = gender { a.extend(["--gender".into(), g]); }
    run_json_cmd(state, a).await
}

#[tauri::command]
async fn wow_party_list(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"list".into(),"--player".into(),player]).await
}

#[tauri::command]
async fn wow_party_kick(bot: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"kick".into(),"--bot".into(),bot]).await
}

#[tauri::command]
async fn wow_party_relogin(player: String, bot: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"relogin".into(),"--player".into(),player,"--bot".into(),bot]).await
}

#[tauri::command]
async fn wow_bridge_setup(on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "bridge-setup".into()], on_event, state).await
}

#[tauri::command]
async fn wow_gm_level(player: String, level: u32, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "level".into(), "--player".into(), player, "--level".into(), level.to_string()],
    )
    .await
}

#[tauri::command]
async fn wow_gm_gold(player: String, gold: u32, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "gold".into(), "--player".into(), player, "--gold".into(), gold.to_string()],
    )
    .await
}

#[tauri::command]
async fn wow_gm_heal(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "gm".into(), "heal".into(), "--player".into(), player]).await
}

#[tauri::command]
async fn wow_gm_revive(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "gm".into(), "revive".into(), "--player".into(), player]).await
}

#[tauri::command]
async fn games_start(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("start", id, on_event, state).await
}

#[tauri::command]
async fn games_stop(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("stop", id, on_event, state).await
}

#[tauri::command]
async fn games_restart(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("restart", id, on_event, state).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { runner: std::sync::Arc::new(DmlRunner::default()) })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            dml_version,
            games_list,
            games_status,
            games_start,
            games_stop,
            games_restart,
            wow_accounts,
            wow_server_info,
            wow_items_search,
            wow_mail_item,
            wow_teleport_list,
            wow_teleport,
            wow_paperdoll,
            wow_config_list,
            wow_config_set,
            wow_config_raw_read,
            wow_config_raw_write,
            wow_party_setup,
            wow_party_online,
            wow_party_add,
            wow_party_list,
            wow_party_kick,
            wow_party_relogin,
            wow_bridge_setup,
            wow_gm_level,
            wow_gm_gold,
            wow_gm_heal,
            wow_gm_revive
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_id_validation() {
        assert!(validate_game_id("wow-server-playerbots"));
        assert!(validate_game_id("Mu_Online.v2"));
        assert!(!validate_game_id(""));
        assert!(!validate_game_id("wow; rm -rf /"));
        assert!(!validate_game_id("wow server"));
        assert!(!validate_game_id("../escape"));
    }
}
