//! LIVE, opt-in smoke for `Backend::Arch` — the real binary, the real distro.
//!
//! # Why this file exists
//!
//! [`dml_core::vocab`] maps the launcher's bash-CLI vocabulary onto `dml-wow`'s
//! flat subcommands, and every bit of that is proven by UNIT tests against
//! clap's derive. What no unit test can reach is the boundary itself: whether
//! `wsl.exe -d dml-arch -u dml --exec dml-wow <translated argv>` is a command
//! the distro accepts, and whether the answer it gives is the SAME ANSWER the
//! bash `dml` in that same distro gives for the same question.
//!
//! So the assertion here is DIFFERENTIAL, not "exit 0". One logical launcher
//! command, two vocabularies:
//!
//!   * [`DmlRunner::default`] — `Vocabulary::Bash`, spawning
//!     `wsl.exe -d dml-arch -u dml -- dml <argv> --json`
//!   * [`DmlRunner::arch`] — `Vocabulary::Arch`, spawning
//!     `wsl.exe -d dml-arch -u dml --exec dml-wow <translated argv>`
//!
//! Same question, two backends, same answer. A smoke that only checked the Arch
//! side would pass just as happily against a binary answering confidently and
//! wrongly — which, as `live_config_list_agrees_across_backends` records, is
//! exactly what happens today.
//!
//! # Running it
//!
//!   cargo test -p dml-core --test live_arch_smoke -- --ignored --nocapture
//!
//! `#[ignore]` rather than a runtime skip, deliberately: a self-skipping test
//! that quietly passes on a machine with no distro is the vacuous-pass trap
//! (CLAUDE.md), and these spawn against the user's REAL server directory.
//!
//! # Preconditions
//!
//!   1. The `dml-arch` distro exists and holds an installed WoW title.
//!   2. `/usr/local/bin/dml` (bash CLI) is installed — `cli/dev-install.ps1`.
//!   3. `/usr/local/bin/dml-wow` is installed AT MODE 0755. A 0644 binary fails
//!      `--exec` with "Permission denied", which reads like a MISSING binary,
//!      so the mode is part of the diagnosis. Build and install it with
//!      `scripts/Invoke-ArchLiveSmoke.ps1`.
//!
//! Everything in this file is READ-ONLY with respect to the server. Nothing
//! here starts, stops or restarts anything, touches the database, or runs a
//! mutating GM/SOAP verb. The one lifecycle test lives in
//! `live_arch_lifecycle.rs` and is separately opt-in.

use dml_core::envelope::Envelope;
use dml_core::runner::{DmlRunner, ARCH_BINARY, DISTRO, USER};
use serde_json::Value;

// -- harness ---------------------------------------------------------------

/// Both answers to one logical launcher command.
struct Pair {
    bash: Envelope,
    arch: Envelope,
}

/// Run `argv` — the launcher's OWN argv literals, in the bash CLI's vocabulary
/// — through both runners.
///
/// Note what is NOT reconstructed here: the argv is handed to the real
/// `DmlRunner`, so the translation table, the program swap, the `--json`
/// decision and `--exec` are all exercised as production exercises them. A test
/// that built the `dml-wow` argv itself would be testing its own arithmetic.
fn differential(argv: &[&str]) -> Pair {
    let bash = DmlRunner::default()
        .run_json(argv)
        .unwrap_or_else(|e| panic!("BASH side failed for {argv:?}: {e:?}"));
    let arch = DmlRunner::arch()
        .run_json(argv)
        .unwrap_or_else(|e| panic!("ARCH side failed for {argv:?}: {e:?}"));
    eprintln!("--- {argv:?}\n    bash: {}\n    arch: {}", brief(&bash), brief(&arch));
    Pair { bash, arch }
}

fn brief(e: &Envelope) -> String {
    let body = match &e.error {
        Some(err) => format!("{}: {}", err.code, err.message),
        None => e.data.to_string(),
    };
    let mut s = format!("ok={} {}", e.ok, body);
    if s.chars().count() > 240 {
        s = s.chars().take(240).collect::<String>() + "...";
    }
    s
}

