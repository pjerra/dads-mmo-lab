//! LIVE LIFECYCLE, opt-in and DELIBERATE. **This test starts and stops the
//! user's real WoW server.**
//!
//! # Read this before running it
//!
//! Everything in `live_arch_smoke.rs` is read-only. This file is not. It runs
//! `games start` and `games stop` against whatever title is installed in the
//! `dml-arch` distro — the user's actual server, with their actual characters
//! and their actual bots. It takes the world down and brings it back up.
//!
//! Do not run it on a whim, do not run it as part of a suite, and do not run it
//! while anyone is playing. Run it when you specifically intend to prove the
//! Arch lifecycle path, on a box you are allowed to disturb:
//!
//!   cargo test -p dml-core --test live_arch_lifecycle -- --ignored --nocapture
//!
//! # What it is for
//!
//! `start` on the Arch backend has only ever been unit-driven. The branch wired
//! [`dml_core::engine::EngineKind::for_backend`] so that `Backend::Arch` maps to
//! `EngineKind::Systemd` and reaches [`dml_core::engine::start_engine_systemd`]
//! (`sudo -n systemctl start docker`) instead of Docker Desktop's
//! `docker desktop start`, which does not exist inside a distro. Nothing has
//! ever exercised that end to end.
//!
//! It also proves the ONE safety addition the translation table makes to `stop`
//! that has no bash counterpart. `vocab::TABLE` appends `--no-stop-engine`,
//! because `dml-wow stop` otherwise stops "the engine" — and inside the distro
//! that is the distro's OWN dockerd. The assertion after the stop is therefore
//! not decoration: if the flag stopped riding along, the docker daemon would be
//! down and `wow server-detail` could no longer answer at all.
//!
//! # Safety rules this test enforces on ITSELF
//!
//!   * It REFUSES to run if the server is already running, so it can never stop
//!     a server somebody else started. It only ever puts the machine back the
//!     way it found it: stopped.
//!   * It touches no database, no config and no GM/SOAP verb.
//!   * It does not wait for `world_ready`. A cold WoW world can take 30 minutes
//!     to load, and elapsed-over-timeout measures the clock rather than the
//!     work. Containers up is the property this test is about.
//!
//! # Status
//!
//! WRITTEN BUT NOT RUN (2026-08-05). The live-smoke round that added it was
//! scoped to read-only verbs against the user's real server, so the lifecycle
//! half was left deliberately unexecuted. It compiles; it has never been run.

use dml_core::envelope::Envelope;
use dml_core::runner::DmlRunner;
use serde_json::Value;
use std::time::{Duration, Instant};

/// How long to let the containers come up before giving up. Generous: a cold
/// `compose up` on a first start pulls nothing but does create the network and
/// start four services.
const UP_TIMEOUT: Duration = Duration::from_secs(300);

/// How long to let them go away again.
const DOWN_TIMEOUT: Duration = Duration::from_secs(180);

fn arch() -> DmlRunner {
    DmlRunner::arch()
}

fn detail() -> Envelope {
    arch()
        .run_json(&["wow", "server-detail"])
        .expect("server-detail must answer")
}

fn verdict() -> String {
    detail().data["verdict"].as_str().unwrap_or("").to_string()
}

fn title_id() -> String {
    let env = arch().run_json(&["games", "list"]).expect("games list must answer");
    env.data["games"][0]["id"]
        .as_str()
        .expect("an installed title is a precondition")
        .to_string()
}

/// Drive a streaming verb to completion, echoing events, and return the
/// terminal event. Panics if the stream ends without one — a lifecycle command
/// that exits without `done` or `error` is exactly the silent failure the
/// launcher's terminal would render as a frozen button.
fn stream(argv: &[&str]) -> Value {
    let mut terminal: Option<Value> = None;
    let code = arch()
        .run_stream(argv, |v| {
            eprintln!("    {v}");
            if matches!(v["event"].as_str(), Some("done") | Some("error")) {
                terminal = Some(v);
            }
        })
        .unwrap_or_else(|e| panic!("{argv:?} failed to stream: {e:?}"));
    let t = terminal
        .unwrap_or_else(|| panic!("{argv:?} exited {code} with no terminal event"));
    assert_ne!(
        t["event"], "error",
        "{argv:?} ended in an error event: {t}"
    );
    t
}

