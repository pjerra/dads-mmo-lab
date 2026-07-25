//! Config-WRITE parity gate for Task B3 (spike: `spike/docker-desktop-native`).
//!
//! Asserts that `dml::config::conf_write` (Rust, used by
//! `wow_config_set_native`'s direct-conf route, Task B2a) and the bash
//! oracle's `wow config set --key conf:<file>:<Key> --value <v>` direct-conf
//! route (`90-main.sh:2344`, the `_cfg_conf_write` awk algorithm) produce
//! BYTE-IDENTICAL conf files for the same edit.
//!
//! HARD RULE: this test never touches the real title dir
//! (`C:/Users/perzi/dml-native/wow-server-playerbots`). Every file it reads
//! or writes lives under a fresh `std::env::temp_dir()` subtree, built fresh
//! per run and removed at the end. Two independent temp "title dirs" are
//! seeded with the identical starting `mod_test.conf`; the SAME edit is then
//! applied to one via the Rust fn directly and to the other via the real
//! `dml` CLI (bash-spawned, `DML_GAMES_DIR` pointed at that copy) — the two
//! resulting files are byte-compared.
//!
//! The picked conf name (`mod_test.conf`) deliberately has NO live-reload
//! console command (`_conf_reload_cmd` only knows `transmog.conf`), so the
//! CLI's direct-conf-set path never attempts a SOAP call or a `docker
//! inspect` — the whole comparison is a pure offline file edit, safe to run
//! with the server down.
//!
//! FILE/TOOL-GATED, same skip convention as `db_pages_parity.rs`/
//! `config_parity.rs`: skips (and passes) when `bash`, the `dml` script, or
//! `yq` can't be found, or when the temp fixtures can't be prepared.
//! Overridable via `DML_BASH` / `DML_SCRIPT` / `DML_YQ_BIN`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use launcher_lib::dml::config::{conf_path_in, conf_write};

fn find_bash() -> Option<OsString> {
    if let Some(b) = std::env::var_os("DML_BASH").filter(|s| !s.is_empty()) {
        return Some(b);
    }
    for c in [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
    ] {
        if Path::new(c).exists() {
            return Some(OsString::from(c));
        }
    }
    None
}

fn find_script() -> Option<PathBuf> {
    if let Some(s) = std::env::var_os("DML_SCRIPT").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(s));
    }
    // <repo>/cli/dml, relative to launcher/src-tauri.
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("cli")
        .join("dml");
    p.exists().then_some(p)
}

/// Only ever READS this binary's existence — never writes anywhere under the
/// real games dir. `DML_YQ_BIN` is honored first; otherwise probes the same
/// fixed dev-box location the sibling parity tests use.
fn find_yq() -> Option<PathBuf> {
    if let Some(y) = std::env::var_os("DML_YQ_BIN").filter(|s| !s.is_empty()) {
        let p = PathBuf::from(y);
        if p.exists() {
            return Some(p);
        }
    }
    let p = PathBuf::from(r"C:\Users\perzi\dml-native").join("tools").join("yq.exe");
    p.exists().then_some(p)
}

/// PATH with the bash toolchain dirs (bin + usr/bin) prepended, so the
/// spawned `dml` finds awk/grep/sed for `_cfg_conf_write`'s awk algorithm.
fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

/// Seeds a fresh temp "games dir" containing exactly one title,
/// `wow-server-playerbots`, with a minimal `docker-compose.yml` (so
/// `_wow_server_dir`/`_resolve_compose_dir` resolve it) and a
/// `mod_test.conf` under `env/dist/etc/modules/` with a few `Key = value`
/// lines plus a comment line. Returns the title dir root and the conf path.
fn seed_title_dir(games_dir: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    let title_dir = games_dir.join("wow-server-playerbots");
    let modules_dir = title_dir.join("env").join("dist").join("etc").join("modules");
    fs::create_dir_all(&modules_dir)?;
    fs::write(
        title_dir.join("docker-compose.yml"),
        "services:\n  ac-worldserver:\n    image: dummy\n",
    )?;
    let conf_path = conf_path_in(&title_dir, "mod_test.conf");
    fs::write(
        &conf_path,
        "# mod_test.conf fixture (Task B3)\n\
         FirstKey = 1\n\
         SomeKey = old\n\
         OtherKey = \"quoted value\"\n",
    )?;
    Ok((title_dir, conf_path))
}

fn run_cli_set(
    bash: &Path,
    script: &Path,
    games_dir: &Path,
    yq: &Path,
    path: &OsString,
    key_spec: &str,
    value: &str,
) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script)
        .args(["wow", "config", "set", "--key", key_spec, "--value", value, "--json"]);
    cmd.env("DML_GAMES_DIR", games_dir);
    cmd.env("DML_YQ_BIN", yq);
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("dml config set output not JSON ({e}): {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr)))
}

