//! Property tests for the native compose/override/build generator
//! (`dml_wow::composegen`, native-first-install plan Task 2).
//!
//! Deliberately NOT golden-file diffs of whole YAML documents: the reviewer's
//! standing instruction is that formatting churn must not be a test failure.
//! Every assertion below is a PROPERTY the rest of the codebase actually
//! depends on, each one traced to the call site that would break:
//!
//! * `ac-*` container names -> `backup.rs`'s `docker exec ac-database
//!   mysqldump`, `restore`/`world-restart`'s `docker restart ac-worldserver`.
//! * DB published on loopback with the port `db.rs::resolve_db_config`
//!   resolves -> every native character/item/stats page.
//! * SOAP env present -> GM tools, My Party, the console, `soap.rs`.
//! * `./modules:/azerothcore/modules` -> the playerbots DB updater shuts the
//!   worldserver down at boot without it (poc README, found live).
//! * `${IMAGE_TAG:-…}` seam intact and `build:` confined to the build file ->
//!   Route B stays a compose-variable swap instead of a rewrite.
//!
//! Ground truth for the shape is the WORKING migrated server at
//! `C:/Users/perzi/dml-native/wow-server-playerbots` (it runs, so its layout
//! is proof rather than a guess), plus the two mechanisms in this repo that
//! read/write these files: `crates/dml-wow/src/config.rs` (the override's
//! owner) and `cli/src/90-main.sh`'s `soap-setup` / port-conflict remedy.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::composegen::{
    self, ComposeOpts, Ports, Rates, BASE_FILE, BUILD_FILE, OVERRIDE_FILE,
};
use serde_yaml_ng::Value;

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Per-test scratch dir under the platform temp dir, matching the convention
/// the existing `dml-wow` suites use (no `tempfile` dev-dependency in this
/// workspace). Recreated from scratch so a leftover from a previous run can
/// never make a test pass or fail for the wrong reason.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dml-composegen-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// A stand-in title dir for the tests that care about the CONTENT of a render
/// rather than about identity. It need not exist: `render_base` only derives
/// the compose project name and image tag from the path text (see the identity
/// section at the bottom of this file for the tests that do care). Forward
/// slashes on purpose — a `C:\…` literal would be Windows-only.
fn title() -> PathBuf {
    PathBuf::from("/games/wow-server-playerbots")
}

fn parse(text: &str) -> Value {
    serde_yaml_ng::from_str(text).unwrap_or_else(|e| panic!("rendered YAML must parse: {e}\n{text}"))
}

fn service<'a>(doc: &'a Value, name: &str) -> &'a Value {
    doc.get("services")
        .and_then(|s| s.get(name))
        .unwrap_or_else(|| panic!("service {name} missing from\n{doc:#?}"))
}

