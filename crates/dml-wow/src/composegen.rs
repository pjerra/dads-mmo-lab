//! Compose/override/build generation for the NATIVE (Docker Desktop on
//! Windows) WoW stack — native-first-install plan, Task 2.
//!
//! This is the one seam Route A (fresh source build), the WSL→native
//! migration and Route B (a published prebuilt worldserver image) all share,
//! so it is deliberately the first thing built and everything else consumes
//! it.
//!
//! # Ground truth
//!
//! The shape below is copied from the WORKING migrated server at
//! `C:/Users/perzi/dml-native/wow-server-playerbots` — it runs, so its layout
//! is evidence rather than a guess — reconciled with the four places in THIS
//! repo that read or write these files:
//!
//! | what | where | why it constrains us |
//! |---|---|---|
//! | `docker exec ac-database mysqldump` | [`crate::backup`] | container names are a contract |
//! | `docker restart -t 300 ac-worldserver` | [`crate::restore`], world-restart | same |
//! | direct MySQL on `127.0.0.1` | [`crate::db::resolve_db_config`] | port/password must agree |
//! | `.services.ac-worldserver.environment` | [`crate::config::override_env_write`] | owns the override file |
//! | `env/dist/etc/**` on the HOST | [`crate::config::conf_path_in`] | the etc bind is non-negotiable |
//!
//! # Why THREE files
//!
//! * `docker-compose.yml` — services, container names, image tags, ports,
//!   binds. Carries the `${IMAGE_TAG:-…}` seam so Route B is a variable
//!   change, not a rewrite.
//! * `docker-compose.override.yml` — ONLY runtime env plus the
//!   `./modules:/azerothcore/modules` mount. Compose auto-loads it, and the
//!   dml config system rewrites it: [`crate::config::override_env_write`]
//!   preserves sibling keys but DROPS COMMENTS and restyles the YAML. That
//!   is exactly why no build config may live here.
//! * `docker-compose.build.yml` — ONLY `build:`/`target:`, never auto-loaded;
//!   the install/build stage passes it with an explicit `-f`. A post-install
//!   `up` therefore can never start a multi-hour rebuild.
//!
//! …plus `.env`, which is not a fourth generated file but a SHARED one: the
//! three YAML files interpolate `${DB_ROOT_PASSWORD:-…}` and
//! `${DOCKER_*_EXTERNAL_PORT:-…}` out of it, [`crate::db::resolve_db_config`]
//! reads it to reach the same MySQL, and `soap-setup` / the CLI's 3306
//! conflict remedy write into it too. [`write_all`] therefore MERGES its own
//! keys into it ([`merge_dotenv`]) rather than generating it — and it must
//! write them at all, or a non-default port or password is real to Compose and
//! invisible to every other reader.
//!
//! # The shadowing rule (the non-obvious one)
//!
//! An `AC_*` env var in the override OVERRIDES the matching key in
//! `worldserver.conf`/`playerbots.conf`. The config system knows this: when
//! the user saves a curated setting it strips the matching env key first and
//! calls it LEGACY ([`crate::config::cfgset_clean_legacy_env`]). So a freshly
//! generated override must NOT carry env keys that shadow curated rows —
//! rates and bot counts are therefore opt-in, not defaults. The single
//! deliberate exception is `AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN`, which BOTH
//! ground-truth sources set (the Linux installer's override and the working
//! migrated server) and without which bots never log in; the tripwire test
//! `default_override_shadows_exactly_one_documented_registry_row` fails if a
//! second one is ever added.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dml_core::error::CmdError;

static BASE_TMPL: &str = include_str!("../data/native-compose.yml.tmpl");
static OVERRIDE_TMPL: &str = include_str!("../data/native-override.yml.tmpl");
static BUILD_TMPL: &str = include_str!("../data/native-build.yml.tmpl");

/// The generated base compose file name. Must stay one of
/// [`dml_core::compose`]'s four recognised candidates or the CLI stops seeing
/// the title as installed.
pub const BASE_FILE: &str = "docker-compose.yml";
/// The config system's file (`config.rs` hardcodes this name too).
pub const OVERRIDE_FILE: &str = "docker-compose.override.yml";
/// The build overlay — never auto-loaded, always passed with `-f`.
pub const BUILD_FILE: &str = "docker-compose.build.yml";
/// The install's `.env`. Compose reads it for `${VAR}` interpolation, and so
/// does [`crate::db::resolve_db_config`] (`<title_dir>/.env`) — which is why
/// [`write_all`] has to leave it on disk rather than merely computing it.
pub const DOTENV_FILE: &str = ".env";

// ---------------------------------------------------------------------------
// INSTALL IDENTITY — derived from the title dir, never a shared constant
//
// A fresh install must not be able to touch an existing server. Two things
// carry that guarantee, and BOTH used to be fixed strings:
//
// 1. The compose PROJECT NAME. Every dml call site runs a bare `docker
//    compose` from the compose dir with no `-p` (`compose_up_argv`/
//    `compose_down_argv`, `logsnap::world_container_argv`), so the file's
//    `name:` is the only separator — and compose keys containers AND NAMED
//    VOLUMES by it. Two title dirs sharing a name are ONE stack: the second
//    `up` mounts the first's `<project>_db-data`, a `games stop` in one dir
//    `down`s the other's containers, and the log snapshot files the other's
//    worldserver log (the cross-title bug commit 6c9291b fixed).
// 2. The IMAGE REFS. `compose -f base -f build.yml build` tags each built
//    stage with the BASE file's `image:` value, so a fixed
//    `acore/ac-wotlk-*:master` makes a Route A build overwrite the very refs
//    the migrated stack runs (`import-to-desktop.sh`'s `docker load` created
//    them) — the user's world silently starts on a freshly compiled
//    worldserver and the loaded image survives only as a dangling id.
//
// Both are therefore derived from the title dir's absolute path, which is what
// `title_dir_for_id`/`resolve_compose_dir` already key everything else on. The
// derivation is STABLE (an install resume must not orphan the volumes the
// first run created) and case/separator-insensitive on Windows.
// ---------------------------------------------------------------------------