/// Real title dir the launcher/CLI use in day-to-day operation — this test
/// must never write here. Asserted against both temp copies as a hard guard.
fn real_title_dir() -> PathBuf {
    PathBuf::from(r"C:\Users\perzi\dml-native\wow-server-playerbots")
}

#[test]
fn native_conf_write_matches_cli_direct_route() {
    // --- gate: everything must be present, else skip (and pass) ---
    let Some(bash) = find_bash() else {
        eprintln!("SKIP config_write_parity: prereqs absent (no bash, set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP config_write_parity: prereqs absent (no dml script, set DML_SCRIPT)");
        return;
    };
    let Some(yq) = find_yq() else {
        eprintln!("SKIP config_write_parity: prereqs absent (no yq, set DML_YQ_BIN)");
        return;
    };
    let path = augmented_path(&bash);

    // --- fixtures: two independent temp "games dirs", never the real one ---
    let tmp_base = std::env::temp_dir().join(format!("dml_cfgwrite_parity_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp_base); // best-effort cleanup of a stale prior run
    let rust_games_dir = tmp_base.join("rust-side");
    let cli_games_dir = tmp_base.join("cli-side");

    let (rust_title_dir, rust_conf) = match seed_title_dir(&rust_games_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP config_write_parity: prereqs absent (seed rust-side fixture: {e})");
            return;
        }
    };
    let (cli_title_dir, cli_conf) = match seed_title_dir(&cli_games_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP config_write_parity: prereqs absent (seed cli-side fixture: {e})");
            let _ = fs::remove_dir_all(&tmp_base);
            return;
        }
    };

    // Hard guard: neither temp title dir is ever the real one.
    assert_ne!(rust_title_dir, real_title_dir(), "rust-side fixture must not be the real title dir");
    assert_ne!(cli_title_dir, real_title_dir(), "cli-side fixture must not be the real title dir");

    let starting_bytes = fs::read(&rust_conf).expect("read seeded rust conf");
    assert_eq!(starting_bytes, fs::read(&cli_conf).expect("read seeded cli conf"), "fixtures must start identical");

    // --- edit 1: SomeKey old -> newval, applied via BOTH backends ---
    let rust_changed = conf_write(&rust_conf, "SomeKey", "newval").expect("rust conf_write");
    assert!(rust_changed, "rust conf_write should report a change on the first edit");

    let cli_out = run_cli_set(&bash, &script, &cli_games_dir, &yq, &path, "conf:mod_test.conf:SomeKey", "newval");
    assert_eq!(cli_out["ok"], true, "cli config set not ok: {cli_out}");
    assert_eq!(cli_out["data"]["changed"], true, "cli should report changed=true: {cli_out}");
    // No reload command is known for mod_test.conf, so the server-down SOAP
    // attempt is never made — this stays a pure offline file edit.
    assert_eq!(cli_out["data"]["applied"], "restart", "cli should report applied=\"restart\": {cli_out}");

    let rust_bytes = fs::read(&rust_conf).expect("read rust conf after edit");
    let cli_bytes = fs::read(&cli_conf).expect("read cli conf after edit");
    assert_eq!(
        rust_bytes, cli_bytes,
        "rust conf_write and the CLI's direct-conf-set route must produce byte-identical files\nrust:\n{}\ncli:\n{}",
        String::from_utf8_lossy(&rust_bytes),
        String::from_utf8_lossy(&cli_bytes)
    );
    assert_ne!(rust_bytes, starting_bytes, "the file must actually have changed from the seed");

    // --- edit 2: no-op case, writing the SAME value again on both sides ---
    let rust_changed2 = conf_write(&rust_conf, "SomeKey", "newval").expect("rust conf_write (no-op)");
    assert!(!rust_changed2, "rust conf_write should report no change on a same-value no-op");

    let cli_out2 = run_cli_set(&bash, &script, &cli_games_dir, &yq, &path, "conf:mod_test.conf:SomeKey", "newval");
    assert_eq!(cli_out2["ok"], true, "cli config set (no-op) not ok: {cli_out2}");
    assert_eq!(cli_out2["data"]["changed"], false, "cli should report changed=false on the no-op: {cli_out2}");
    assert_eq!(cli_out2["data"]["applied"], "none", "cli should report applied=\"none\" on the no-op: {cli_out2}");

    let rust_bytes2 = fs::read(&rust_conf).expect("read rust conf after no-op");
    let cli_bytes2 = fs::read(&cli_conf).expect("read cli conf after no-op");
    assert_eq!(rust_bytes2, rust_bytes, "rust file must be byte-unchanged by the no-op write");
    assert_eq!(cli_bytes2, cli_bytes, "cli file must be byte-unchanged by the no-op write");
    assert_eq!(rust_bytes2, cli_bytes2, "both sides must still agree after the no-op");

    // --- cleanup: never leave temp fixtures behind ---
    let _ = fs::remove_dir_all(&tmp_base);
}