fn seq_strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_sequence)
        .map(|s| s.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Every `build` key anywhere in the document, as dotted paths — the
/// separation test's oracle. Walks the whole tree rather than checking the
/// four services we happen to know about, so a `build:` smuggled in anywhere
/// (a fifth service, an `x-` anchor) is still caught.
fn build_key_paths(v: &Value, path: String, out: &mut Vec<String>) {
    match v {
        Value::Mapping(m) => {
            for (k, val) in m {
                let key = k.as_str().unwrap_or("<non-string>");
                let child = if path.is_empty() { key.to_string() } else { format!("{path}.{key}") };
                if key == "build" {
                    out.push(child.clone());
                }
                build_key_paths(val, child, out);
            }
        }
        Value::Sequence(s) => {
            for (i, val) in s.iter().enumerate() {
                build_key_paths(val, format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn builds_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    build_key_paths(&parse(text), String::new(), &mut out);
    out.sort();
    out
}

fn env_map(text: &str, svc: &str) -> BTreeMap<String, String> {
    let doc = parse(text);
    service(&doc, svc)
        .get("environment")
        .and_then(Value::as_mapping)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| {
                    Some((k.as_str()?.to_string(), v.as_str().map(str::to_string).unwrap_or_default()))
                })
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// docker-compose.yml — the base file
// ---------------------------------------------------------------------------

/// Container names are a hard contract: `backup.rs:641` builds `docker exec
/// ac-database mysqldump`, `restore`/`modmgr` do `docker … ac-worldserver`,
/// and `config::env_frozen` inspects the literal container `ac-worldserver`.
/// The poc template's `dml-native-*` names are exactly the committed bug the
/// plan's Task 9 has to fix — this test is the tripwire that keeps them from
/// coming back through the generator.
#[test]
fn base_uses_the_ac_container_names_the_rest_of_the_code_addresses() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    for (svc, container) in [
        ("ac-database", "ac-database"),
        ("ac-db-import", "ac-db-import"),
        ("ac-client-data-init", "ac-client-data-init"),
        ("ac-authserver", "ac-authserver"),
        ("ac-worldserver", "ac-worldserver"),
    ] {
        assert_eq!(
            service(&doc, svc).get("container_name").and_then(Value::as_str),
            Some(container),
            "service {svc} must keep container_name {container}"
        );
    }
    assert!(
        !text.contains("dml-native-"),
        "the stale poc container-name prefix must never appear"
    );
}

/// Route B ("publish a prebuilt playerbots worldserver image") stays a
/// compose-variable swap only while the four acore images keep the
/// `${IMAGE_TAG:-…}` seam. A hardcoded tag designs Route B out.
///
/// The prefix is read from the module's own constant, not restated: the DEFAULT
/// deliberately is NOT upstream's org (see
/// `a_build_can_never_clobber_the_image_refs_another_stack_already_runs`), and a
/// literal here would have to be edited in lockstep with the thing it guards.
#[test]
fn base_image_tags_carry_the_route_b_seam() {
    let dir = title();
    let text = composegen::render_base(&dir, &ComposeOpts::default()).unwrap();
    let tag = composegen::image_tag_for(&dir);
    for svc in ["worldserver", "authserver", "db-import", "client-data"] {
        let want =
            format!("{}{svc}:${{IMAGE_TAG:-{tag}}}", composegen::DEFAULT_IMAGE_PREFIX);
        assert!(text.contains(&want), "missing image seam {want} in\n{text}");
    }
    // The DB is a stock upstream image, not an acore build product.
    assert!(text.contains("image: mysql:8.4"), "ac-database must run stock mysql:8.4");
}

/// The tag option must move the seam's DEFAULT, never replace the seam — and it
/// must still be settable, because the MIGRATION path has to run the exact refs
/// `docker load` created (`acore/ac-wotlk-*:master`) rather than a build tag.
#[test]
fn image_tag_option_moves_the_default_inside_the_seam_and_never_hardcodes_it() {
    let opts = ComposeOpts { image_tag: Some("v1.2.3".into()), ..Default::default() };
    let text = composegen::render_base(&title(), &opts).unwrap();
    for svc in ["worldserver", "authserver", "db-import", "client-data"] {
        let want = format!("{}{svc}:${{IMAGE_TAG:-v1.2.3}}", composegen::DEFAULT_IMAGE_PREFIX);
        assert!(text.contains(&want), "missing tagged seam {want} in\n{text}");
    }
    assert!(
        text.matches("${IMAGE_TAG:-").count() == 4,
        "all four acore images keep the seam"
    );

    // The migration's shape: upstream prefix + the loaded tag, verbatim.
    let migrated = ComposeOpts {
        image_prefix: composegen::UPSTREAM_IMAGE_PREFIX.to_string(),
        image_tag: Some("master".into()),
        ..Default::default()
    };
    let text = composegen::render_base(&title(), &migrated).unwrap();
    assert!(
        text.contains("acore/ac-wotlk-worldserver:${IMAGE_TAG:-master}"),
        "an explicit prefix+tag must survive intact for the loaded images:\n{text}"
    );
}

/// `db.rs` connects to `127.0.0.1` and resolves the port from
/// `DOCKER_DB_EXTERNAL_PORT` FIRST (`resolve_db_config`), which is also the
/// key the CLI's port-conflict remedy appends to `.env`
/// (`lifecycle.rs:249`, `90-main.sh:534`). Publish under any other variable
/// name and the remedy becomes a no-op that silently breaks every DB page.
#[test]
fn db_is_published_on_loopback_under_the_variable_the_native_reader_resolves() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    let ports = seq_strings(service(&doc, "ac-database").get("ports"));
    assert_eq!(
        ports,
        vec!["127.0.0.1:${DOCKER_DB_EXTERNAL_PORT:-3306}:3306".to_string()],
        "DB must publish loopback-only under DOCKER_DB_EXTERNAL_PORT"
    );
    // The password interpolation must match what db.rs falls back to.
    let env = env_map(&text, "ac-database");
    assert_eq!(
        env.get("MYSQL_ROOT_PASSWORD").map(String::as_str),
        Some("${DB_ROOT_PASSWORD:-password}")
    );
}

/// A non-default DB port or password is only real if BOTH surfaces agree:
/// compose must publish it AND `db.rs::resolve_db_config` must resolve the same
/// values from the `.env` **that the generator actually leaves on disk**.
///
/// The `.env` half is deliberately driven through `write_all` and read back
/// from the filesystem: joining `dotenv_lines` in-process would assert an
/// integration that does not exist. `db::dotenv_path()` reads exactly
/// `<title_dir>/.env` (`ConfigReader::title_dir_from_env().join(".env")`), so
/// the file this test reads is the file production reads — and if nothing
/// writes it, every native DB page reports DB_UNREACHABLE and every
/// `mysqldump` in `backup`/`restore`/`lifecycle` authenticates with the wrong
/// password.
#[test]
fn custom_db_port_reaches_compose_and_the_real_db_resolver_identically() {
    let dir = scratch("dbcreds").join(dml_wow::config::TITLE);
    let opts = ComposeOpts {
        ports: Ports { db: 13307, ..Ports::default() },
        db_password: "s3cret".into(),
        ..Default::default()
    };
    composegen::write_all(&dir, &opts).unwrap();

    let text = std::fs::read_to_string(dir.join(BASE_FILE)).unwrap();
    assert!(
        text.contains("127.0.0.1:${DOCKER_DB_EXTERNAL_PORT:-13307}:3306"),
        "compose must publish the chosen port"
    );

    let dotenv = std::fs::read_to_string(dir.join(".env")).unwrap_or_else(|e| {
        panic!("write_all must leave a .env where db.rs looks for it ({e})")
    });
    let cfg = dml_wow::db::resolve_db_config(|_| None, Some(&dotenv));
    assert_eq!(cfg.port, 13307, "db.rs must resolve the same port from the generated .env");
    assert_eq!(cfg.password, "s3cret", "db.rs must resolve the same password");
    assert_eq!(cfg.host, "127.0.0.1");

    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// `.env` is SHARED property, not a generated artifact: `dml wow soap-setup`
/// writes `DOCKER_SOAP_EXTERNAL_PORT` into it and the CLI's port-conflict
/// remedy appends `DOCKER_DB_EXTERNAL_PORT=13306` when 3306 is taken. An
/// install RESUME re-runs `write_all`, so overwriting the file would silently
/// undo both — the server would come back on a port nothing published.
#[test]
fn write_all_merges_into_an_existing_dot_env_instead_of_replacing_it() {
    let dir = scratch("dotenvmerge").join(dml_wow::config::TITLE);
    std::fs::create_dir_all(&dir).unwrap();
    let pre = "# written by the port-conflict remedy\nDOCKER_DB_EXTERNAL_PORT=13306\n";
    std::fs::write(dir.join(".env"), pre).unwrap();

    // Default options emit no lines at all — the file must survive untouched.
    let gen = composegen::write_all(&dir, &ComposeOpts::default()).unwrap();
    assert!(gen.dotenv.is_none(), "default options need no .env write");
    assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), pre);

    // A generated key lands without evicting the sibling the CLI put there.
    let opts = ComposeOpts { db_password: "s3cret".into(), ..Default::default() };
    let gen = composegen::write_all(&dir, &opts).unwrap();
    assert_eq!(gen.dotenv.as_deref(), Some(dir.join(".env").as_path()));
    let after = std::fs::read_to_string(dir.join(".env")).unwrap();
    let cfg = dml_wow::db::resolve_db_config(|_| None, Some(&after));
    assert_eq!(cfg.port, 13306, "the remedy's port must survive an install resume:\n{after}");
    assert_eq!(cfg.password, "s3cret");
    assert!(after.contains("# written by the port-conflict remedy"), "{after}");

    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// Writing `DOCKER_DB_EXTERNAL_PORT` into `.env` unconditionally would DISABLE
/// the 3306 auto-remap: `db_port_conflict_message` stays silent whenever the
/// key is already present (`env_has_db_external_port`). So default options must
/// emit no `.env` at all — and `write_all` must not create one either, which is
/// the half that actually reaches the disk the remedy inspects.
#[test]
fn default_options_emit_no_dot_env_so_the_3306_remap_remedy_still_fires() {
    let dir = scratch("nodotenv").join(dml_wow::config::TITLE);
    let gen = composegen::write_all(&dir, &ComposeOpts::default()).unwrap();
    assert!(gen.dotenv.is_none(), "default options must not pin any .env key");
    assert!(!dir.join(".env").exists(), "no .env may be created for default options");

    // Positive control, so the assertion above cannot pass vacuously: a
    // NON-default port does pin the key, and the remedy then correctly goes
    // quiet because the port is already chosen.
    let opts = ComposeOpts { ports: Ports { db: 13307, ..Ports::default() }, ..Default::default() };
    composegen::write_all(&dir, &opts).unwrap();
    let text = std::fs::read_to_string(dir.join(".env")).unwrap();
    assert!(
        dml_wow::lifecycle::env_has_db_external_port(&text),
        "a pinned port must be visible to the remedy's own check: {text:?}"
    );

    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// Auth/world are the ports a game client connects to from the LAN, so they
/// must NOT be loopback-pinned — the opposite of the DB/SOAP posture.
#[test]
fn game_ports_stay_reachable_while_admin_ports_stay_loopback() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    assert_eq!(
        seq_strings(service(&doc, "ac-authserver").get("ports")),
        vec!["${DOCKER_AUTH_EXTERNAL_PORT:-3724}:3724".to_string()]
    );
    let world = seq_strings(service(&doc, "ac-worldserver").get("ports"));
    assert!(world.contains(&"${DOCKER_WORLD_EXTERNAL_PORT:-8085}:8085".to_string()), "{world:?}");
    // SOAP: loopback is carried in the DEFAULT VALUE, not as an IP prefix on
    // the mapping — `dml wow soap-setup` writes the whole host binding into
    // .env (`DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878`), so a literal prefix
    // here would render `127.0.0.1:127.0.0.1:7878:7878`.
    assert!(
        world.contains(&"${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7878}:7878".to_string()),
        "SOAP must publish loopback via the variable default: {world:?}"
    );
}

/// `config.rs::conf_path_in` reads `env/dist/etc/**` from the HOST side, so
/// the etc/logs binds are what makes the whole config system work at all.
#[test]
fn base_binds_the_env_dist_tree_the_config_system_reads() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    for svc in ["ac-worldserver", "ac-authserver", "ac-db-import"] {
        let vols = seq_strings(service(&doc, svc).get("volumes"));
        assert!(
            vols.iter().any(|v| v.starts_with("./env/dist/etc:")),
            "{svc} must bind ./env/dist/etc, got {vols:?}"
        );
    }
    let world = seq_strings(service(&doc, "ac-worldserver").get("volumes"));
    assert!(
        world.iter().any(|v| v.starts_with("client-data:")),
        "worldserver must mount the client-data volume: {world:?}"
    );
}

/// `docker attach` (the Task 11 account step) needs stdin+tty, and the
/// 5-minute stop grace is what stops compose SIGKILLing 1600 bots mid-save.
/// Both are ground truth from the working server.
#[test]
fn worldserver_keeps_the_console_and_the_shutdown_grace_period() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    let ws = service(&doc, "ac-worldserver");
    assert_eq!(ws.get("stdin_open").and_then(Value::as_bool), Some(true));
    assert_eq!(ws.get("tty").and_then(Value::as_bool), Some(true));
    assert_eq!(ws.get("stop_grace_period").and_then(Value::as_str), Some("5m"));
}

/// The base file must always pin a project name (never leave compose to guess
/// it from the directory basename — the title id is fixed, so every install
/// would guess the SAME one), and that name is the one the identity derivation
/// produces for this dir.
#[test]
fn base_pins_the_compose_project_name_derived_from_the_title_dir() {
    let dir = title();
    let text = composegen::render_base(&dir, &ComposeOpts::default()).unwrap();
    assert_eq!(
        parse(&text).get("name").and_then(Value::as_str),
        Some(composegen::project_name_for(&dir).as_str())
    );
}

/// `restart: unless-stopped` is not cosmetic here — it is the ONLY thing that
/// makes `.State.RestartCount` climb, and that counter is the whole input to
/// the 2026-07-28 boot-loop diagnosis (`lifecycle::BootLoopWatch` on the Rust
/// surface, `_boot_loop_check` in `cli/src/40-config.sh`). A worldserver that
/// dies once and stays dead never increments it, so the incident follow-up
/// that names the cause and offers "Restart Docker" can never fire on a
/// native install: the user gets a stopped server and no explanation.
///
/// The one-shot services are the mirror image: `ac-db-import` and
/// `ac-client-data-init` are depended on with `service_completed_successfully`,
/// and a restart policy on a container that is SUPPOSED to exit turns a
/// completed import into an endless re-import that never satisfies the
/// condition, so the worldserver never starts.
#[test]
fn long_running_services_restart_and_one_shot_services_deliberately_do_not() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    for svc in ["ac-database", "ac-authserver", "ac-worldserver"] {
        assert_eq!(
            service(&doc, svc).get("restart").and_then(Value::as_str),
            Some("unless-stopped"),
            "{svc} must restart — RestartCount is the boot-loop watch's only signal"
        );
    }
    for svc in ["ac-db-import", "ac-client-data-init"] {
        assert_eq!(
            service(&doc, svc).get("restart"),
            None,
            "{svc} is a one-shot depended on with service_completed_successfully; \
             a restart policy makes it loop and the worldserver never starts"
        );
    }
}