/// Image-name prefix for images this machine BUILDS.
///
/// Deliberately NOT the upstream `acore/` org: a build tags whatever the base
/// file's `image:` says, and reusing an upstream ref both clobbers a
/// pulled/loaded image and lets a later `docker compose pull` silently replace
/// this playerbots build with upstream's vanilla worldserver. The `dml.local`
/// first component contains a dot, so Docker reads it as a REGISTRY HOST and
/// can never resolve it to somebody else's Docker Hub repo — a stale-image
/// mistake fails loudly at pull time instead of booting a plausible-looking
/// wrong server.
pub const DEFAULT_IMAGE_PREFIX: &str = "dml.local/ac-wotlk-";

/// The refs a PULLED or `docker load`ed stack runs — upstream's names, the
/// ones the WSL→native export/import pair moves. The migration path sets
/// [`ComposeOpts::image_prefix`]/[`ComposeOpts::image_tag`] to these
/// explicitly; a BUILD must never produce them.
pub const UPSTREAM_IMAGE_PREFIX: &str = "acore/ac-wotlk-";

/// FNV-1a, written out rather than borrowed from `DefaultHasher`: this value
/// names containers and volumes on a user's disk, so it must be identical
/// forever, across machines and Rust releases.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The string the identity hashes. Absolute (two relative spellings of one dir
/// must not become two stacks), forward-slashed, trailing separator stripped,
/// and lowercased on Windows because NTFS is case-insensitive.
fn identity_key(title_dir: &Path) -> String {
    let abs = std::path::absolute(title_dir).unwrap_or_else(|_| title_dir.to_path_buf());
    let mut s = abs.to_string_lossy().replace('\\', "/");
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if cfg!(windows) {
        s = s.to_lowercase();
    }
    s
}

/// Short, stable id for a title dir — the part of the identity that makes two
/// dirs with the SAME name (the title id is fixed, so this is the normal case:
/// a scratch games dir beside the real one) distinct stacks.
pub fn install_id(title_dir: &Path) -> String {
    let h = fnv1a64(identity_key(title_dir).as_bytes());
    format!("{:08x}", ((h >> 32) as u32) ^ (h as u32))
}

/// Lowercase `[a-z0-9_-]` slug of a directory name, so the project name stays
/// readable in `docker ps` instead of being a bare hash.
fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.truncate(32);
    while out.ends_with('-') || out.ends_with('_') {
        out.pop();
    }
    out
}

/// Compose project name for a title dir: `dml-<dir name>-<id>`.
///
/// Always starts with a letter and carries only `[a-z0-9_-]`, so it satisfies
/// compose's own project-name rule (and this module's `project_name`
/// [`ScalarRule`]) whatever the directory is called.
pub fn project_name_for(title_dir: &Path) -> String {
    let base = title_dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut s = slug(&base);
    if s.is_empty() {
        s = "title".to_string();
    }
    format!("dml-{s}-{}", install_id(title_dir))
}

/// Default tag for images BUILT for a title dir. Per-install, so two Route A
/// installs on one engine cannot overwrite each other's build either.
pub fn image_tag_for(title_dir: &Path) -> String {
    format!("native-{}", install_id(title_dir))
}

/// Default published host ports, matching the working server.
pub const DEFAULT_DB_PORT: u16 = 3306;
pub const DEFAULT_AUTH_PORT: u16 = 3724;
pub const DEFAULT_WORLD_PORT: u16 = 8085;
pub const DEFAULT_SOAP_PORT: u16 = 7878;

/// Env keys the BASE file owns (DB wiring and the dist paths). A migrated
/// override may carry stale copies pointing at the old host, so the merge
/// drops them rather than letting them shadow the generated wiring.
const BASE_OWNED_ENV: &[&str] = &[
    "AC_DATA_DIR",
    "AC_LOGS_DIR",
    "AC_TEMP_DIR",
    "AC_LOGIN_DATABASE_INFO",
    "AC_WORLD_DATABASE_INFO",
    "AC_CHARACTER_DATABASE_INFO",
    "AC_PLAYERBOTS_DATABASE_INFO",
];

/// Host ports the stack publishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ports {
    pub db: u16,
    pub auth: u16,
    pub world: u16,
    pub soap: u16,
}

impl Default for Ports {
    fn default() -> Self {
        Ports {
            db: DEFAULT_DB_PORT,
            auth: DEFAULT_AUTH_PORT,
            world: DEFAULT_WORLD_PORT,
            soap: DEFAULT_SOAP_PORT,
        }
    }
}

/// The three rate multipliers the old servers pinned via env. Opt-in only —
/// see the shadowing rule in the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rates {
    pub xp_kill: String,
    pub xp_quest: String,
    pub drop_money: String,
}