/// A raw spawn into the distro, bypassing `DmlRunner` entirely — for CONTROLS,
/// where the point is to prove that some other route would have failed. Returns
/// (stdout, exit code).
fn raw(program_and_args: &[&str]) -> (String, i32) {
    let out = std::process::Command::new("wsl.exe")
        .args(["-d", DISTRO, "-u", USER, "--exec"])
        .args(program_and_args)
        .output()
        .unwrap_or_else(|e| panic!("raw spawn of {program_and_args:?} failed: {e}"));
    (
        dml_core::envelope::decode_wsl_output(&out.stdout),
        out.status.code().unwrap_or(-1),
    )
}

/// The installed title id, discovered rather than hardcoded, so this suite is
/// not pinned to one machine's server name.
fn title_id() -> String {
    let env = DmlRunner::default()
        .run_json(&["games", "list"])
        .expect("bash games list must answer");
    env.data["games"][0]["id"]
        .as_str()
        .expect("at least one installed title is a precondition of this suite")
        .to_string()
}

fn ok(e: &Envelope, side: &str, argv: &[&str]) {
    assert!(
        e.ok,
        "{side} returned an error envelope for {argv:?}: {:?}",
        e.error
    );
}

// -- 1. version ------------------------------------------------------------

/// Both binaries answer `version`, and the VALUES legitimately differ.
///
/// The bash `dml` reports the CLI script's own contract version (`3.0.0`);
/// `dml-wow` reports the crate version (`0.1.0`) plus `contract` and `backend`
/// fields bash has never had. So the differential here is deliberately weak —
/// what must agree is that both are reachable and both answer `ok:true` with a
/// non-empty version. Asserting equality would be asserting a falsehood.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_version_answers_on_both_backends() {
    let argv = &["version"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    for (side, e) in [("bash", &p.bash), ("arch", &p.arch)] {
        let v = e.data["version"].as_str().unwrap_or("");
        assert!(!v.is_empty(), "{side} version must be a non-empty string: {}", e.data);
    }

    // The Arch side carries the envelope contract marker; that is the field
    // whose value the two DO have to agree about in spirit, since it is what
    // makes every other comparison in this file meaningful.
    assert_eq!(
        p.arch.data["contract"], "dml-json-v3",
        "dml-wow must declare the same envelope contract the launcher parses"
    );
}

/// NON-VACUITY CONTROL for the whole suite: `dml-wow` really does reject the
/// unconditional `--json` the bash runner appends. If it quietly accepted it,
/// "the Arch runner drops --json" would be untestable from the outside and
/// every pass above would be worth less than it looks.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_dml_wow_rejects_the_json_flag_the_bash_runner_appends() {
    let (stdout, _) = raw(&[ARCH_BINARY, "version", "--json"]);
    let e: Value = serde_json::from_str(stdout.trim().lines().next().unwrap_or("{}"))
        .expect("dml-wow emits an envelope even on a clap error");
    assert_eq!(e["ok"], false, "dml-wow must refuse --json, got: {stdout}");
    assert_eq!(e["error"]["code"], "BAD_ARGS");
    assert!(
        e["error"]["message"].as_str().unwrap_or("").contains("--json"),
        "the refusal must name the flag: {stdout}"
    );
}

// -- 2. games list ---------------------------------------------------------

/// The strongest differential available: a pure filesystem question, no docker,
/// no database, no clock. The two backends must agree EXACTLY.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_games_list_agrees_across_backends() {
    let argv = &["games", "list"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);
    assert_eq!(
        p.bash.data, p.arch.data,
        "games list must be byte-identical across backends"
    );
    // Non-vacuity: an empty list would make the equality above trivially true.
    assert!(
        !p.bash.data["games"].as_array().expect("games array").is_empty(),
        "this suite needs at least one installed title"
    );
}