fn wait_until(label: &str, limit: Duration, mut pred: impl FnMut(&str) -> bool) -> String {
    let start = Instant::now();
    loop {
        let v = verdict();
        if pred(&v) {
            eprintln!("    [{label}] reached verdict={v} after {:?}", start.elapsed());
            return v;
        }
        assert!(
            start.elapsed() < limit,
            "[{label}] still verdict={v} after {limit:?}"
        );
        std::thread::sleep(Duration::from_secs(5));
    }
}

/// The whole round trip: start, confirm, stop, confirm — and confirm the
/// daemon survived the stop.
#[test]
#[ignore = "STARTS AND STOPS THE USER'S REAL SERVER; run only deliberately, with --ignored"]
fn live_lifecycle_start_then_stop_on_the_arch_backend() {
    let id = title_id();

    // -- refuse rather than disturb someone else's session -------------------
    let before = verdict();
    assert_eq!(
        before, "stopped",
        "REFUSING: the server is already {before}. This test only runs against a \
         stopped server, so that it can never take down a session it did not start."
    );

    // -- start ---------------------------------------------------------------
    // Translated by vocab::TABLE to `dml-wow start --id <id>`, and it is this
    // path that reaches EngineKind::Systemd -> `sudo -n systemctl start docker`
    // rather than Docker Desktop.
    eprintln!("=== games start {id}");
    let done = stream(&["games", "start", id.as_str()]);
    eprintln!("    terminal: {done}");

    let up = wait_until("start", UP_TIMEOUT, |v| v != "stopped");
    assert_ne!(up, "stopped", "the stack never left the stopped verdict");

    // `games status` must agree with `server-detail` about what just happened.
    let st = arch()
        .run_json(&["games", "status", id.as_str()])
        .expect("games status must answer");
    assert_eq!(
        st.data["state"], "running",
        "games status disagrees with server-detail about a server we just started: {}",
        st.data
    );

    // The container table is the non-vacuity floor: a verdict alone could be
    // produced by a docker probe that answered about nothing.
    let d = detail();
    let running = d.data["containers"]
        .as_array()
        .map(|cs| cs.iter().filter(|c| c["state"] == "running").count())
        .unwrap_or(0);
    assert!(running > 0, "no container is running despite verdict={up}: {}", d.data);

    // -- stop ----------------------------------------------------------------
    // vocab::TABLE appends `--no-stop-engine` here. See the module doc.
    eprintln!("=== games stop {id}");
    let done = stream(&["games", "stop", id.as_str()]);
    eprintln!("    terminal: {done}");

    wait_until("stop", DOWN_TIMEOUT, |v| v == "stopped");

    // -- THE SAFETY ASSERTION ------------------------------------------------
    // `server-detail` reaches docker to build its container table. If the stop
    // had taken the distro's own dockerd down with it, this call could not
    // produce an ok envelope with a populated table — it would fail or report
    // the stack as unknowable. This is the live proof of `--no-stop-engine`.
    let after = detail();
    assert!(
        after.ok,
        "server-detail failed after the stop — did the engine go down with it? {:?}",
        after.error
    );
    let table = after.data["containers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !table.is_empty(),
        "docker answered with no container table after the stop; the daemon may be down: {}",
        after.data
    );
    assert!(
        table.iter().all(|c| c["state"] != "running"),
        "something is still running after the stop: {}",
        after.data
    );

    // -- put the machine back the way we found it ----------------------------
    assert_eq!(verdict(), "stopped", "the test must leave the server stopped");
}