/// Everything the three files are rendered from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeOpts {
    /// Compose project name (`name:`), pinned so `ps`/`down`/log-snapshot all
    /// scope to this stack.
    ///
    /// `None` — the default — DERIVES it from the title dir
    /// ([`project_name_for`]). A fixed value here would make two title dirs one
    /// compose project, sharing containers and named volumes; `Some(..)` is the
    /// deliberate escape hatch for ADOPTING an existing stack (a migration
    /// pointed at a dir whose volumes were created under another name), and the
    /// caller then owns the collision risk.
    pub project_name: Option<String>,
    /// Image name prefix. Route B's escape hatch: a published playerbots
    /// worldserver under another org only changes this and the tag. Defaults to
    /// [`DEFAULT_IMAGE_PREFIX`] — never upstream's, so a build cannot overwrite
    /// a pulled or `docker load`ed image.
    pub image_prefix: String,
    /// The DEFAULT inside the `${IMAGE_TAG:-…}` seam — never a hardcoded tag.
    ///
    /// `None` derives a per-install tag ([`image_tag_for`]) so a build is
    /// namespaced to its own title dir. The migration path sets `Some("master")`
    /// (plus [`UPSTREAM_IMAGE_PREFIX`]) because `docker load` decides those
    /// names, not us.
    pub image_tag: Option<String>,
    /// Compose interpolation default for `DB_ROOT_PASSWORD`; must stay in
    /// step with [`crate::db::resolve_db_config`].
    pub db_password: String,
    pub ports: Ports,
    /// SOAP on by default. Off ships a launcher whose GM tools, My Party and
    /// console are all dead, so this is opt-OUT, never opt-in.
    pub soap: bool,
    /// `(min, max)` random bots. Opt-in: shadows a curated setting.
    pub bots: Option<(u32, u32)>,
    /// Rate multipliers. Opt-in: shadows curated settings.
    pub rates: Option<Rates>,
    /// Extra worldserver env, applied LAST so it wins. Populated by
    /// [`merge_exported_override`] on the migration path.
    pub extra_env: Vec<(String, String)>,
    /// Build context for the build overlay; `.` because the title dir IS the
    /// AzerothCore checkout (plan decision 2).
    pub build_context: String,
}

impl Default for ComposeOpts {
    fn default() -> Self {
        ComposeOpts {
            project_name: None,
            image_prefix: DEFAULT_IMAGE_PREFIX.to_string(),
            image_tag: None,
            db_password: "password".to_string(),
            ports: Ports::default(),
            soap: true,
            bots: None,
            rates: None,
            extra_env: Vec::new(),
            build_context: ".".to_string(),
        }
    }
}

/// Paths written by [`write_all`], plus whether the override was actually
/// (re)generated — an existing one is left alone, because it belongs to the
/// config system and carries the user's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub base: PathBuf,
    pub overrides: PathBuf,
    pub build: PathBuf,
    pub override_written: bool,
    /// The override existed and was REPLACED — only reachable through
    /// [`write_all_with`]`(.., true)`, i.e. an install that has not started its
    /// containers yet. Distinguished from `override_written` so the caller can
    /// say "refreshed" rather than "wrote", which are different events to a
    /// user watching a resume.
    pub override_replaced: bool,
    /// `<title_dir>/.env`, present iff this call wrote settings into it.
    /// `None` means the options needed no `.env` — never "the settings were
    /// applied somewhere else".
    pub dotenv: Option<PathBuf>,
}

fn gen_err(code: &str, message: impl Into<String>, hint: impl Into<String>) -> CmdError {
    CmdError { code: code.to_string(), message: message.into(), hint: hint.into() }
}

/// `{{TOKEN}}` substitution. Any token left unresolved is an ERROR, not a
/// silently broken YAML file — a template edit that adds a placeholder
/// nobody fills fails loudly at the first render.
fn fill(tmpl: &str, vars: &[(&str, String)]) -> Result<String, CmdError> {
    let mut out = tmpl.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    if let Some(idx) = out.find("{{") {
        let tail: String = out[idx..].chars().take(40).collect();
        return Err(gen_err(
            "COMPOSE_TEMPLATE",
            format!("unresolved template placeholder near: {tail}"),
            "A data/native-*.yml.tmpl placeholder has no value — fix composegen.rs.",
        ));
    }
    Ok(out)
}