// -- 3. games status <id> --------------------------------------------------

#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_games_status_agrees_across_backends() {
    let id = title_id();
    let argv = &["games", "status", id.as_str()];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);
    assert_eq!(p.bash.data, p.arch.data, "games status must agree exactly");
    assert_eq!(p.arch.data["id"], id.as_str(), "the id must survive the --id promotion");
}

/// `--exec` is not a style preference, and this is the live proof: the bare
/// `wsl -- ` form runs a SHELL inside the distro, which would split this id on
/// the `;`. Under `--exec` the whole string arrives as one literal token, so
/// the id validator sees — and names — the metacharacter verbatim.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_exec_passes_a_metacharacter_id_verbatim() {
    let (stdout, _) = raw(&[ARCH_BINARY, "games-status", "--id", "a;whoami"]);
    let e: Value = serde_json::from_str(stdout.trim().lines().next().unwrap_or("{}"))
        .unwrap_or_else(|_| panic!("expected an envelope, got: {stdout}"));
    assert_eq!(e["ok"], false);
    assert_eq!(e["error"]["code"], "BAD_ID");
    assert!(
        e["error"]["message"].as_str().unwrap_or("").contains("a;whoami"),
        "the id must arrive as ONE token, shell-unsplit: {stdout}"
    );
}

// -- 4. wow server-detail --------------------------------------------------