/// Every service that declares `condition: service_healthy` on another
/// service is a promise that the OTHER service defines a `healthcheck` —
/// Compose rejects the dependency outright otherwise, and both migration
/// scripts poll `{{.State.Health.Status}}` and would spin forever on
/// "database never became healthy".
///
/// Derived from the document rather than restated: the set of depended-on
/// services is computed from the real `depends_on` blocks, so deleting the
/// healthcheck fails here, and deleting the `depends_on` blocks empties the
/// set and fails the sanity assert.
#[test]
fn every_service_healthy_dependency_is_backed_by_a_real_healthcheck() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    let services = doc.get("services").and_then(Value::as_mapping).expect("services map");

    let mut needs_health: Vec<String> = Vec::new();
    for (_name, body) in services {
        let Some(deps) = body.get("depends_on").and_then(Value::as_mapping) else { continue };
        for (dep, spec) in deps {
            if spec.get("condition").and_then(Value::as_str) == Some("service_healthy") {
                let dep = dep.as_str().expect("dependency name is a string").to_string();
                if !needs_health.contains(&dep) {
                    needs_health.push(dep);
                }
            }
        }
    }
    needs_health.sort();
    assert_eq!(
        needs_health,
        vec!["ac-database".to_string()],
        "the schema import and the authserver must wait for a HEALTHY database"
    );

    for dep in &needs_health {
        let hc = service(&doc, dep)
            .get("healthcheck")
            .unwrap_or_else(|| panic!("{dep} is depended on as service_healthy but defines no healthcheck"));
        let test = hc.get("test").and_then(Value::as_str).unwrap_or_default();
        assert!(
            test.contains("mysql") && test.contains("SHOW DATABASES"),
            "{dep}'s healthcheck must actually query MySQL, got {test:?}"
        );
        // Without retries the check fails once during the (slow) first boot and
        // the container is marked unhealthy forever.
        assert!(
            hc.get("retries").and_then(Value::as_u64).unwrap_or(0) >= 10,
            "{dep}'s healthcheck needs enough retries to span a cold first boot: {hc:#?}"
        );
    }
}

/// The worldserver's `depends_on` is what stops it racing the schema import
/// and the client-data extraction. Without it Compose starts all five at once
/// and the worldserver connects to a half-imported `acore_world` (or an empty
/// data dir) and dies — the failure a first-time installer cannot diagnose.
#[test]
fn worldserver_waits_for_the_database_the_schema_import_and_the_client_data() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    let deps = service(&doc, "ac-worldserver")
        .get("depends_on")
        .and_then(Value::as_mapping)
        .expect("ac-worldserver must declare depends_on");
    let mut got: Vec<(String, String)> = deps
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().unwrap_or_default().to_string(),
                v.get("condition").and_then(Value::as_str).unwrap_or_default().to_string(),
            )
        })
        .collect();
    got.sort();
    assert_eq!(
        got,
        vec![
            ("ac-client-data-init".to_string(), "service_completed_successfully".to_string()),
            ("ac-database".to_string(), "service_healthy".to_string()),
            ("ac-db-import".to_string(), "service_completed_successfully".to_string()),
        ],
        "the worldserver must not race the import or the client-data extraction"
    );
}