/// A double-quoted YAML scalar, matching the style
/// [`crate::config::override_env_write`] emits for every value it writes
/// (always a string, never an int/bool).
fn yaml_quoted(v: &str) -> String {
    let mut out = String::with_capacity(v.len() + 2);
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// An env var name we are willing to splice into YAML. Exported overrides
/// come off ANOTHER machine, so this is a real input filter, not a formality.
pub fn is_env_identifier(k: &str) -> bool {
    let mut chars = k.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// One rule for one scalar spliced into a template.
///
/// # Why an ALLOWLIST, and why REFUSE rather than escape
///
/// Unlike the override's env map — whose values land in exactly one context,
/// a double-quoted YAML scalar this module writes itself — the base template's
/// scalars land in TWO contexts at once, and Compose interpolates on top of
/// both:
///
/// ```text
/// MYSQL_ROOT_PASSWORD: ${DB_ROOT_PASSWORD:-<db_password>}      # plain scalar
/// AC_LOGIN_DATABASE_INFO: "ac-database;3306;root;<db_password>;acore_auth"
/// ```
///
/// `#` truncates the first at the YAML comment and leaves `${` unbalanced;
/// `}` ends Compose's `:-` default early; `;` splits the AC_*_DATABASE_INFO
/// field list; `"`/`\` break the quoted string; `$` opens another
/// interpolation; a newline splices arbitrary YAML. No single escaping
/// survives all of them (YAML escaping is undone before Compose's
/// interpolation ever sees the text), so a bad value is REFUSED — these are
/// install-time options, and a caller can pick another password.
struct ScalarRule {
    /// Field name as the caller knows it — it must appear in the error so the
    /// user is told what to change.
    field: &'static str,
    /// Human description of the accepted shape, quoted back in the error.
    allowed: &'static str,
    first: fn(char) -> bool,
    rest: fn(char) -> bool,
}

fn is_lower_alnum(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

/// Compose's own project-name rule: lowercase alphanumerics, `-` and `_`,
/// starting with an alphanumeric. Compose refuses anything else outright.
fn project_name_rest(c: char) -> bool {
    is_lower_alnum(c) || c == '-' || c == '_'
}

/// Docker image-reference characters (registry host, path, separators). No
/// `:` — the tag is appended by the template, never carried in the prefix.
fn image_prefix_rest(c: char) -> bool {
    is_lower_alnum(c) || matches!(c, '.' | '_' | '-' | '/')
}

/// Docker's tag grammar: `[A-Za-z0-9_][A-Za-z0-9._-]*`.
fn image_tag_first(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
fn image_tag_rest(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// A password charset wide enough for a real password and narrow enough that
/// none of the four splice/interpolation contexts can be escaped from.
/// Deliberately excludes `# $ { } " \ ' : ; , < > | & ( ) [ ]` and every
/// space/control character.
fn password_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+' | '=' | '~' | '@' | '!' | '?' | '*' | '^' | '%')
}

/// Relative build context (`.`, `./azerothcore`). Backslashes and drive
/// letters are refused: the context is resolved relative to the compose dir,
/// and a Windows-style path here does not survive Compose's own resolution.
fn build_context_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/')
}

const SCALAR_RULES: &[ScalarRule] = &[
    ScalarRule {
        field: "project_name",
        allowed: "lowercase letters, digits, '-' and '_', starting with a letter or digit",
        first: is_lower_alnum,
        rest: project_name_rest,
    },
    ScalarRule {
        field: "image_prefix",
        allowed: "lowercase letters, digits and '. _ - /', starting with a letter or digit",
        first: is_lower_alnum,
        rest: image_prefix_rest,
    },
    ScalarRule {
        field: "image_tag",
        allowed: "letters, digits and '. _ -', starting with a letter, digit or '_'",
        first: image_tag_first,
        rest: image_tag_rest,
    },
    ScalarRule {
        field: "db_password",
        allowed: "letters, digits and '- _ . + = ~ @ ! ? * ^ %'",
        first: password_char,
        rest: password_char,
    },
    ScalarRule {
        field: "build_context",
        allowed: "a relative path of letters, digits and '. _ - /'",
        first: build_context_char,
        rest: build_context_char,
    },
];

fn rule(field: &str) -> &'static ScalarRule {
    SCALAR_RULES
        .iter()
        .find(|r| r.field == field)
        .expect("every validated field has a rule")
}

fn check_scalar(field: &str, value: &str) -> Result<(), CmdError> {
    let r = rule(field);
    let bad = |detail: String| {
        gen_err(
            "BAD_ARG",
            format!("{field}: {detail}"),
            format!("Allowed in {field}: {}.", r.allowed),
        )
    };
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err(bad("must not be empty".to_string()));
    };
    if !(r.first)(first) {
        return Err(bad(format!("must not start with {first:?}")));
    }
    if let Some(c) = chars.find(|c| !(r.rest)(*c)) {
        return Err(bad(format!("must not contain {c:?}")));
    }
    Ok(())
}

/// Validate every scalar this module splices into a template.
///
/// Called by all three `render_*` entry points (and therefore by
/// [`write_all`]/[`write_override_force`]) so a bad option can never reach
/// disk: a half-written title dir turns a typo into a Compose error hours
/// later, when the user is two stages into an install.
/// The derived identity is checked by [`render_base`] against the SAME rules,
/// so nothing reaches a template unvalidated whether the caller supplied it or
/// the title dir did.
pub fn validate(opts: &ComposeOpts) -> Result<(), CmdError> {
    if let Some(p) = &opts.project_name {
        check_scalar("project_name", p)?;
    }
    if let Some(t) = &opts.image_tag {
        check_scalar("image_tag", t)?;
    }
    check_scalar("image_prefix", &opts.image_prefix)?;
    check_scalar("db_password", &opts.db_password)?;
    check_scalar("build_context", &opts.build_context)?;
    Ok(())
}

fn check_value(k: &str, v: &str) -> Result<(), CmdError> {
    if v.chars().any(|c| c.is_control()) {
        return Err(gen_err(
            "BAD_ARG",
            format!("value for {k} contains a control character"),
            "Compose env values must be single-line text.",
        ));
    }
    Ok(())
}

fn push_env(env: &mut Vec<(String, String)>, k: &str, v: impl Into<String>) {
    let v = v.into();
    if let Some(slot) = env.iter_mut().find(|(ek, _)| ek == k) {
        slot.1 = v;
    } else {
        env.push((k.to_string(), v));
    }
}

/// The ordered `ac-worldserver` environment the override will carry.
///
/// Order is deliberate (DB-updater switch, bot autologin, SOAP, then the
/// opt-ins, then migration extras last so an imported server's own settings
/// win) but NOT contractual — the config writer restyles the file anyway.
pub fn override_env(opts: &ComposeOpts) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = Vec::new();
    // Without this the playerbots schema is never created/updated.
    push_env(&mut env, "AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES", "1");
    // The documented shadowing exception — see the module docs.
    push_env(&mut env, "AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN", "1");
    if opts.soap {
        push_env(&mut env, "AC_SOAP_ENABLED", "1");
        // Inside the container SOAP must listen on all interfaces; the HOST
        // side is what is pinned to loopback (see the base file's ports).
        push_env(&mut env, "AC_SOAP_IP", "0.0.0.0");
        // CONTAINER-side port, deliberately NOT `opts.ports.soap`: the base
        // file always maps `<host>:7878`, so binding the server to the host
        // port would leave the mapping pointing at a closed port. `ports.soap`
        // moves the HOST side only (and a non-default host port then needs
        // `DML_SOAP_URL` for `soap.rs`, whose default is 127.0.0.1:7878).
        push_env(&mut env, "AC_SOAP_PORT", DEFAULT_SOAP_PORT.to_string());
    }
    if let Some((min, max)) = opts.bots {
        push_env(&mut env, "AC_AI_PLAYERBOT_MIN_RANDOM_BOTS", min.to_string());
        push_env(&mut env, "AC_AI_PLAYERBOT_MAX_RANDOM_BOTS", max.to_string());
    }
    if let Some(r) = &opts.rates {
        push_env(&mut env, "AC_RATE_XP_KILL", r.xp_kill.clone());
        push_env(&mut env, "AC_RATE_XP_QUEST", r.xp_quest.clone());
        push_env(&mut env, "AC_RATE_DROP_MONEY", r.drop_money.clone());
    }
    for (k, v) in &opts.extra_env {
        push_env(&mut env, k, v.clone());
    }
    env
}