/// Home's status card. Compared field by field rather than whole, because two
/// fields legitimately differ between two calls seconds apart: the SOAP latency
/// samples (`mean_ms`/`median_ms`) are measurements, and `bots.online` moves on
/// a running server. What must agree is the VERDICT and the container table —
/// the parts the UI renders as fact.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_server_detail_agrees_on_the_verdict_and_containers() {
    let argv = &["wow", "server-detail"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    assert_eq!(p.bash.data["verdict"], p.arch.data["verdict"], "verdict must agree");
    assert_eq!(
        p.bash.data["world_ready"], p.arch.data["world_ready"],
        "world_ready must agree"
    );

    let names = |v: &Value| -> Vec<(String, String, String)> {
        v["containers"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|c| {
                (
                    c["name"].as_str().unwrap_or("").to_string(),
                    c["role"].as_str().unwrap_or("").to_string(),
                    c["state"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    };
    let (b, a) = (names(&p.bash.data), names(&p.arch.data));
    assert_eq!(b, a, "the container table must agree in name, role and state");
    assert!(!b.is_empty(), "a server-detail with no containers proves nothing");
}

// -- 5. read-only WoW verbs that translate ---------------------------------

/// `wow client-path get` → `dml-wow client-path get`. A stored path, read from
/// the same file by both.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_client_path_get_agrees_across_backends() {
    let argv = &["wow", "client-path", "get"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);
    assert_eq!(p.bash.data, p.arch.data, "client-path get must agree exactly");
}

/// `wow cache-status` → `dml-wow cache status` (a verb RENAME across a
/// namespace boundary, so the translation is doing real work here).
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_cache_status_agrees_across_backends() {
    let argv = &["wow", "cache-status"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);
    // Compared whole: the byte/file counts are stable unless something is
    // actively writing the wowhead cache, which nothing in this suite does.
    assert_eq!(p.bash.data, p.arch.data, "cache-status must agree exactly");
}

/// `wow backup list` → `dml-wow backup list`. Reads `~/.dml/backups`.
///
/// The `name` key is asserted around rather than through: `dml-wow` emits an
/// explicit `"name": null` where bash omits the key entirely. Both decode to
/// "no name" for the UI, so the comparison is over the fields that carry
/// meaning.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_backup_list_agrees_across_backends() {
    let argv = &["wow", "backup", "list"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    let rows = |v: &Value| -> Vec<(String, u64, String, bool)> {
        v["backups"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|b| {
                (
                    b["file"].as_str().unwrap_or("").to_string(),
                    b["size"].as_u64().unwrap_or(0),
                    b["created"].as_str().unwrap_or("").to_string(),
                    b["world"].as_bool().unwrap_or(false),
                )
            })
            .collect()
    };
    assert_eq!(
        rows(&p.bash.data),
        rows(&p.arch.data),
        "the backup inventory must agree in file, size, created and world"
    );
}

// -- 5b. the cwd-dependent verbs -------------------------------------------
//
// # THE BUG THIS GROUP EXISTS FOR (found by this suite, fixed 2026-08-04)
//
// `dml_wow::config::ConfigReader::title_dir_from_env` used to resolve the title
// dir itself, falling back to `"."` — the CURRENT WORKING DIRECTORY — while the
// bash CLI falls back to `$HOME/games` (`cli/src/00-head.sh:9`) and
// `dml_core::compose::games_dir_from_env` (which `games list`/`status` use)
// falls back to `$HOME/games` too. `DML_GAMES_DIR` is UNSET inside the distro,
// and a Windows-side value does NOT cross the wsl.exe boundary (there is no
// `WSLENV` entry for it), so the fallback is what runs — and the launcher's cwd
// is never the games dir.
//
// Roughly twenty call sites route through that one helper, so the blast radius
// was the whole file-backed and DB-backed surface, in three shapes:
//
//   1. honest `NOT_FOUND` refusals (`config files`, `commands`, `accountwide`);
//   2. SILENTLY WRONG `ok:true` payloads (`config list`, `tuning list`,
//      `module list`) — registry defaults rendered as the server's settings;
//   3. DB verbs dialling 3306 instead of the port in the title's own `.env`.
//
// One test per shape below, plus `config list`. Shape 2 is why this group is
// worth its runtime: nothing about those answers looks broken. The fix made
// `title_dir_from_env` a delegation to `dml_core::compose::title_dir_for_id`,
// and `startup.rs`'s `games_dir_reader_scan_tests` pins that there is no
// second resolver to disagree with it again.

/// `wow config list` → `dml-wow config list`. The Config page's entire content.
///
/// SHAPE 2. Measured before the fix: bash reported the 3x XP/gold rates the
/// user actually runs and `dml-wow` reported 1x; `bots.population` 2000 vs 500;
/// `ahbot.character` 2501 vs 0 — eleven settings wrong, with no error, no
/// warning and no empty result to notice.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_config_list_agrees_across_backends() {
    let argv = &["wow", "config", "list"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    let vals = |v: &Value| -> Vec<(String, String)> {
        v["settings"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|s| {
                (
                    s["key"].as_str().unwrap_or("").to_string(),
                    s["value"].as_str().unwrap_or("").to_string(),
                )
            })
            .collect()
    };
    let (b, a) = (vals(&p.bash.data), vals(&p.arch.data));
    assert!(!b.is_empty(), "an empty settings list would make this vacuous");

    let drift: Vec<String> = b
        .iter()
        .zip(a.iter())
        .filter(|(x, y)| x != y)
        .map(|(x, y)| format!("{} bash={:?} arch={:?}", x.0, x.1, y.1))
        .collect();
    assert!(
        drift.is_empty(),
        "the Config page would show {} setting(s) that are NOT what the server runs \
         (see the group comment above: a second games-dir resolver):\n  {}",
        drift.len(),
        drift.join("\n  ")
    );
}

/// `wow config tuning-list` → `dml-wow tuning list`. The Modules page's
/// Lua-script knobs.
///
/// SHAPE 2, and a DIFFERENT field from `config list`: what diverged here was
/// `installed` — four `learnspells.*` rows that bash reported as installed and
/// `dml-wow` reported as absent, because the file it looks for lives under the
/// title dir. A page that greys out the controls for a module the server is
/// actually running is the same silent lie in a different costume.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_tuning_list_agrees_across_backends() {
    let argv = &["wow", "config", "tuning-list"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    let rows = |v: &Value| -> Vec<(String, String, bool)> {
        v["settings"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|s| {
                (
                    s["key"].as_str().unwrap_or("").to_string(),
                    s["value"].as_str().unwrap_or("").to_string(),
                    s["installed"].as_bool().unwrap_or(false),
                )
            })
            .collect()
    };
    let (b, a) = (rows(&p.bash.data), rows(&p.arch.data));
    assert!(!b.is_empty(), "an empty tuning list would make this vacuous");

    let drift: Vec<String> = b
        .iter()
        .zip(a.iter())
        .filter(|(x, y)| x != y)
        .map(|(x, y)| format!("{} bash=({:?},{}) arch=({:?},{})", x.0, x.1, x.2, y.1, y.2))
        .collect();
    assert!(
        drift.is_empty(),
        "the Modules page would show {} tuning row(s) that are NOT the server's:\n  {}",
        drift.len(),
        drift.join("\n  ")
    );
    // The zip above stops at the shorter list, so a truncated answer would slip
    // through as "no drift".
    assert_eq!(b.len(), a.len(), "the two backends disagree about how many tuning rows exist");
}

/// `wow module list` → `dml-wow module list`. The Modules page's inventory.
///
/// SHAPE 2, and the widest of the three: before the fix `ale_ready` read
/// `false` on a server whose ALE is installed and working, seventeen cpp fields
/// disagreed (`conf` state, `head`, `head_date`), and the two backends did not
/// even agree on how many cpp modules EXIST — bash found 20 and `dml-wow` 19,
/// because a user-added custom module is discovered by looking in the title
/// dir. `rebuild_pending` is compared too: it is what the page uses to decide
/// whether to nag about a rebuild.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_module_list_agrees_across_backends() {
    let argv = &["wow", "module", "list"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    assert_eq!(
        p.bash.data["ale_ready"], p.arch.data["ale_ready"],
        "ale_ready decides whether the Lua-module half of the page is usable at all"
    );
    assert_eq!(
        p.bash.data["rebuild_pending"], p.arch.data["rebuild_pending"],
        "rebuild_pending must agree"
    );

    // Compared per family and per row so a failure NAMES the module. `installed`
    // / `conf` / `head` are the title-dir-derived fields; `name`/`desc`/`url`
    // come from the embedded registry and could not diverge.
    let mut checked = 0usize;
    for family in ["cpp", "lua", "sql"] {
        let pick = |v: &Value| -> Vec<Value> {
            v["families"][family].as_array().cloned().unwrap_or_default()
        };
        let (b, a) = (pick(&p.bash.data), pick(&p.arch.data));
        let keys = |rows: &[Value]| -> Vec<String> {
            rows.iter().map(|r| r["key"].as_str().unwrap_or("").to_string()).collect()
        };
        assert_eq!(
            keys(&b),
            keys(&a),
            "the {family} module inventory itself differs (a custom module is found by \
             looking in the title dir, so a wrong title dir loses it)"
        );
        for (x, y) in b.iter().zip(a.iter()) {
            let key = x["key"].as_str().unwrap_or("");
            for field in ["installed", "conf", "head", "head_date", "pending_rebuild", "custom",
                          "cloned", "deployed", "has_sql", "warn"] {
                if x.get(field).is_none() && y.get(field).is_none() {
                    continue; // field belongs to another family's row shape
                }
                assert_eq!(
                    x.get(field),
                    y.get(field),
                    "{family}/{key}.{field} disagrees: bash={:?} arch={:?}",
                    x.get(field),
                    y.get(field)
                );
                checked += 1;
            }
        }
    }
    // Non-vacuity: a comparison loop that ran zero times proves nothing, and
    // `families` renaming a key would make every `pick` above return empty.
    assert!(checked >= 50, "only {checked} module field(s) compared — the picker is broken");
}

/// `wow config files` → `dml-wow config files`. The Modules page's raw editor.
///
/// SHAPE 1, the honest half of the bug: before the fix this refused with
/// `NOT_FOUND` / "WoW Playerbots server not installed" while bash listed the
/// files. A refusal is a far better failure than shape 2 — the user sees that
/// something is wrong — but it is still wrong, and it is the same one line.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_config_files_agrees_across_backends() {
    let argv = &["wow", "config", "files"];
    let p = differential(argv);
    ok(&p.bash, "bash", argv);
    ok(&p.arch, "arch", argv);

    let rows = |v: &Value| -> Vec<(String, bool, bool, bool)> {
        v["files"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|f| {
                (
                    f["name"].as_str().unwrap_or("").to_string(),
                    f["exists"].as_bool().unwrap_or(false),
                    f["dist"].as_bool().unwrap_or(false),
                    f["readonly"].as_bool().unwrap_or(false),
                )
            })
            .collect()
    };
    let (b, a) = (rows(&p.bash.data), rows(&p.arch.data));
    assert!(!b.is_empty(), "an empty file list would make this vacuous");
    assert_eq!(b, a, "the raw-editor file list must agree in name, exists, dist and readonly");
    // `exists` is the field the title dir decides; all-false would mean the
    // comparison agreed about a directory neither backend can see.
    assert!(
        b.iter().any(|f| f.1),
        "no config file reported as existing — both backends are looking at the wrong place"
    );
}

/// `wow stats` → `dml-wow stats`. SHAPE 3: the DB verbs, and the whole
/// Dashboard / Item DB / Characters / Bots surface behind them.
///
/// `dml-wow` reads MySQL over TCP and takes host, port and password from the
/// title dir's own `.env` (`db::DbConfig::from_env` → `dotenv_path` →
/// `ConfigReader::title_dir_from_env`). With the title dir unresolved that file
/// is unreadable, so every value fell back to the compose default and the
/// launcher dialled **3306** — while this server publishes MySQL on the port
/// its `.env` names. Nothing else in this suite would have caught it: the
/// verdict is `DB_UNREACHABLE` either way.
///
/// So the assertion is on the ADDRESS, which `dml-wow` puts in its own error
/// message, against the port read out of the title's `.env`. That works whether
/// the server is up or down — this suite may not start it. When the server IS
/// up the two backends must simply agree that the DB is reachable.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_stats_dials_the_db_port_from_the_title_dir() {
    let id = title_id();
    // Ground truth, discovered rather than hardcoded. `--exec` runs no shell, so
    // `sh -c` is explicit; the id arrives as `$1` rather than spliced into the
    // script, so it cannot break out even though `games list` already validated
    // it.
    let (dotenv, _) = raw(&[
        "sh",
        "-c",
        "cat \"$HOME/games/$1/.env\" 2>/dev/null || true",
        "sh",
        id.as_str(),
    ]);
    let port = dotenv
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| l.strip_prefix("DOCKER_DB_EXTERNAL_PORT="))
        .or_else(|| {
            dotenv
                .lines()
                .map(str::trim)
                .filter(|l| !l.starts_with('#'))
                .find_map(|l| l.strip_prefix("DB_EXTERNAL_PORT="))
        })
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .unwrap_or_default();

    // NON-VACUITY, and the reason this is an assertion rather than a skip: if
    // the title's `.env` does not remap the port, the resolved answer and the
    // hardcoded default are the same number and this test cannot tell them
    // apart. Saying so out loud beats passing on a machine where it proves
    // nothing.
    assert!(
        !port.is_empty() && port != "3306",
        "this test needs a title `.env` that remaps the DB port away from the 3306 default; \
         found {port:?}. Without that, 'dialled the resolved port' and 'dialled the compiled \
         default' are indistinguishable."
    );

    let argv = &["wow", "stats"];
    let p = differential(argv);

    match (&p.bash.error, &p.arch.error) {
        // Server up: both must simply be able to read it.
        (None, None) => {
            assert!(p.bash.ok && p.arch.ok, "both backends answered without an error object");
        }
        // Server down (this suite may not start it): the verdict must agree,
        // and the address `dml-wow` tried must be the one the title configures.
        _ => {
            let bcode = p.bash.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
            let acode = p.arch.error.as_ref().map(|e| e.code.as_str()).unwrap_or("");
            assert_eq!(bcode, acode, "the two backends disagree about WHY stats failed");
            assert_eq!(acode, "DB_UNREACHABLE", "unexpected failure mode: {:?}", p.arch.error);
            let msg = p.arch.error.as_ref().map(|e| e.message.clone()).unwrap_or_default();
            assert!(
                msg.contains(&format!("127.0.0.1:{port}")),
                "dml-wow dialled the wrong address. The title's .env says port {port}, but the \
                 error names: {msg}"
            );
            assert!(
                !msg.contains("127.0.0.1:3306"),
                "dml-wow fell back to the compiled 3306 default — the title dir is unresolved \
                 again: {msg}"
            );
        }
    }
}

