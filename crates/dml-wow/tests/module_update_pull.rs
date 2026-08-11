//! `wow module update`'s PULL PATH against a real git repo — the coverage the
//! Rust side never had (the bash oracle's `cli/tests/wow-module-update.bats`
//! has driven real clones against local bare origins since the module-update
//! round; the Rust port was only ever tested at the guard/refusal level, so a
//! whole class of "the pull did the wrong thing quietly" bugs could only be
//! caught on one surface).
//!
//! Zero network: every remote here is a LOCAL bare repo inside the test's own
//! temp tree, exactly like the bats fixture this mirrors
//! (`wow-module-update.bats:31-49`). `tempfile` is NOT a dependency of this
//! crate (see `native_compose_gen.rs`'s note), so the temp trees are built and
//! torn down by hand.
//!
//! The one docker touch on this path is `cpp_build_guard`'s `compose config`
//! parse; it is answered by a per-test fake `docker` that always reports a
//! worldserver WITH a `build:` key, i.e. the guard resolves `Some(true)` and
//! the flow proceeds — the guard itself is already covered by the unit tests
//! in `modmgr.rs`.

use std::cell::RefCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    root: PathBuf,
    sdir: PathBuf,
    origin: PathBuf,
    clone: PathBuf,
    key: String,
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "dml test")
        .env("GIT_AUTHOR_EMAIL", "dml-test@example.invalid")
        .env("GIT_COMMITTER_NAME", "dml test")
        .env("GIT_COMMITTER_EMAIL", "dml-test@example.invalid")
        .status()
        .unwrap_or_else(|e| panic!("spawning git {args:?}: {e}"))
        .success();
    assert!(ok, "git {args:?} failed in {}", dir.display());
}