/// Render `docker-compose.yml` for the title dir it will be written into.
///
/// The dir is a PARAMETER rather than an option because the file's identity —
/// its compose project name and the tag its build products carry — has to come
/// from the dir; there is no safe default for either, and a shared one is how a
/// new install ends up mounting an existing server's volumes.
/// Every `container_name` the generated base declares, in file order.
///
/// Derived from the template rather than restated as a constant, and the
/// distinction is the point: these are the exact strings `docker compose up`
/// prints back ("Container ac-worldserver Started"), so a service added to the
/// template is counted without anyone remembering to bump a number. A hardcoded
/// total would drift silently and report 5/4 containers, or stall at 80%.
pub fn base_container_names() -> Vec<String> {
    BASE_TMPL
        .lines()
        .filter_map(|l| l.trim().strip_prefix("container_name:"))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

pub fn render_base(title_dir: &Path, opts: &ComposeOpts) -> Result<String, CmdError> {
    validate(opts)?;
    let project = opts.project_name.clone().unwrap_or_else(|| project_name_for(title_dir));
    let tag = opts.image_tag.clone().unwrap_or_else(|| image_tag_for(title_dir));
    // A DERIVED value clears the same bar an explicit one does: a directory
    // named something Compose refuses must fail here, not at `up`.
    check_scalar("project_name", &project)?;
    check_scalar("image_tag", &tag)?;
    fill(
        BASE_TMPL,
        &[
            ("PROJECT_NAME", project),
            ("IMAGE_PREFIX", opts.image_prefix.clone()),
            ("IMAGE_TAG", tag),
            ("DB_PASSWORD", opts.db_password.clone()),
            ("DB_PORT", opts.ports.db.to_string()),
            ("AUTH_PORT", opts.ports.auth.to_string()),
            ("WORLD_PORT", opts.ports.world.to_string()),
            ("SOAP_PORT", opts.ports.soap.to_string()),
        ],
    )
}

/// Render `docker-compose.override.yml`.
pub fn render_override(opts: &ComposeOpts) -> Result<String, CmdError> {
    validate(opts)?;
    let env = override_env(opts);
    let mut lines = String::new();
    if env.is_empty() {
        // A `{}` keeps the file valid YAML; the config writer can still add
        // keys to it later.
        lines.push_str("      {}");
    }
    for (k, v) in &env {
        if !is_env_identifier(k) {
            return Err(gen_err(
                "BAD_ARG",
                format!("not a valid environment variable name: {k}"),
                "Environment names must match [A-Za-z_][A-Za-z0-9_]*.",
            ));
        }
        check_value(k, v)?;
        if !lines.is_empty() {
            lines.push('\n');
        }
        lines.push_str(&format!("      {k}: {}", yaml_quoted(v)));
    }
    fill(OVERRIDE_TMPL, &[("ENVIRONMENT", lines)])
}

/// Render `docker-compose.build.yml`.
pub fn render_build(opts: &ComposeOpts) -> Result<String, CmdError> {
    validate(opts)?;
    fill(BUILD_TMPL, &[("BUILD_CONTEXT", opts.build_context.clone())])
}

/// The `.env` lines needed to make NON-DEFAULT options real — Compose reads
/// this file for `${VAR}` interpolation and so does
/// [`crate::db::resolve_db_config`].
///
/// Deliberately EMPTY for default options. Writing `DOCKER_DB_EXTERNAL_PORT`
/// unconditionally would disable the CLI's 3306 auto-remap, which stays
/// silent whenever that key is already present
/// ([`crate::lifecycle::env_has_db_external_port`] →
/// `db_port_conflict_message`).
pub fn dotenv_lines(opts: &ComposeOpts) -> Vec<String> {
    let mut out = Vec::new();
    if opts.ports.db != DEFAULT_DB_PORT {
        out.push(format!("DOCKER_DB_EXTERNAL_PORT={}", opts.ports.db));
    }
    if opts.ports.auth != DEFAULT_AUTH_PORT {
        out.push(format!("DOCKER_AUTH_EXTERNAL_PORT={}", opts.ports.auth));
    }
    if opts.ports.world != DEFAULT_WORLD_PORT {
        out.push(format!("DOCKER_WORLD_EXTERNAL_PORT={}", opts.ports.world));
    }
    if opts.ports.soap != DEFAULT_SOAP_PORT {
        // Same shape `dml wow soap-setup` writes: the whole host binding.
        out.push(format!("DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:{}", opts.ports.soap));
    }
    if opts.db_password != ComposeOpts::default().db_password {
        out.push(format!("DB_ROOT_PASSWORD={}", opts.db_password));
    }
    out
}

/// Fold generated `KEY=VALUE` lines into an existing `.env` text.
///
/// MERGE, never overwrite: this file is shared property. `dml wow soap-setup`
/// writes `DOCKER_SOAP_EXTERNAL_PORT` into it and the CLI's 3306 remedy appends
/// `DOCKER_DB_EXTERNAL_PORT` when the port is busy, so an install RESUME that
/// rewrote the file would silently undo either. A key the generator emits
/// replaces its FIRST existing assignment — first is what both readers take
/// ([`crate::db::dotenv_get`] and Compose) — and everything else, comments
/// included, is left byte-for-byte.
pub fn merge_dotenv(existing: &str, lines: &[String]) -> String {
    let mut out: Vec<String> = existing.lines().map(str::to_string).collect();
    for line in lines {
        let Some((key, _)) = line.split_once('=') else { continue };
        let hit = out
            .iter()
            .position(|l| l.split_once('=').map(|(k, _)| k.trim() == key).unwrap_or(false));
        match hit {
            Some(i) => out[i] = line.clone(),
            None => out.push(line.clone()),
        }
    }
    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

/// Fold an EXPORTED override's worldserver environment into `base` for the
/// migration path.
///
/// The migration's recorded "biggest lesson" is that dropping the exported
/// override boots a silently-wrong 500-bot / 1x / SOAP-off server, so the
/// exported keys are carried verbatim and WIN over the generator's defaults.
/// Two filters apply: keys the base file owns ([`BASE_OWNED_ENV`] — stale DB
/// wiring pointing at the old host) are dropped, and anything that is not a
/// plain env identifier is refused outright rather than spliced into YAML.
pub fn merge_exported_override(
    base: &ComposeOpts,
    exported: &BTreeMap<String, String>,
) -> Result<ComposeOpts, CmdError> {
    let mut opts = base.clone();
    for (k, v) in exported {
        if !is_env_identifier(k) {
            return Err(gen_err(
                "BAD_ARG",
                format!("exported override carries an invalid environment name: {k:?}"),
                "Re-export from the source server; names must match [A-Za-z_][A-Za-z0-9_]*.",
            ));
        }
        check_value(k, v)?;
        if BASE_OWNED_ENV.contains(&k.as_str()) {
            continue;
        }
        push_env(&mut opts.extra_env, k, v.clone());
    }
    Ok(opts)
}

fn write_file(path: &Path, text: &str) -> Result<(), CmdError> {
    std::fs::write(path, text).map_err(|e| {
        gen_err("WRITE_FAILED", format!("Could not write {}: {e}", path.display()), "")
    })
}

/// Write the generated files into `title_dir`, creating it if needed.
///
/// `docker-compose.yml` and `docker-compose.build.yml` are pure generated
/// artifacts and are always rewritten (so a resumed install repairs them).
/// `docker-compose.override.yml` is only written when ABSENT: it belongs to
/// the config system and carries the user's settings, so regenerating it on a
/// resume would silently discard them. Use [`write_all_with`] when the caller
/// can prove the override is not yet in service (the install engine can, before
/// its `up` stage), or [`write_override_force`] to replace it outright.
///
/// `.env` is written (merged) whenever the options need one. It is not
/// optional bookkeeping: Compose interpolates the base file's
/// `${DB_ROOT_PASSWORD:-…}` / `${DOCKER_*_EXTERNAL_PORT:-…}` from it, and it is
/// the ONLY place [`crate::db::resolve_db_config`] can learn a non-default port
/// or password. Without it a `db_password` option would be real to MySQL and
/// invisible to every reader — every native DB page dead with `DB_UNREACHABLE`
/// and every `mysqldump` in [`crate::backup`]/[`crate::restore`]/
/// [`crate::lifecycle`] failing auth.
pub fn write_all(title_dir: &Path, opts: &ComposeOpts) -> Result<Generated, CmdError> {
    write_all_with(title_dir, opts, false)
}

/// [`write_all`], with the one decision it cannot make for itself: may an
/// EXISTING `docker-compose.override.yml` be regenerated?
///
/// ## Why this switch exists
///
/// Every other generated file is rewritten on every call, so a fixed template
/// reaches an in-progress install on the next run. The override was the
/// exception, and that exception was a silent hole: a template fix could never
/// reach any directory that already had one — not on a resume, not after a DML
/// update. The bug that broke the first live install was exactly a template
/// omission, so "our generator was wrong and the fix cannot reach you" is a real
/// failure mode, not a hypothetical one.
///
/// The exception was not arbitrary, though, and must not simply be deleted. The
/// override is where [`crate::config`] writes the user's settings — bot counts,
/// rates, SOAP — so rewriting one that is in service destroys them.
///
/// Both facts hold, so the caller has to say which situation it is in. The
/// engine's rule is `up`: the override first takes effect when the containers
/// start, so before that stage is recorded no setting can have been applied
/// through it and regenerating is provably lossless. Afterwards the file is live
/// configuration and is left alone. Not "before ready" — a stack that reached
/// `up` and then timed out waiting for the world server is running, reachable
/// from Settings, and a resume must not eat what the user changed.
pub fn write_all_with(
    title_dir: &Path,
    opts: &ComposeOpts,
    replace_override: bool,
) -> Result<Generated, CmdError> {
    // Before the filesystem is touched at all: a refused option must not even
    // leave a title dir behind.
    validate(opts)?;
    std::fs::create_dir_all(title_dir).map_err(|e| {
        gen_err("WRITE_FAILED", format!("Could not create {}: {e}", title_dir.display()), "")
    })?;

    let base = title_dir.join(BASE_FILE);
    let overrides = title_dir.join(OVERRIDE_FILE);
    let build = title_dir.join(BUILD_FILE);

    write_file(&base, &render_base(title_dir, opts)?)?;
    write_file(&build, &render_build(opts)?)?;

    let existed = overrides.exists();
    let override_replaced = existed && replace_override;
    let override_written = !existed || override_replaced;
    if override_written {
        write_file(&overrides, &render_override(opts)?)?;
    }

    let lines = dotenv_lines(opts);
    let dotenv = if lines.is_empty() {
        // Default options pin nothing — and MUST NOT: an unconditional
        // `DOCKER_DB_EXTERNAL_PORT` would permanently silence the CLI's 3306
        // conflict remedy (`lifecycle::env_has_db_external_port`).
        None
    } else {
        let path = title_dir.join(DOTENV_FILE);
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        write_file(&path, &merge_dotenv(&existing, &lines))?;
        Some(path)
    };

    Ok(Generated { base, overrides, build, override_written, override_replaced, dotenv })
}

/// Replace `docker-compose.override.yml` from `opts`, discarding whatever was
/// there. For the migration path (where `opts` already carries the exported
/// env) and explicit repair — never for a plain install resume.
pub fn write_override_force(title_dir: &Path, opts: &ComposeOpts) -> Result<PathBuf, CmdError> {
    validate(opts)?;
    std::fs::create_dir_all(title_dir).map_err(|e| {
        gen_err("WRITE_FAILED", format!("Could not create {}: {e}", title_dir.display()), "")
    })?;
    let path = title_dir.join(OVERRIDE_FILE);
    write_file(&path, &render_override(opts)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_every_occurrence_of_a_token() {
        let out = fill("a={{X}} b={{X}} c={{Y}}", &[("X", "1".into()), ("Y", "2".into())]).unwrap();
        assert_eq!(out, "a=1 b=1 c=2");
    }

    #[test]
    fn fill_refuses_to_emit_a_file_with_an_unresolved_placeholder() {
        let err = fill("x={{MISSING}}", &[("X", "1".into())]).unwrap_err();
        assert_eq!(err.code, "COMPOSE_TEMPLATE");
        assert!(err.message.contains("MISSING"), "{}", err.message);
    }

    #[test]
    fn yaml_quoted_escapes_quotes_and_backslashes() {
        assert_eq!(yaml_quoted("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn env_identifier_rejects_yaml_injection_shapes() {
        assert!(is_env_identifier("AC_SOAP_ENABLED"));
        assert!(is_env_identifier("_X9"));
        assert!(!is_env_identifier(""));
        assert!(!is_env_identifier("9AC"));
        assert!(!is_env_identifier("AC-SOAP"));
        assert!(!is_env_identifier("AC_X\n      build"));
        assert!(!is_env_identifier("AC_X: y"));
    }

    /// The defaults are the one set of values that MUST pass — a validator
    /// that refuses its own defaults is a validator nobody would notice was
    /// wrong until an install failed.
    #[test]
    fn the_default_options_validate() {
        validate(&ComposeOpts::default()).unwrap();
    }

    /// Each rejected character corresponds to a concrete corruption of the
    /// rendered file; the error must name the field so the caller knows which
    /// option to change.
    #[test]
    fn check_scalar_names_the_field_and_the_offending_character() {
        let err = check_scalar("db_password", "se cret # x").unwrap_err();
        assert_eq!(err.code, "BAD_ARG");
        assert!(err.message.starts_with("db_password:"), "{}", err.message);
        assert!(err.message.contains("' '"), "the space must be named: {}", err.message);
        assert!(err.hint.contains("Allowed in db_password"), "{}", err.hint);

        let err = check_scalar("project_name", "").unwrap_err();
        assert!(err.message.contains("must not be empty"), "{}", err.message);

        let err = check_scalar("image_tag", "-lead").unwrap_err();
        assert!(err.message.contains("must not start with"), "{}", err.message);
    }

    /// Table of the shapes that break a splice context, kept next to the rule
    /// they exercise. Anything added to an allowlist has to answer to this.
    #[test]
    fn every_scalar_rule_refuses_its_own_splice_breakers() {
        for (field, bad) in [
            ("db_password", "a#b"),
            ("db_password", "a}b"),
            ("db_password", "a{b"),
            ("db_password", "a$b"),
            ("db_password", "a;b"),
            ("db_password", "a\"b"),
            ("db_password", "a'b"),
            ("db_password", "a\\b"),
            ("db_password", "a:b"),
            ("db_password", "a b"),
            ("db_password", "a\nb"),
            ("db_password", "a\tb"),
            ("project_name", "Upper"),
            ("project_name", "a b"),
            ("project_name", "a\nservices: {}"),
            ("image_prefix", "acore/ac wotlk-"),
            ("image_prefix", "acore/${X}-"),
            ("image_prefix", "registry:5000/acore-"),
            ("image_tag", "master # x"),
            ("image_tag", "a b"),
            ("build_context", ". # x"),
            ("build_context", "C:\\src"),
        ] {
            let err = check_scalar(field, bad)
                .err()
                .unwrap_or_else(|| panic!("{field} must refuse {bad:?}"));
            assert_eq!(err.code, "BAD_ARG", "{field} / {bad:?}");
        }
    }

    /// …and accepts the values Route A, Route B and a normal user need.
    #[test]
    fn every_scalar_rule_accepts_the_values_the_product_needs() {
        for (field, good) in [
            ("project_name", "dml-wow-native"),
            ("project_name", "wow-server-playerbots"),
            ("image_prefix", "acore/ac-wotlk-"),
            ("image_prefix", "ghcr.io/pjerra/dml-wotlk-"),
            ("image_tag", "master"),
            ("image_tag", "v0.2.0_rc1"),
            ("db_password", "password"),
            ("db_password", "Str0ng-P@ss_w0rd!"),
            ("build_context", "."),
            ("build_context", "./azerothcore"),
        ] {
            check_scalar(field, good).unwrap_or_else(|e| panic!("{field}/{good:?}: {e:?}"));
        }
    }

    #[test]
    fn control_characters_in_a_value_are_refused() {
        let err = check_value("AC_X", "a\nb").unwrap_err();
        assert_eq!(err.code, "BAD_ARG");
        assert!(check_value("AC_X", "0.0.0.0").is_ok());
    }

    #[test]
    fn push_env_replaces_in_place_so_later_sources_win_without_duplicating() {
        let mut env = vec![("A".to_string(), "1".to_string()), ("B".to_string(), "2".to_string())];
        push_env(&mut env, "A", "9");
        assert_eq!(env, vec![("A".into(), "9".into()), ("B".into(), "2".into())]);
    }

    #[test]
    fn default_override_env_is_the_soap_trio_plus_the_two_playerbot_switches() {
        let keys: Vec<String> =
            override_env(&ComposeOpts::default()).into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            vec![
                "AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES",
                "AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN",
                "AC_SOAP_ENABLED",
                "AC_SOAP_IP",
                "AC_SOAP_PORT",
            ]
        );
    }

    /// `ports.soap` moves the HOST binding only. The container always listens
    /// on 7878 because the base file's mapping always ends in `:7878` —
    /// tying `AC_SOAP_PORT` to the host port would publish a closed port and
    /// kill every SOAP feature with no visible cause.
    #[test]
    fn a_non_default_host_soap_port_does_not_move_the_container_side_port() {
        let opts = ComposeOpts {
            ports: Ports { soap: 7879, ..Ports::default() },
            ..Default::default()
        };
        let env = override_env(&opts);
        assert!(env.contains(&("AC_SOAP_PORT".to_string(), "7878".to_string())), "{env:?}");
        let base = render_base(Path::new("/games/wow-server-playerbots"), &opts).unwrap();
        assert!(base.contains("${DOCKER_SOAP_EXTERNAL_PORT:-127.0.0.1:7879}:7878"), "{base}");
    }

    // -- install identity ---------------------------------------------------

    /// The property the whole derivation exists for: a scratch install beside
    /// the real one is two stacks. The title id is FIXED
    /// (`config::TITLE`), so the dirs differ only in their parent — a
    /// derivation that used the dir NAME alone would still collide.
    #[test]
    fn identity_differs_for_two_games_dirs_holding_the_same_title() {
        let a = Path::new("/games-a/wow-server-playerbots");
        let b = Path::new("/games-b/wow-server-playerbots");
        assert_ne!(project_name_for(a), project_name_for(b));
        assert_ne!(image_tag_for(a), image_tag_for(b));
        // …while staying readable rather than a bare hash.
        assert!(project_name_for(a).starts_with("dml-wow-server-playerbots-"), "{}", project_name_for(a));
    }

    /// Containers and named volumes are keyed by these strings on a user's
    /// disk: an install RESUME that renamed the project would orphan the
    /// volumes the first run created. Same dir, spelled differently, is the
    /// same install.
    #[test]
    fn identity_is_stable_across_spellings_of_the_same_directory() {
        let a = project_name_for(Path::new("/games/wow-server-playerbots"));
        assert_eq!(a, project_name_for(Path::new("/games/wow-server-playerbots/")));
        assert_eq!(a, project_name_for(Path::new("/games/./wow-server-playerbots")));
        if cfg!(windows) {
            // NTFS is case-insensitive: one dir, one stack.
            assert_eq!(a, project_name_for(Path::new("/Games/WOW-Server-Playerbots")));
        }
    }

    /// A derived name is spliced into `name:` unquoted and into an image ref,
    /// so it must satisfy the same rules an explicit one does — for ANY
    /// directory a user might have.
    #[test]
    fn a_derived_identity_always_satisfies_the_scalar_rules() {
        for dir in [
            "/games/wow-server-playerbots",
            "/games/WoW Server (Playerbots)!",
            "/games/---",
            "/games/ÆØÅ",
            "/games/a-very-long-title-directory-name-that-goes-on-and-on-forever",
            "/",
        ] {
            let p = Path::new(dir);
            check_scalar("project_name", &project_name_for(p))
                .unwrap_or_else(|e| panic!("{dir}: {e:?}"));
            check_scalar("image_tag", &image_tag_for(p)).unwrap_or_else(|e| panic!("{dir}: {e:?}"));
        }
    }

    /// The default prefix must not be upstream's, or a build overwrites the
    /// refs a pulled/loaded stack runs.
    #[test]
    fn the_default_image_prefix_is_not_the_upstream_one() {
        assert_ne!(DEFAULT_IMAGE_PREFIX, UPSTREAM_IMAGE_PREFIX);
        assert!(!DEFAULT_IMAGE_PREFIX.starts_with("acore/"));
        check_scalar("image_prefix", DEFAULT_IMAGE_PREFIX).unwrap();
    }

    // -- .env merge ---------------------------------------------------------

    /// `.env` is shared property: `soap-setup` and the 3306 conflict remedy
    /// both write into it. The generator may only touch the keys it emits.
    #[test]
    fn merge_dotenv_replaces_its_own_keys_and_preserves_everything_else() {
        let existing = "# hand written\nDOCKER_DB_EXTERNAL_PORT=13306\nOTHER=keep\n";
        let out = merge_dotenv(
            existing,
            &["DB_ROOT_PASSWORD=s3cret".to_string(), "DOCKER_DB_EXTERNAL_PORT=13307".to_string()],
        );
        assert_eq!(
            out,
            "# hand written\nDOCKER_DB_EXTERNAL_PORT=13307\nOTHER=keep\nDB_ROOT_PASSWORD=s3cret\n"
        );
        // Replaced in place, not appended twice — a second assignment would be
        // ignored by both readers and silently disagree with the first.
        assert_eq!(out.matches("DOCKER_DB_EXTERNAL_PORT=").count(), 1);
        // A commented-out key is not an assignment.
        let out = merge_dotenv("#DB_ROOT_PASSWORD=old\n", &["DB_ROOT_PASSWORD=new".to_string()]);
        assert_eq!(out, "#DB_ROOT_PASSWORD=old\nDB_ROOT_PASSWORD=new\n");
    }

    #[test]
    fn dotenv_lines_are_empty_for_defaults_and_precise_otherwise() {
        assert!(dotenv_lines(&ComposeOpts::default()).is_empty());
        let opts = ComposeOpts {
            ports: Ports { db: 13307, soap: 7879, ..Ports::default() },
            db_password: "pw".into(),
            ..Default::default()
        };
        assert_eq!(
            dotenv_lines(&opts),
            vec![
                "DOCKER_DB_EXTERNAL_PORT=13307",
                "DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7879",
                "DB_ROOT_PASSWORD=pw",
            ]
        );
    }

    #[test]
    fn merge_drops_base_owned_wiring_but_keeps_runtime_keys() {
        let exported: BTreeMap<String, String> = [
            ("AC_LOGIN_DATABASE_INFO", "old;3306;root;pw;acore_auth"),
            ("AC_RATE_XP_KILL", "5"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let opts = merge_exported_override(&ComposeOpts::default(), &exported).unwrap();
        assert_eq!(opts.extra_env, vec![("AC_RATE_XP_KILL".to_string(), "5".to_string())]);
    }
}