// -- 6. the bash fallback --------------------------------------------------

/// THE SAFETY PROPERTY, proven end to end for the first time.
///
/// 33 of the launcher's call sites have no `dml-wow` arm and route to the bash
/// `dml` in the SAME distro. Nothing had ever confirmed that the swap actually
/// lands — a unit test can only show which `Target` the table returns.
///
/// The proof is three-part, and the third part is what makes it non-vacuous:
///
///   1. the Arch runner answers `ok:true` with real pb-keys data;
///   2. its answer equals the Bash runner's, byte for byte — same program,
///      same argv, so anything else would mean it reached somewhere else;
///   3. CONTROL: `dml-wow` handed this very argv emits `ok:false` /
///      `BAD_ARGS` / "unrecognized subcommand 'wow'". So (1) could not have
///      been produced by the wrong target. Without this, "the envelope parsed"
///      would be satisfied by a clap refusal, which is also a valid envelope —
///      a probe whose failure mode looks like its negative answer is not a
///      probe (CLAUDE.md).
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_an_unmapped_verb_really_reaches_the_bash_cli() {
    let argv = &["wow", "config", "pb-keys"];
    let p = differential(argv);

    // (1) and (2)
    ok(&p.arch, "arch", argv);
    assert_eq!(
        p.bash.data, p.arch.data,
        "a Bash-target verb must produce the bash CLI's own answer, unchanged"
    );
    assert!(
        !p.arch.data["keys"].as_array().expect("pb-keys returns keys").is_empty(),
        "an empty key list would make this comparison vacuous"
    );

    // (3) the control
    let (stdout, _) = raw(&[ARCH_BINARY, "wow", "config", "pb-keys"]);
    let ctl: Value = serde_json::from_str(stdout.trim().lines().next().unwrap_or("{}"))
        .unwrap_or_else(|_| panic!("expected an envelope from the control, got: {stdout}"));
    assert_eq!(
        ctl["ok"], false,
        "CONTROL BROKEN: dml-wow now accepts `wow config pb-keys`, so the test \
         above no longer distinguishes the two targets"
    );
    assert_eq!(ctl["error"]["code"], "BAD_ARGS");
}

/// The same fallback, on the TEXT path (`run_captured`, no `--json` ever). A
/// `doctor` that reached `dml-wow` would print clap's "unrecognized subcommand"
/// instead of the report.
#[test]
#[ignore = "spawns into the real dml-arch distro; run with --ignored"]
fn live_a_text_mode_verb_falls_back_to_bash_too() {
    let text = DmlRunner::arch()
        .run_captured(&["doctor"])
        .expect("doctor must answer");
    eprintln!("--- doctor (arch runner, text mode)\n{}", text.lines().take(6).collect::<Vec<_>>().join("\n"));
    assert!(
        !text.contains("unrecognized subcommand"),
        "`doctor` reached dml-wow instead of the bash CLI: {text}"
    );
    assert!(
        text.contains("dml") || text.contains("Docker") || text.contains("docker"),
        "expected the bash doctor's report, got: {text}"
    );
}