impl Fixture {
    /// A server dir holding `modules/<key>`, a real clone of a real local bare
    /// origin with two tracked files (`a.txt` is what the origin advances,
    /// `b.txt` is what a "local edit" test dirties — same split as the bats
    /// fixture, so a stash pop re-applies cleanly instead of conflicting).
    fn new(tag: &str, key: &str) -> Fixture {
        let root = std::env::temp_dir().join(format!("dml-modupd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let sdir = root.join("server");
        let origin = root.join("origin.git");
        let clone = sdir.join("modules").join(key);
        std::fs::create_dir_all(sdir.join("modules")).unwrap();
        std::fs::create_dir_all(&root).unwrap();

        // seed -> bare origin -> the installed clone.
        let seed = root.join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        git(&root, &["init", "-q", "--initial-branch=master", seed.to_str().unwrap()]);
        std::fs::write(seed.join("a.txt"), "one\n").unwrap();
        std::fs::write(seed.join("b.txt"), "keep\n").unwrap();
        git(&seed, &["add", "a.txt", "b.txt"]);
        git(&seed, &["commit", "-qm", "seed"]);
        git(&root, &["clone", "-q", "--bare", seed.to_str().unwrap(), origin.to_str().unwrap()]);
        let _ = std::fs::remove_dir_all(&seed);
        git(&root, &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);

        Fixture { root, sdir, origin, clone, key: key.to_string() }
    }

    /// One more commit on the origin (touches `a.txt`), so the installed clone
    /// is exactly one behind.
    fn origin_gains_commit(&self) {
        let work = self.root.join("adv");
        let _ = std::fs::remove_dir_all(&work);
        git(&self.root, &["clone", "-q", self.origin.to_str().unwrap(), work.to_str().unwrap()]);
        std::fs::write(work.join("a.txt"), "one\ntwo\n").unwrap();
        git(&work, &["add", "a.txt"]);
        git(&work, &["commit", "-qm", "advance"]);
        git(&work, &["push", "-q", "origin", "HEAD"]);
        let _ = std::fs::remove_dir_all(&work);
    }

    fn head(&self) -> String {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&self.clone)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// The single `local-changes-*.patch` `wow_pull_repo` wrote into the clone.
    fn patch_text(&self) -> String {
        let mut found: Option<PathBuf> = None;
        for e in std::fs::read_dir(&self.clone).unwrap().flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("local-changes-") && n.ends_with(".patch") {
                found = Some(e.path());
            }
        }
        let p = found.unwrap_or_else(|| panic!("no local-changes-*.patch in {}", self.clone.display()));
        std::fs::read_to_string(p).unwrap()
    }

    /// Drives the REAL `module update` orchestration
    /// ([`dml_wow::modmgr::module_update_stream_with`]) against this fixture's
    /// server dir and the always-`Some(true)` fake docker.
    fn run_update(&self) -> Vec<serde_json::Value> {
        let docker = write_fake_docker(&self.root);
        let events = RefCell::new(Vec::new());
        dml_wow::modmgr::module_update_stream_with(docker.into_os_string(), self.sdir.clone(), self.key.clone(), |v| {
            events.borrow_mut().push(v)
        });
        events.into_inner()
    }

    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A stand-in `docker` answering `compose … config --format json` with a
/// worldserver that HAS a `build:` key (guard -> `Some(true)`). Per-platform
/// per the repo's test-portability rule — never a hardcoded interpreter.
#[cfg(windows)]
fn write_fake_docker(dir: &Path) -> PathBuf {
    let p = dir.join("fake-docker.cmd");
    std::fs::write(
        &p,
        "@echo off\r\necho {\"services\":{\"ac-worldserver\":{\"build\":{\"context\":\".\"}}}}\r\nexit /b 0\r\n",
    )
    .unwrap();
    p
}

#[cfg(not(windows))]
fn write_fake_docker(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = dir.join("fake-docker.sh");
    std::fs::write(
        &p,
        "#!/bin/sh\necho '{\"services\":{\"ac-worldserver\":{\"build\":{\"context\":\".\"}}}}'\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

fn lines_of(events: &[serde_json::Value]) -> Vec<String> {
    events
        .iter()
        .filter(|e| e["event"] == "line")
        .map(|e| e["text"].as_str().unwrap_or_default().to_string())
        .collect()
}

fn done_data(events: &[serde_json::Value]) -> serde_json::Value {
    events
        .iter()
        .find(|e| e["event"] == "done")
        .unwrap_or_else(|| panic!("no done event in {events:#?}"))["data"]
        .clone()
}

// ---------------------------------------------------------------------------
// (c) The local-edits patch backup must capture STAGED edits too.
// ---------------------------------------------------------------------------

#[test]
fn b_staged_edits_land_in_the_backup_patch() {
    let f = Fixture::new("staged", "mod-fixture");
    std::fs::write(f.clone.join("b.txt"), "STAGED EDIT\n").unwrap();
    git(&f.clone, &["add", "b.txt"]);

    let events = RefCell::new(Vec::new());
    let changed = dml_wow::modmgr::wow_pull_repo(OsStr::new("git"), &f.clone, &f.key, &|v| events.borrow_mut().push(v));
    assert_eq!(changed, Ok(false), "{:#?}", events.borrow());

    let patch = f.patch_text();
    assert!(patch.contains("STAGED EDIT"), "staged edit missing from the backup patch:\n{patch}");
    f.cleanup();
}

// ---------------------------------------------------------------------------
// (b) `pending_rebuild` must report what actually happened, and (a) the
// db-import advisory may only be said when the marker really landed.
// ---------------------------------------------------------------------------

const ADVISORY: &str = "module SQL (if any) is applied automatically by the server's db-import on next start -- never by hand";

#[test]
fn b_changed_pull_marks_rebuild_pending() {
    let f = Fixture::new("changed", "mod-fixture");
    let before = f.head();
    f.origin_gains_commit();

    let events = f.run_update();
    let data = done_data(&events);
    assert_eq!(data["changed"], true, "{events:#?}");
    assert_eq!(data["pending_rebuild"], true, "{events:#?}");
    assert_eq!(data["rebuild_required"], true, "{events:#?}");
    assert_eq!(data["before"], before, "{events:#?}");
    assert_eq!(data["after"], f.head(), "{events:#?}");
    assert_ne!(data["before"], data["after"], "{events:#?}");

    let marker = std::fs::read_to_string(f.sdir.join(".dml-rebuild-pending")).unwrap();
    assert!(marker.lines().any(|l| l == "mod-fixture"), "marker: {marker:?}");
    // The advisory is true here: the module joined the rebuild-pending list,
    // so the next rebuild's db-import really will pick its SQL up.
    let lines = lines_of(&events);
    assert!(lines.iter().any(|l| l == ADVISORY), "{lines:#?}");
    f.cleanup();
}

#[test]
fn b_up_to_date_pull_writes_no_marker() {
    let f = Fixture::new("uptodate", "mod-fixture");

    let events = f.run_update();
    let data = done_data(&events);
    assert_eq!(data["changed"], false, "{events:#?}");
    assert_eq!(data["pending_rebuild"], false, "{events:#?}");
    assert_eq!(data["before"], data["after"], "{events:#?}");
    assert!(!f.sdir.join(".dml-rebuild-pending").exists(), "nothing changed -- no marker expected");
    // Nothing was pulled, so there is no new SQL to talk about.
    let lines = lines_of(&events);
    assert!(!lines.iter().any(|l| l == ADVISORY), "{lines:#?}");
    f.cleanup();
}

#[test]
fn b_dirty_worktree_edit_survives_and_patch_written() {
    let f = Fixture::new("dirty", "mod-fixture");
    f.origin_gains_commit();
    std::fs::write(f.clone.join("b.txt"), "keep\nlocal-edit\n").unwrap();

    let events = f.run_update();
    assert_eq!(done_data(&events)["changed"], true, "{events:#?}");
    assert!(f.patch_text().contains("local-edit"), "patch: {}", f.patch_text());
    // The stash was popped back on top of the update…
    assert!(std::fs::read_to_string(f.clone.join("b.txt")).unwrap().contains("local-edit"));
    // …and the update itself landed.
    assert!(std::fs::read_to_string(f.clone.join("a.txt")).unwrap().contains("two"));
    f.cleanup();
}

#[test]
fn b_marker_write_failure_reports_pending_rebuild_false_and_warns() {
    let f = Fixture::new("nomarker", "mod-fixture");
    f.origin_gains_commit();
    // A DIRECTORY where the marker file belongs: `rebuild_pending_add`'s
    // append-open fails on every platform, so the banner can never light up.
    std::fs::create_dir_all(f.sdir.join(".dml-rebuild-pending")).unwrap();

    let events = f.run_update();
    let data = done_data(&events);
    assert_eq!(data["changed"], true, "{events:#?}");
    assert_eq!(data["pending_rebuild"], false, "a failed marker write must not be reported as pending:\n{events:#?}");
    // …but the module still NEEDS compiling: the pair is what lets the
    // launcher tell "nothing to compile" apart from "nobody queued it".
    assert_eq!(data["rebuild_required"], true, "{events:#?}");

    let lines = lines_of(&events);
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "line" && e["level"] == "warn" && e["text"].as_str().unwrap_or_default().contains(".dml-rebuild-pending")),
        "expected a warn naming the marker path:\n{lines:#?}"
    );
    // (a) The advisory is FALSE when nothing is pending: db-import only runs
    // the module's SQL as part of a rebuild the banner never asked for.
    assert!(!lines.iter().any(|l| l == ADVISORY), "advisory claimed on an unmarked update:\n{lines:#?}");
    f.cleanup();
}

#[test]
fn b_arac_update_says_its_sql_is_not_auto_applied() {
    let f = Fixture::new("arac", "mod-arac");
    f.origin_gains_commit();

    let events = f.run_update();
    let data = done_data(&events);
    assert_eq!(data["changed"], true, "{events:#?}");
    assert_eq!(data["pending_rebuild"], false, "mod-arac never joins the rebuild list:\n{events:#?}");
    assert_eq!(data["rebuild_required"], false, "mod-arac ships no C++ at all:\n{events:#?}");
    assert!(!f.sdir.join(".dml-rebuild-pending").exists(), "mod-arac must not be marked pending");

    let lines = lines_of(&events);
    assert!(
        lines.iter().any(|l| l == "mod-arac is data-only: new SQL is NOT auto-applied -- re-run Apply client patch / apply its SQL manually (Repair panel)."),
        "{lines:#?}"
    );
    // The old blanket claim must be GONE for arac: it never rebuilds, so its
    // db-import never sees the new SQL.
    assert!(!lines.iter().any(|l| l == ADVISORY), "the db-import advisory is a lie for mod-arac:\n{lines:#?}");
    f.cleanup();
}
