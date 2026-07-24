//! Parity gate for the native-mode **module-list** reader (Task 2, spike:
//! `spike/docker-desktop-native`).
//!
//! Asserts that `dml::modules::ModuleReader`, fed the real
//! `dml wow module catalog --json` `.data` and reading the real on-disk native
//! files (+ local git for installed clones), assembles JSON that DEEP-EQUALS a
//! real `dml wow module list --json` run against those same files. Every field
//! is file/git-derived, so NO divergence is tolerated — an exact deep-equal is
//! required.
//!
//! head/head_date come from the SAME local `git` binary the bash emitter shells
//! (`git -C <dir> log -1 --format=%h%x1f%cs`), so the abbreviation length and
//! date are byte-identical. `git` must therefore be on PATH for this test.
//!
//! FILE/TOOL-GATED like `config_parity.rs`: runs only when the native files +
//! `bash` are present (this dev box has them at `C:/Users/perzi/dml-native`);
//! elsewhere it prints why it skipped and passes, leaving the pure-fn unit tests
//! in `dml::modules` as always-run coverage. `module list`/`catalog` need no yq
//! and no docker (pure file + local git), so — unlike config/tuning — yq is not
//! required here.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use launcher_lib::dml::modules::ModuleReader;

fn games_dir() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\perzi\dml-native"))
}

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
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cli").join("dml");
    p.exists().then_some(p)
}

fn yq_path(games: &Path) -> Option<PathBuf> {
    if let Some(y) = std::env::var_os("DML_YQ_BIN").filter(|s| !s.is_empty()) {
        let p = PathBuf::from(y);
        if p.exists() {
            return Some(p);
        }
    }
    let p = games.join("tools").join("yq.exe");
    p.exists().then_some(p)
}

/// PATH with the bash toolchain (bin + usr/bin), the git dir, and Docker
/// Desktop's bin prepended — so both the spawned `dml` AND this test's own
/// direct `git` reads resolve the same `git.exe`.
fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    let docker =
        PathBuf::from(r"C:\Users\perzi\AppData\Local\Programs\DockerDesktop\resources\bin");
    if docker.exists() {
        dirs.push(docker);
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

fn run_dml(
    bash: &Path,
    script: &Path,
    games: &Path,
    yq: Option<&Path>,
    path: &OsString,
    args: &[&str],
) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script).args(args).arg("--json");
    cmd.env("DML_GAMES_DIR", games);
    if let Some(y) = yq {
        cmd.env("DML_YQ_BIN", y);
    }
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("dml {args:?} output not JSON ({e}): {stdout}"))
}

#[test]
fn native_reader_deep_equals_module_list() {
    let games = games_dir();
    let title_dir = games.join("wow-server-playerbots");
    // `module list` gates on the compose dir; the override file is a reliable
    // presence marker for the native title dir (same gate as config/tuning).
    let override_file = title_dir.join("docker-compose.override.yml");

    if !override_file.is_file() {
        eprintln!("SKIP module parity: no {}", override_file.display());
        return;
    }
    let Some(bash) = find_bash() else {
        eprintln!("SKIP module parity: no bash (set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP module parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let yq = yq_path(&games);
    let path = augmented_path(&bash);

    // Ground truth: the real `module list`.
    let list = run_dml(&bash, &script, &games, yq.as_deref(), &path, &["wow", "module", "list"]);
    assert_eq!(list["ok"], true, "module list not ok: {list}");
    let want = &list["data"];

    // The catalog the Rust command would cache.
    let catalog =
        run_dml(&bash, &script, &games, yq.as_deref(), &path, &["wow", "module", "catalog"]);
    assert_eq!(catalog["ok"], true, "module catalog not ok: {catalog}");
    let cat_data = &catalog["data"];

    // The reader under test — must deep-equal the whole `.data` exactly.
    let reader = ModuleReader::for_title(&title_dir);
    let got = reader.assemble(cat_data);

    // Compare family-by-family for a legible failure, then the top-level fields.
    for fam in ["cpp", "lua", "sql"] {
        assert_eq!(
            got["families"][fam], want["families"][fam],
            "module reader diverged from `module list` in families.{fam}"
        );
    }
    assert_eq!(got["rebuild_pending"], want["rebuild_pending"], "rebuild_pending diverged");
    assert_eq!(got["ale_ready"], want["ale_ready"], "ale_ready diverged");
    assert_eq!(got, *want, "module reader diverged from `module list`");
}
