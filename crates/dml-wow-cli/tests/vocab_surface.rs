//! T1 — does every translated argv actually PARSE as `dml-wow`?
//!
//! The whole vocabulary bug shipped because nothing compared the launcher's
//! verbs against `dml-wow`'s real surface. A table of subcommand names typed
//! into a test would have the same failure mode, so the oracle here is the clap
//! DERIVE ITSELF: [`dml_core::vocab::translate_for_arch`] is run over each
//! row's own sample argv and the result is handed to `Cli::try_parse_from`.
//!
//! `try_parse_from` runs no dispatch — nothing spawns, nothing touches docker,
//! SOAP or the database.

use clap::Parser;
use dml_core::vocab::{translate_for_arch, Target, TABLE};
use dml_wow_cli::cli::Cli;

/// Rows that must reach `dml-wow`, and the parse that proves they can.
#[test]
fn every_dml_wow_translation_parses_under_the_real_clap_tree() {
    let rows: Vec<_> = TABLE.iter().filter(|r| r.target == Target::DmlWow).collect();

    // NON-VACUITY. A table that lost its rows, or a filter that matched
    // nothing, must fail here rather than pass an empty loop.
    // (Raised to 70 when `games-list`/`games-status` land.)
    assert!(
        rows.len() >= 68,
        "expected >=68 dml-wow rows to check, got {} — the table or the filter is broken",
        rows.len()
    );

    for row in rows {
        let t = translate_for_arch(row.sample);
        assert_eq!(
            t.target,
            Target::DmlWow,
            "row {:?} is declared DmlWow but its own sample translated to {:?}",
            row.verb,
            t.target
        );
        let argv: Vec<&str> = std::iter::once("dml-wow").chain(t.argv.iter().map(String::as_str)).collect();
        if let Err(e) = Cli::try_parse_from(&argv) {
            panic!(
                "launcher sends {:?}\n  translated to {:?}\n  which dml-wow REFUSES: {}",
                row.sample,
                t.argv,
                e.to_string().lines().next().unwrap_or("")
            );
        }
    }
}

/// The other half of the incompatibility: `--json`. Verified against the real
/// binary — `dml-wow version --json` is "unexpected argument '--json' found".
/// If the translation ever let one through, this is where it shows up.
#[test]
fn dml_wow_would_reject_the_json_flag_the_bash_runner_appends() {
    assert!(
        Cli::try_parse_from(["dml-wow", "version", "--json"]).is_err(),
        "dml-wow grew a global --json; the runner's is_bash condition needs revisiting"
    );
}

/// R2, pinned on the parse side too: the `--no-stop-engine` the translation
/// adds must be a flag `dml-wow stop` really has. A row asserting a flag that
/// clap does not define would otherwise only fail at runtime, on a live server.
#[test]
fn stop_accepts_the_no_stop_engine_flag_the_translation_adds() {
    let t = translate_for_arch(&["games", "stop", "wow-server-playerbots"]);
    assert!(
        t.argv.iter().any(|a| a == "--no-stop-engine"),
        "the stop translation lost --no-stop-engine: {:?}",
        t.argv
    );
    let argv: Vec<&str> = std::iter::once("dml-wow").chain(t.argv.iter().map(String::as_str)).collect();
    assert!(Cli::try_parse_from(&argv).is_ok());
}

/// A `Bash` row must be left EXACTLY as the launcher sent it — that is the
/// property the whole fallback rests on.
#[test]
fn every_bash_row_passes_its_argv_through_untouched() {
    let rows: Vec<_> = TABLE.iter().filter(|r| r.target == Target::Bash).collect();
    assert!(rows.len() >= 30, "expected >=30 bash rows, got {}", rows.len());
    for row in rows {
        let t = translate_for_arch(row.sample);
        assert_eq!(t.target, Target::Bash, "row {:?} changed target", row.verb);
        assert_eq!(
            t.argv,
            row.sample.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "row {:?} is a bash row but its argv was rewritten",
            row.verb
        );
    }
}