/// A service that is told to write its logs to `AC_LOGS_DIR` must have that
/// exact container path bound back to the host, or the logs live only inside
/// the container and a `compose down` (which recreates it) destroys them —
/// the same evidence-loss the 2026-07-28 incident follow-up added the
/// worldserver log snapshot for. Server.log/DBErrors.log are the only record
/// of WHY a boot failed.
///
/// Derived from each service's own `AC_LOGS_DIR` value rather than a
/// hardcoded path, so moving the env var and forgetting the bind fails here.
#[test]
fn every_service_that_declares_ac_logs_dir_binds_it_back_to_the_host() {
    let text = composegen::render_base(&title(), &ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    let services = doc.get("services").and_then(Value::as_mapping).expect("services map");

    let mut checked = 0;
    for (name, body) in services {
        let name = name.as_str().unwrap_or_default();
        let Some(dir) = body
            .get("environment")
            .and_then(|e| e.get("AC_LOGS_DIR"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let vols = seq_strings(body.get("volumes"));
        assert!(
            vols.iter().any(|v| v.split(':').nth(1) == Some(dir)),
            "{name} writes logs to {dir} but never binds it out; got {vols:?}"
        );
        checked += 1;
    }
    assert_eq!(
        checked, 3,
        "ac-db-import, ac-authserver and ac-worldserver all declare AC_LOGS_DIR"
    );
}

// ---------------------------------------------------------------------------
// docker-compose.override.yml — the config system's property
// ---------------------------------------------------------------------------

/// The single most expensive omission available here: the Linux installer's
/// override has no `AC_SOAP_*`, and a Route A install that copied it would
/// ship a launcher whose GM tools, My Party and console are all dead with no
/// visible cause. The working server has it; so must we.
#[test]
fn soap_is_enabled_by_default_in_the_override() {
    let text = composegen::render_override(&ComposeOpts::default()).unwrap();
    let env = env_map(&text, "ac-worldserver");
    assert_eq!(env.get("AC_SOAP_ENABLED").map(String::as_str), Some("1"));
    assert_eq!(env.get("AC_SOAP_IP").map(String::as_str), Some("0.0.0.0"));
    assert_eq!(env.get("AC_SOAP_PORT").map(String::as_str), Some("7878"));
}

/// SOAP off is a real option, but it must only drop the SOAP trio — the
/// playerbots DB-updater switch is not negotiable.
#[test]
fn soap_off_drops_only_the_soap_trio() {
    let opts = ComposeOpts { soap: false, ..Default::default() };
    let text = composegen::render_override(&opts).unwrap();
    let env = env_map(&text, "ac-worldserver");
    assert!(!env.keys().any(|k| k.starts_with("AC_SOAP_")), "{env:?}");
    assert_eq!(env.get("AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES").map(String::as_str), Some("1"));
    let doc = parse(&text);
    assert!(seq_strings(service(&doc, "ac-worldserver").get("volumes"))
        .contains(&"./modules:/azerothcore/modules".to_string()));
}

/// Without this mount the playerbots DB updater shuts the worldserver down at
/// boot — found live during the migration, and the reason the mount lives in
/// the override next to the env the config system already owns.
#[test]
fn override_carries_the_load_bearing_modules_mount() {
    let text = composegen::render_override(&ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    assert_eq!(
        seq_strings(service(&doc, "ac-worldserver").get("volumes")),
        vec!["./modules:/azerothcore/modules".to_string()]
    );
}

/// Bot counts and rates map to CURATED registry rows the config editor writes
/// into `playerbots.conf`/`worldserver.conf`; an `AC_*` env var shadows the
/// conf file, which is why `config.rs` calls those env keys LEGACY and strips
/// them (`cfgset_clean_legacy_env`). So they must be opt-in, never default.
#[test]
fn bot_counts_and_rates_are_opt_in() {
    let default_env = env_map(&composegen::render_override(&ComposeOpts::default()).unwrap(), "ac-worldserver");
    for k in [
        "AC_AI_PLAYERBOT_MIN_RANDOM_BOTS",
        "AC_AI_PLAYERBOT_MAX_RANDOM_BOTS",
        "AC_RATE_XP_KILL",
        "AC_RATE_XP_QUEST",
        "AC_RATE_DROP_MONEY",
    ] {
        assert!(!default_env.contains_key(k), "{k} must not be emitted by default");
    }

    let opts = ComposeOpts {
        bots: Some((1600, 2000)),
        rates: Some(Rates { xp_kill: "3".into(), xp_quest: "3".into(), drop_money: "3".into() }),
        ..Default::default()
    };
    let env = env_map(&composegen::render_override(&opts).unwrap(), "ac-worldserver");
    assert_eq!(env.get("AC_AI_PLAYERBOT_MIN_RANDOM_BOTS").map(String::as_str), Some("1600"));
    assert_eq!(env.get("AC_AI_PLAYERBOT_MAX_RANDOM_BOTS").map(String::as_str), Some("2000"));
    assert_eq!(env.get("AC_RATE_XP_KILL").map(String::as_str), Some("3"));
    assert_eq!(env.get("AC_RATE_XP_QUEST").map(String::as_str), Some("3"));
    assert_eq!(env.get("AC_RATE_DROP_MONEY").map(String::as_str), Some("3"));
}

/// Tripwire on the shadowing class as a whole, computed from the REAL
/// registry + the REAL `env_name_for` derivation rather than a restated list:
/// exactly ONE default key is allowed to collide with a curated conf row
/// (`AiPlayerbot.RandomBotAutologin`, kept because both ground-truth sources
/// — the Linux installer's override and the working migrated server — set
/// it). Any new collision fails here.
#[test]
fn default_override_shadows_exactly_one_documented_registry_row() {
    let owned: std::collections::BTreeSet<String> = dml_wow::registry::config_registry_rows()
        .iter()
        .filter_map(|r| r.get("env").and_then(|e| e.as_str()))
        .filter(|e| e.starts_with("conf:"))
        .map(|e| dml_wow::config::env_name_for(e.rsplit(':').next().unwrap()))
        .collect();
    // Sanity: the oracle is real (a typo'd derivation would silently empty it).
    assert!(owned.contains("AC_RATE_XP_KILL"), "registry oracle looks wrong: {owned:?}");

    let env = env_map(&composegen::render_override(&ComposeOpts::default()).unwrap(), "ac-worldserver");
    let collisions: Vec<&String> = env.keys().filter(|k| owned.contains(*k)).collect();
    assert_eq!(
        collisions,
        vec!["AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN"],
        "only the documented autologin exception may shadow a curated setting"
    );
}

/// THE round-trip that protects the migration's "biggest lesson": the config
/// writer owns this file, restyles it and drops comments — the mount and
/// every sibling env key must still be there afterwards.
#[test]
fn config_writer_round_trip_preserves_the_mount_and_every_sibling_key() {
    let dir = scratch("roundtrip");
    let path = dir.join(OVERRIDE_FILE);
    std::fs::write(&path, composegen::render_override(&ComposeOpts::default()).unwrap()).unwrap();

    let changed = dml_wow::config::override_env_write(&path, "AC_RATE_XP_KILL", "3").unwrap();
    assert!(changed, "the writer must have applied the new key");

    let after = std::fs::read_to_string(&path).unwrap();
    let doc = parse(&after);
    assert!(
        seq_strings(service(&doc, "ac-worldserver").get("volumes"))
            .contains(&"./modules:/azerothcore/modules".to_string()),
        "the config writer must not drop the modules mount:\n{after}"
    );
    let env = env_map(&after, "ac-worldserver");
    assert_eq!(env.get("AC_SOAP_ENABLED").map(String::as_str), Some("1"));
    assert_eq!(env.get("AC_SOAP_IP").map(String::as_str), Some("0.0.0.0"));
    assert_eq!(env.get("AC_SOAP_PORT").map(String::as_str), Some("7878"));
    assert_eq!(env.get("AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES").map(String::as_str), Some("1"));
    assert_eq!(env.get("AC_RATE_XP_KILL").map(String::as_str), Some("3"));

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// the three-file separation — the Route B tripwire
// ---------------------------------------------------------------------------

#[test]
fn build_config_lives_only_in_the_build_file() {
    let opts = ComposeOpts::default();
    assert!(
        builds_in(&composegen::render_base(&title(), &opts).unwrap()).is_empty(),
        "docker-compose.yml must carry no build: key"
    );
    assert!(
        builds_in(&composegen::render_override(&opts).unwrap()).is_empty(),
        "docker-compose.override.yml must carry no build: key — the config \
         writer owns that file and a post-install `up` would rebuild"
    );
    assert_eq!(
        builds_in(&composegen::render_build(&opts).unwrap()),
        vec![
            "services.ac-authserver.build",
            "services.ac-client-data-init.build",
            "services.ac-db-import.build",
            "services.ac-worldserver.build",
        ]
    );
}

/// The build overlay is STRICTLY build config: it is merged on top of the
/// base with an explicit `-f`, so any runtime key here would be a second,
/// divergent source of truth (and compose CONCATENATES `ports:`/`volumes:`
/// lists rather than replacing them).
#[test]
fn build_file_carries_nothing_but_build_config() {
    let text = composegen::render_build(&ComposeOpts::default()).unwrap();
    let doc = parse(&text);
    let services = doc.get("services").and_then(Value::as_mapping).expect("services map");
    for (svc, body) in services {
        let keys: Vec<&str> = body
            .as_mapping()
            .expect("service body is a map")
            .keys()
            .filter_map(Value::as_str)
            .collect();
        assert_eq!(keys, vec!["build"], "{svc:?} must carry only build:");
    }
    for stage in ["worldserver", "authserver", "db-import", "client-data"] {
        assert!(text.contains(&format!("target: {stage}")), "missing stage {stage}");
    }
    assert!(!text.contains("image:"), "tagging comes from the base file's image: keys");
    assert!(!text.contains("name:"), "the project name comes from the base file");
}

// ---------------------------------------------------------------------------
// writing to a title dir
// ---------------------------------------------------------------------------

#[test]
fn write_all_produces_three_parsable_files_in_the_title_dir() {
    let dir = scratch("writeall").join("wow-server-playerbots");
    let gen = composegen::write_all(&dir, &ComposeOpts::default()).unwrap();

    assert_eq!(gen.base, dir.join(BASE_FILE));
    assert_eq!(gen.overrides, dir.join(OVERRIDE_FILE));
    assert_eq!(gen.build, dir.join(BUILD_FILE));
    assert!(gen.override_written, "a fresh dir must get a generated override");

    for p in [&gen.base, &gen.overrides, &gen.build] {
        let text = std::fs::read_to_string(p).unwrap();
        parse(&text);
        assert!(!text.contains("{{"), "unresolved template token in {}", p.display());
    }
    // `dml-core::compose::resolve_compose_dir` must recognise the result.
    assert_eq!(
        dml_core::compose::resolve_compose_dir(&dir).as_deref(),
        Some(dir.as_path()),
        "the generated dir must be a compose dir to the resolver the CLI uses"
    );

    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

/// Re-running generation (an install RESUME) must never wipe the user's
/// settings: base + build are pure generated artifacts and get rewritten, the
/// override belongs to the config system and is left alone once it exists.
#[test]
fn write_all_rewrites_generated_files_but_never_clobbers_an_existing_override() {
    let dir = scratch("noclobber");
    let mine = "services:\n  ac-worldserver:\n    environment:\n      AC_RATE_XP_KILL: \"7\"\n";
    std::fs::write(dir.join(OVERRIDE_FILE), mine).unwrap();
    std::fs::write(dir.join(BASE_FILE), "# stale\n").unwrap();

    let gen = composegen::write_all(&dir, &ComposeOpts::default()).unwrap();
    assert!(!gen.override_written, "an existing override must be reported as untouched");
    assert_eq!(std::fs::read_to_string(dir.join(OVERRIDE_FILE)).unwrap(), mine);
    assert!(std::fs::read_to_string(dir.join(BASE_FILE)).unwrap().contains("ac-worldserver"));

    // …and the explicit path still exists for the migration/repair case.
    composegen::write_override_force(&dir, &ComposeOpts::default()).unwrap();
    let env = env_map(&std::fs::read_to_string(dir.join(OVERRIDE_FILE)).unwrap(), "ac-worldserver");
    assert_eq!(env.get("AC_SOAP_ENABLED").map(String::as_str), Some("1"));
    assert!(!env.contains_key("AC_RATE_XP_KILL"), "force regenerates from options");

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// the scalars spliced into the BASE template
//
// The override's env map is filtered (`is_env_identifier` + `check_value`);
// the base template's scalars were not, and they land in two splice contexts
// that both break silently:
//
//   MYSQL_ROOT_PASSWORD: ${DB_ROOT_PASSWORD:-<db_password>}
//   AC_LOGIN_DATABASE_INFO: "ac-database;3306;root;<db_password>;acore_auth"
//
// A `#` truncates the first at the comment and leaves `${` unbalanced; a `}`
// terminates Compose's `:-` default early; a `;` silently splits the
// AC_*_DATABASE_INFO field list; a `"` or a newline makes the whole document
// unparsable. Every one of those renders Ok today and gets written to disk,
// so the failure surfaces as a Compose error (or a wrong password) hours
// later. DECISION: refuse, do not escape — no escaping exists that survives
// both contexts plus Compose's own interpolation, and these are install-time
// options a caller can simply re-enter.
// ---------------------------------------------------------------------------

/// The shapes that break a splice context must be REFUSED at render time.
/// Each row is a real corruption of the rendered file, not a style nit.
#[test]
fn base_scalars_refuse_the_shapes_that_break_their_splice_context() {
    let cases: Vec<(&str, ComposeOpts, &str)> = vec![
        (
            "db_password",
            ComposeOpts { db_password: "se cret # not a comment".into(), ..Default::default() },
            "a # truncates the value and leaves ${ unbalanced",
        ),
        (
            "db_password",
            ComposeOpts { db_password: "pa}ss".into(), ..Default::default() },
            "a } ends compose's :- default early",
        ),
        (
            "db_password",
            ComposeOpts { db_password: "a;b".into(), ..Default::default() },
            "a ; is the AC_*_DATABASE_INFO field separator",
        ),
        (
            "db_password",
            ComposeOpts { db_password: "a\"b".into(), ..Default::default() },
            "a quote breaks the double-quoted AC_* connection strings",
        ),
        (
            "db_password",
            ComposeOpts { db_password: "a\nb".into(), ..Default::default() },
            "a newline splices arbitrary YAML",
        ),
        (
            "db_password",
            ComposeOpts { db_password: "$OTHER".into(), ..Default::default() },
            "a $ opens a second compose interpolation",
        ),
        (
            "db_password",
            ComposeOpts { db_password: String::new(), ..Default::default() },
            "an empty password renders ${DB_ROOT_PASSWORD:-}",
        ),
        (
            "project_name",
            ComposeOpts { project_name: Some("Bad Name".into()), ..Default::default() },
            "compose rejects uppercase/spaces in a project name",
        ),
        (
            "project_name",
            ComposeOpts { project_name: Some("a\nservices: {}".into()), ..Default::default() },
            "a newline in the unquoted `name:` scalar splices YAML",
        ),
        (
            "image_prefix",
            ComposeOpts { image_prefix: "acore/ac wotlk-".into(), ..Default::default() },
            "a space in an unquoted image: scalar",
        ),
        (
            "image_tag",
            ComposeOpts { image_tag: Some("master # x".into()), ..Default::default() },
            "a # inside the ${IMAGE_TAG:-…} default",
        ),
        (
            "build_context",
            ComposeOpts { build_context: ". # x".into(), ..Default::default() },
            "a # in the build overlay's context:",
        ),
    ];

    for (field, opts, why) in cases {
        let err = composegen::render_base(&title(), &opts)
            .err()
            .or_else(|| composegen::render_build(&opts).err())
            .unwrap_or_else(|| panic!("{field} must be refused ({why})"));
        assert_eq!(err.code, "BAD_ARG", "{field}: {why} -> {err:?}");
        assert!(
            err.message.contains(field),
            "the error must name the option the caller has to fix; got {:?}",
            err.message
        );
    }
}

/// A refusal is only worth anything if it happens BEFORE anything is written —
/// a half-written title dir is what turns a bad option into a Compose error
/// hours later.
#[test]
fn write_all_refuses_a_hostile_scalar_before_writing_any_file() {
    let dir = scratch("badscalar");
    let opts = ComposeOpts { db_password: "se cret # not a comment".into(), ..Default::default() };
    let err = composegen::write_all(&dir, &opts).unwrap_err();
    assert_eq!(err.code, "BAD_ARG", "{err:?}");
    for f in [BASE_FILE, OVERRIDE_FILE, BUILD_FILE, ".env"] {
        assert!(!dir.join(f).exists(), "{f} must not exist after a refused render");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The other half of the decision: the validator must not be a blanket
/// "reject anything interesting". These are the values Route A, Route B and a
/// normal user actually need, and every one of them must render AND parse.
#[test]
fn realistic_scalars_still_render_and_parse() {
    let opts = ComposeOpts {
        // Route B's open question 6: a published playerbots image under
        // another org. If the validator rejected this, Route B would be
        // designed out by the input filter instead of by the seam.
        image_prefix: "ghcr.io/pjerra/dml-wotlk-".into(),
        image_tag: Some("v0.2.0_rc1".into()),
        project_name: Some("dml-wow-native-2".into()),
        db_password: "Str0ng-P@ss_w0rd!".into(),
        build_context: "./azerothcore".into(),
        ..Default::default()
    };
    let base = composegen::render_base(&title(), &opts).unwrap();
    let doc = parse(&base);
    assert_eq!(doc.get("name").and_then(Value::as_str), Some("dml-wow-native-2"));
    assert!(
        base.contains("ghcr.io/pjerra/dml-wotlk-worldserver:${IMAGE_TAG:-v0.2.0_rc1}"),
        "{base}"
    );
    let env = env_map(&base, "ac-worldserver");
    assert_eq!(
        env.get("AC_LOGIN_DATABASE_INFO").map(String::as_str),
        Some("ac-database;3306;root;${DB_ROOT_PASSWORD:-Str0ng-P@ss_w0rd!};acore_auth"),
        "the password must survive the connection string intact"
    );
    parse(&composegen::render_build(&opts).unwrap());
    parse(&composegen::render_override(&opts).unwrap());
}

// ---------------------------------------------------------------------------
// migration merge
// ---------------------------------------------------------------------------

/// The migration's recorded "biggest lesson": dropping the exported override
/// boots a silently-wrong 500-bot/1x/SOAP-off server. The merge must carry the
/// exported runtime env verbatim — while refusing the base-owned DB wiring,
/// whose stale copy would point the new stack at the old host.
#[test]
fn merge_exported_override_keeps_runtime_env_and_drops_base_owned_wiring() {
    let exported: BTreeMap<String, String> = [
        ("AC_RATE_XP_KILL", "5"),
        ("AC_AI_PLAYERBOT_MAX_RANDOM_BOTS", "500"),
        ("AC_SOAP_ENABLED", "0"),
        ("AC_LOGIN_DATABASE_INFO", "old-host;3306;root;password;acore_auth"),
        ("AC_DATA_DIR", "/somewhere/else"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();

    let opts = composegen::merge_exported_override(&ComposeOpts::default(), &exported).unwrap();
    let env = env_map(&composegen::render_override(&opts).unwrap(), "ac-worldserver");

    assert_eq!(env.get("AC_RATE_XP_KILL").map(String::as_str), Some("5"));
    assert_eq!(env.get("AC_AI_PLAYERBOT_MAX_RANDOM_BOTS").map(String::as_str), Some("500"));
    // The exported value WINS over the generator's default — the server the
    // user is migrating is the source of truth for its own runtime settings.
    assert_eq!(env.get("AC_SOAP_ENABLED").map(String::as_str), Some("0"));
    for owned in ["AC_LOGIN_DATABASE_INFO", "AC_DATA_DIR"] {
        assert!(!env.contains_key(owned), "{owned} is base-owned wiring and must not be merged");
    }
    // The mount survives the merge, and no build config sneaks in with it.
    let text = composegen::render_override(&opts).unwrap();
    assert!(builds_in(&text).is_empty());
    let doc = parse(&text);
    assert!(seq_strings(service(&doc, "ac-worldserver").get("volumes"))
        .contains(&"./modules:/azerothcore/modules".to_string()));
}

/// An exported override is untrusted input (it comes off another machine).
/// A key that is not a plain env identifier would be spliced straight into
/// YAML, so it must be refused rather than rendered.
#[test]
fn merge_exported_override_refuses_a_key_that_is_not_an_env_identifier() {
    let hostile: BTreeMap<String, String> = [(
        "AC_X\n      build:\n        context: .".to_string(),
        "1".to_string(),
    )]
    .into_iter()
    .collect();
    let err = composegen::merge_exported_override(&ComposeOpts::default(), &hostile).unwrap_err();
    assert_eq!(err.code, "BAD_ARG", "{err:?}");
}

// ---------------------------------------------------------------------------
// the Compose oracle
//
// EVERY assertion above this line is a `serde_yaml_ng` parse. That models
// neither Compose's `-f` merge semantics, nor its `${VAR:-default}`
// interpolation, nor whether `build.context`/`target` resolve at all — so a
// trio that Compose itself refuses would pass all of them.
//
// This is the only oracle that answers the question the whole three-file
// split rests on: do the three files actually merge into one project?
//
// It used to be `#[ignore]`d. The documented live gate is
// `cargo test -p dml-wow --tests -- --nocapture`, which does NOT pass
// `--ignored`, so it had never run anywhere. It is no longer ignored: it
// self-skips in the same style as the `find_bash`/`yq_path` gates in the
// parity suites, because `docker compose config` is a CLIENT-side operation —
// verified 2026-07-29 with Docker Desktop's engine down — so it needs the
// docker CLI on the box, not a running server.
// ---------------------------------------------------------------------------

/// The docker CLI, if `docker compose` is usable here. `Err` carries the
/// reason, which goes into the SKIP line verbatim — a skip must always say
/// WHY, or it is indistinguishable from a run (CLAUDE.md's vacuous-pass trap).
fn compose_cli() -> Result<OsString, String> {
    let program = dml_wow::native::docker_program();
    match Command::new(&program).args(["compose", "version"]).output() {
        Err(e) => Err(format!("cannot spawn {program:?}: {e}")),
        Ok(out) if !out.status.success() => Err(format!(
            "{program:?} compose version exited {}: {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Ok(_) => Ok(program),
    }
}

/// Run `docker compose … config` over `files` in `dir`, returning the resolved
/// project YAML or the stderr Compose rejected it with.
fn compose_config(program: &OsStr, dir: &Path, files: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new(program);
    cmd.current_dir(dir).arg("compose");
    for f in files {
        cmd.arg("-f").arg(f);
    }
    cmd.arg("config");
    let out = cmd.output().map_err(|e| format!("spawn failed: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Published-port entries for a service in a RESOLVED compose project, as
/// `(host_ip, published, target)`. Compose expands the short `"a:b"` syntax
/// into a mapping, which is the only place the `${…:-127.0.0.1:7878}` default
/// is proven to parse as an address rather than as nonsense.
fn resolved_ports(doc: &Value, svc: &str) -> Vec<(String, String, u64)> {
    service(doc, svc)
        .get("ports")
        .and_then(Value::as_sequence)
        .map(|s| {
            s.iter()
                .map(|p| {
                    (
                        p.get("host_ip").and_then(Value::as_str).unwrap_or_default().to_string(),
                        p.get("published").and_then(Value::as_str).unwrap_or_default().to_string(),
                        p.get("target").and_then(Value::as_u64).unwrap_or_default(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// THE oracle: Compose itself merges the three generated files and resolves
/// every `${…}` in them.
///
/// Self-skips when the docker CLI is absent (SKIP line names the reason, and
/// `DML_REQUIRE_DOCKER=1` turns the skip into a failure for CI/the release
/// gate). Before trusting a green it proves the oracle DISCRIMINATES, so a
/// `docker` that answered 0 to everything cannot produce a vacuous pass.
#[test]
fn compose_itself_merges_and_resolves_the_generated_trio() {
    let program = match compose_cli() {
        Ok(p) => p,
        Err(why) => {
            assert!(
                std::env::var_os("DML_REQUIRE_DOCKER").is_none(),
                "DML_REQUIRE_DOCKER is set but `docker compose` is unusable: {why}"
            );
            eprintln!(
                "SKIP compose-oracle validation: {why} \
                 (install Docker Desktop, or point DML_DOCKER at the docker CLI)"
            );
            return;
        }
    };

    // --- anti-vacuous control, FIRST -------------------------------------
    // Prove this oracle rejects a bad file before believing it about a good
    // one. The bad file is the exact mistake `native-compose.yml.tmpl`'s own
    // comment warns about: an IP prefix on the SOAP mapping whose default
    // ALREADY carries one, which renders `127.0.0.1:127.0.0.1:7878:7878`.
    let control = scratch("oraclecontrol");
    std::fs::write(
        control.join(BASE_FILE),
        "name: dml-oracle-control\nservices:\n  ac-worldserver:\n    image: x:latest\n    \
         ports:\n      - \"127.0.0.1:${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7878}:7878\"\n",
    )
    .unwrap();
    let control_err = compose_config(&program, &control, &[BASE_FILE]).err().expect(
        "the compose oracle accepted a double-prefixed port mapping — it is not \
         discriminating, so a green result from it proves nothing",
    );
    assert!(
        control_err.contains("invalid IP address"),
        "the control must fail for the reason we think it does, got: {control_err}"
    );
    let _ = std::fs::remove_dir_all(&control);

    // --- the real trio ----------------------------------------------------
    // Expectations are DERIVED from the options and the identity helpers,
    // never restated as literals: this test's job is "the three files merge
    // and every ${…} resolves", not "the defaults are X" (which the tests
    // above own). Pinning literals here would make a deliberate change of a
    // default look like a merge failure.
    let opts = ComposeOpts::default();
    let dir = scratch("livecfg");
    composegen::write_all(&dir, &opts).unwrap();
    let resolved = compose_config(&program, &dir, &[BASE_FILE, OVERRIDE_FILE, BUILD_FILE])
        .unwrap_or_else(|e| panic!("docker compose rejected the generated trio: {e}"));
    let doc = parse(&resolved);

    // The project identity comes from the base file — and Compose ACCEPTING
    // the derived name is itself the proof that the derivation obeys
    // Compose's own project-name grammar, which no unit test can assert.
    assert_eq!(
        doc.get("name").and_then(Value::as_str),
        Some(composegen::project_name_for(&dir).as_str())
    );

    // All THREE files reached the same service — this is the merge itself:
    let ws = service(&doc, "ac-worldserver");
    // …base: the `${IMAGE_TAG:-…}` seam interpolated to a real tag.
    let want_image =
        format!("{}worldserver:{}", opts.image_prefix, composegen::image_tag_for(&dir));
    assert_eq!(
        ws.get("image").and_then(Value::as_str),
        Some(want_image.as_str()),
        "the base file's image seam must resolve"
    );
    // …build overlay: the stage a `-f docker-compose.build.yml` build needs.
    assert_eq!(
        ws.get("build").and_then(|b| b.get("target")).and_then(Value::as_str),
        Some("worldserver"),
        "the build overlay must merge into the same service"
    );
    // …override: the load-bearing modules mount.
    let targets: Vec<&str> = ws
        .get("volumes")
        .and_then(Value::as_sequence)
        .map(|s| s.iter().filter_map(|v| v.get("target").and_then(Value::as_str)).collect())
        .unwrap_or_default();
    assert!(
        targets.contains(&"/azerothcore/modules"),
        "the override's modules mount must survive the merge: {targets:?}"
    );

    // Compose MERGES environment maps across files rather than replacing
    // them: the override's SOAP trio and the base's DB wiring must BOTH be
    // present, with the password interpolated.
    let env = env_map(&resolved, "ac-worldserver");
    assert_eq!(env.get("AC_SOAP_ENABLED").map(String::as_str), Some("1"));
    assert_eq!(
        env.get("AC_LOGIN_DATABASE_INFO").map(String::as_str),
        Some("ac-database;3306;root;password;acore_auth"),
        "the override must not replace the base's environment map"
    );
    assert_eq!(
        env_map(&resolved, "ac-database").get("MYSQL_ROOT_PASSWORD").map(String::as_str),
        Some("password"),
        "${{DB_ROOT_PASSWORD:-…}} must resolve to the generated default"
    );

    // The port posture, as COMPOSE resolves it — the `${…:-127.0.0.1:7878}`
    // default has to parse as host-ip + port, and there must be exactly one
    // entry per port (compose CONCATENATES ports lists across files, so a
    // stray duplicate in the override would show up as a second binding).
    let mut ports = resolved_ports(&doc, "ac-worldserver");
    ports.sort();
    assert_eq!(
        ports,
        vec![
            (String::new(), "8085".to_string(), 8085),
            ("127.0.0.1".to_string(), "7878".to_string(), 7878),
        ],
        "world must be LAN-reachable and SOAP loopback-only, once each"
    );
    assert_eq!(
        resolved_ports(&doc, "ac-database"),
        vec![("127.0.0.1".to_string(), "3306".to_string(), 3306)],
        "the DB must be loopback-only"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// IDENTITY — a new install must never be able to touch an existing server
//
// Every dml call site runs a BARE `docker compose` from the compose dir with
// no `-p` (`dml_core::compose::compose_up_argv`/`compose_down_argv`,
// `logsnap::world_container_argv`), so the generated file's `name:` is the ONLY
// thing that separates two installs — and compose keys CONTAINERS and NAMED
// VOLUMES by it. Two title dirs that render the same project name are one
// stack: the second `up` mounts the first's `<project>_db-data` (its MySQL
// world), a `games stop` in one dir `compose down`s the other's containers, and
// the boot-log snapshot resolves the other's worldserver — the exact
// cross-title bug commit 6c9291b fixed.
//
// This matters TODAY, not hypothetically: plan Task 12's live gate installs
// into a SCRATCH games dir while the working migrated server stays at
// %USERPROFILE%\dml-native\wow-server-playerbots, and the title dir name is
// fixed (`config::TITLE`), so the two dirs differ only in their PARENT.
// ---------------------------------------------------------------------------

fn project_name_at(dir: &Path) -> String {
    let text = std::fs::read_to_string(dir.join(BASE_FILE)).expect("generated base file");
    parse(&text)
        .get("name")
        .and_then(Value::as_str)
        .expect("the base file must pin a compose project name")
        .to_string()
}

fn image_refs_at(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join(BASE_FILE)).expect("generated base file");
    let doc = parse(&text);
    let mut out: Vec<String> = doc
        .get("services")
        .and_then(Value::as_mapping)
        .expect("services map")
        .values()
        .filter_map(|s| s.get("image").and_then(Value::as_str).map(str::to_string))
        .filter(|i| !i.starts_with("mysql:"))
        .collect();
    out.sort();
    out
}

#[test]
fn two_title_dirs_never_share_a_compose_project() {
    let root = scratch("identity");
    // Same title id (it is fixed), different games dirs — the Task 12 shape.
    let a = root.join("games-a").join(dml_wow::config::TITLE);
    let b = root.join("games-b").join(dml_wow::config::TITLE);
    composegen::write_all(&a, &ComposeOpts::default()).unwrap();
    composegen::write_all(&b, &ComposeOpts::default()).unwrap();

    let na = project_name_at(&a);
    let nb = project_name_at(&b);
    assert_ne!(
        na, nb,
        "two title dirs rendered the SAME compose project ({na}) — they would \
         share containers and named volumes"
    );
    // The working migrated server pins exactly this name; generating it again
    // is what would have let a scratch install `down` the user's real world.
    for n in [&na, &nb] {
        assert_ne!(n, "dml-wow-native", "a fresh install must not claim the migrated server's project");
    }

    // …and the identity must be STABLE: an install resume that renamed the
    // project would orphan the volumes the first run created.
    composegen::write_all(&a, &ComposeOpts::default()).unwrap();
    assert_eq!(project_name_at(&a), na, "re-generating the same dir must keep its project name");

    let _ = std::fs::remove_dir_all(&root);
}

/// `docker compose -f docker-compose.yml -f docker-compose.build.yml build`
/// tags every built stage with the BASE file's `image:` value. If that value is
/// `acore/ac-wotlk-worldserver:master`, a Route A build overwrites the exact
/// ref the migrated stack runs (`import-to-desktop.sh`'s `docker load` created
/// it) — the user's server silently starts running a freshly compiled
/// worldserver, and the loaded image survives only as a dangling id.
#[test]
fn a_build_can_never_clobber_the_image_refs_another_stack_already_runs() {
    let root = scratch("imageid");
    let a = root.join("games-a").join(dml_wow::config::TITLE);
    let b = root.join("games-b").join(dml_wow::config::TITLE);
    composegen::write_all(&a, &ComposeOpts::default()).unwrap();
    composegen::write_all(&b, &ComposeOpts::default()).unwrap();

    let ia = image_refs_at(&a);
    assert!(!ia.is_empty(), "the base file must name the acore images");
    for r in &ia {
        assert!(
            !r.starts_with("acore/"),
            "a BUILT image must not be tagged under the upstream org the \
             migrated stack pulls/loads: {r}"
        );
    }
    assert_ne!(
        ia,
        image_refs_at(&b),
        "two installs tag their builds identically — the second build \
         overwrites the first's images"
    );

    // The Route B seam survives the fix: all four acore images keep
    // `${IMAGE_TAG:-…}` so a published image stays a variable swap.
    let text = std::fs::read_to_string(a.join(BASE_FILE)).unwrap();
    assert_eq!(text.matches("${IMAGE_TAG:-").count(), 4, "{text}");

    let _ = std::fs::remove_dir_all(&root);
}

/// Does this line ASSIGN `name`, anywhere on it?
///
/// True for `KEY: v` (YAML map), `KEY=v` (compose list / .env / shell),
/// `export KEY=v`, `ENV KEY=v`, `-e KEY=v`. False for a mere mention, and false
/// for a longer identifier that merely ENDS with the key (`MY_AC_RATE_XP_KILL`),
/// which is why the preceding character is checked.
fn assigns_env_key(line: &str, name: &str) -> bool {
    let bytes = line.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = line[from..].find(name) {
        let at = from + rel;
        let prev_ok = at == 0 || {
            let p = bytes[at - 1];
            !(p.is_ascii_alphanumeric() || p == b'_')
        };
        let rest = line[at + name.len()..].trim_start();
        if prev_ok && (rest.starts_with(':') || rest.starts_with('=')) {
            return true;
        }
        from = at + name.len();
    }
    false
}

/// The installer half of "the shadowing rule".
///
/// `default_override_shadows_exactly_one_documented_registry_row` (above) proves
/// the NATIVE generator never emits an env key that shadows a curated setting.
/// The WSL/Windows title installers write their own
/// `docker-compose.override.yml` from a heredoc/here-string, and for a long time
/// they DID emit bot counts: `AC_AI_PLAYERBOT_MIN/MAX_RANDOM_BOTS`. That cost a
/// real bug — the user changed the world bot population, saved, restarted, and
/// watched the old value come back five times, because the env beat the conf and
/// removing an env key needs the container RECREATED, which the restart button
/// could not do (SHIP-LIST 4.0f).
///
/// Reading the shipped installer text is the only way to pin this: the installers
/// are bash and PowerShell, so no amount of Rust unit testing reaches them. Same
/// mechanism `provision.rs` uses to detect drift against `cli/dev-install.ps1`.
///
/// Three things here are deliberate, and the first two are corrections of a
/// version of this test that LOOKED right and was not (adversarial review,
/// 2026-07-30):
///
/// 1. The forbidden set is the registry rows UNION the companion keys that
///    `config_set_curated` writes and un-shadows by hard-coded special case.
///    `AiPlayerbot.MinRandomBots` is NOT a registry row — it rides along with
///    `bots.population` (`config.rs`) — so a registry-only oracle silently
///    ignored `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS`, the exact key that pins the bot
///    FLOOR. A test named "no bot count env keys" would have passed while an
///    installer re-set the floor to 1600 and the ceiling moved to 800.
/// 2. The installer list is DISCOVERED, not hard-coded, so a new title installer
///    is covered the day it lands rather than when someone remembers this test.
/// 3. Both `KEY: value` (YAML map) and `KEY=value` (compose list / `.env`) count
///    as an assignment. Only the map form was checked before, and only the map
///    form is understood by `_cfg_env_read`/`parse_override_env` — so a list-form
///    key would have slipped the tripwire AND made `config set` report
///    `world-restart` while the env still shadowed.
///
/// A missing or unreadable file FAILS rather than skipping — a tripwire that
/// silently finds nothing to check is not a tripwire.
#[test]
fn installers_carry_no_bot_count_env_keys() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");

    // (2) Discover every shipped title installer under guides/.
    let mut installers: Vec<std::path::PathBuf> = Vec::new();
    let guides = root.join("guides");
    let mut stack = vec![guides.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
            let is_script = name.ends_with(".sh") || name.ends_with(".ps1");
            if is_script && name.contains("install") {
                installers.push(path);
            }
        }
    }
    installers.sort();
    // The discovery itself must be real: an empty or tiny sweep would make every
    // assertion below vacuous.
    assert!(
        installers.len() >= 6,
        "expected to discover the shipped title installers under guides/, found {}: {installers:?}",
        installers.len()
    );
    for known in [
        "install-wow-wotlk.sh",
        "install-wow-wotlk-ubuntu.sh",
        "install-wow-wotlk-fedora.sh",
        "Install-WoW-WotLK.ps1",
    ] {
        assert!(
            installers.iter().any(|p| p.file_name().and_then(|n| n.to_str()) == Some(known)),
            "{known} was not discovered -- has it moved? this tripwire must not silently pass"
        );
    }

    // (1) Every env name that shadows a setting the config editor owns: the
    // curated registry rows, plus the hard-coded companion writes.
    let mut owned: std::collections::BTreeSet<String> = dml_wow::registry::config_registry_rows()
        .iter()
        .filter_map(|r| r.get("env").and_then(|e| e.as_str()))
        .filter(|e| e.starts_with("conf:"))
        .map(|e| dml_wow::config::env_name_for(e.rsplit(':').next().unwrap()))
        .collect();
    for companion in ["AiPlayerbot.MinRandomBots", "AuctionHouseBot.Account"] {
        owned.insert(dml_wow::config::env_name_for(companion));
    }
    // Sanity: name BOTH population keys explicitly. The missing floor key is the
    // blind spot this test already had once; an assert that only pins the ceiling
    // is what let it through.
    for must in ["AC_AI_PLAYERBOT_MAX_RANDOM_BOTS", "AC_AI_PLAYERBOT_MIN_RANDOM_BOTS"] {
        assert!(owned.contains(must), "oracle lost {must}: {owned:?}");
    }

    for path in &installers {
        let rel = path.strip_prefix(&root).unwrap_or(path).display().to_string();
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {rel} ({e}) -- this tripwire must not silently pass"));
        for name in &owned {
            // The autologin exception is documented and shared with the native
            // generator (see composegen.rs, "the shadowing rule").
            if name == "AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN" {
                continue;
            }
            // Only an ASSIGNMENT counts, ANYWHERE on a non-comment line. The
            // explanatory comment in each installer names these keys on purpose
            // and must stay legal, which is what the `#` filter buys.
            //
            // Position-independent deliberately: an earlier version only matched
            // when the TRIMMED line STARTED with the key, which a third reviewer
            // showed is evaded by `export AC_AI_PLAYERBOT_MAX_RANDOM_BOTS=800` --
            // an ordinary shell assignment in a .sh installer, and exactly the
            // bug this tripwire exists to prevent. It also misses `ENV KEY=v` and
            // `-e KEY=v`. Matching anywhere costs nothing and closes the family.
            let assigned = text.lines().map(str::trim).any(|l| {
                if l.starts_with('#') {
                    return false;
                }
                assigns_env_key(l, name)
            });
            assert!(
                !assigned,
                "{rel} sets {name}, which OVERRIDES the matching conf key and makes \
                 the launcher's save of that setting silently revert. Configure it \
                 in the conf file instead (see composegen.rs, 'the shadowing rule')."
            );
        }
    }
}

/// The bug the first REAL native build died on, 2026-07-31.
///
/// The build overlay declared `context:` and `target:` but no `dockerfile:`, so
/// Compose looked for `<checkout>/Dockerfile` — and AzerothCore keeps its
/// Dockerfile at `apps/docker/Dockerfile`. The build failed with "failed to read
/// dockerfile: open Dockerfile: no such file or directory" after cloning 600+ MB
/// and passing five green stages.
///
/// Nothing caught it because every test in this repo drives a FAKE docker that
/// never opens the file: the generated YAML was well-formed, the stage names
/// were right, and the fake happily reported success. That is the shape of this
/// whole class — a generator can only be proven against the thing it generates
/// FOR, and the cheap half of that is asserting the paths it names.
#[test]
fn the_build_overlay_names_the_dockerfile_azerothcore_actually_ships() {
    let dir = scratch("build-dockerfile");
    composegen::write_all(&dir, &ComposeOpts::default()).unwrap();
    let text = std::fs::read_to_string(dir.join(composegen::BUILD_FILE)).unwrap();
    let doc: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    let services = doc["services"].as_mapping().expect("services mapping");

    assert!(!services.is_empty(), "no services rendered — the assertions below would be vacuous");
    for (name, svc) in services {
        let name = name.as_str().unwrap_or("<non-string>");
        let build = &svc["build"];
        assert!(!build.is_null(), "{name} has no build section");
        let dockerfile = build["dockerfile"].as_str().unwrap_or_else(|| {
            panic!("{name} declares no `dockerfile:` — Compose would look for <context>/Dockerfile, which AzerothCore does not have")
        });
        assert_eq!(
            dockerfile, "apps/docker/Dockerfile",
            "{name} points at a Dockerfile path AzerothCore does not ship"
        );
        // A target without a Dockerfile is the same failure one step later.
        assert!(build["target"].as_str().is_some(), "{name} declares no build target");
    }
}
