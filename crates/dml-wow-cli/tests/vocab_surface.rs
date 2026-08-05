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
    assert!(
        rows.len() >= 70,
        "expected >=70 dml-wow rows to check, got {} — the table or the filter is broken",
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

// ---------------------------------------------------------------------------
// T1b — do the promoted flags land on the RIGHT positionals?
//
// T1 above is a much weaker claim than it looks, and the gap is silent data
// corruption rather than a crash: TWO POSITIONALS OF THE SAME TYPE PARSE IN
// EITHER ORDER. Swap `wow party kick`'s `pos` from `["--player","--bot"]` to
// `["--bot","--player"]` and the launcher's `--player Bob --bot Zug` becomes
// `party kick Zug Bob` — kicking a bot named "Bob" from a player named "Zug" —
// and every suite in the workspace stays green. Mutation-proven, and ~19 rows
// are exposed, several of them mutating GM verbs: `account set-password`,
// `account set-gm`, `mail-item`, `gm gold`/`summon`/`at-login`, `config set`,
// `tuning set`, `party preset-save`/`preset-load`.
//
// The oracle stays the clap derive rather than a hand-written argv table —
// hand-writing the expected output is the failure mode this file was built to
// avoid. Clap knows every subcommand's positionals AND their index order, and
// the launcher's flag `--player` is the same word as the derive's field
// `player`. So the row's promotion order must equal the leaf subcommand's
// positional order, name for name. A row whose flags do NOT share their
// names with the arm's fields is not a hole — it fails loudly and is listed in
// `ALIASES` with the reason.
// ---------------------------------------------------------------------------

/// Bash flags whose `dml-wow` positional is spelled differently, and why.
///
/// Deliberately short and explicit rather than a fuzzy match: an alias is a
/// place where the two vocabularies disagree, and each one is a decision
/// somebody made. A NEW mismatch must be read and named here — absorbing it
/// silently is how an order bug would hide.
///
/// Every entry below is a rename of a SINGLE positional, where no order can be
/// wrong; that is not a coincidence, it is why nobody noticed the spellings had
/// drifted. Keyed on `(row verb, flag) -> field` so an alias cannot leak into
/// another row that happens to use the same flag name.
const ALIASES: &[(&[&str], &str, &str)] = &[
    // `dml-wow client-path set` names its argument for what it is on that side:
    // the WoW install DIRECTORY, not a free-form path.
    (&["wow", "client-path", "set"], "--path", "dir"),
    // `char` is a Rust KEYWORD, so no derive can ever spell this field `char`.
    // A structural rename, not a choice — but written out per row anyway, so a
    // sixth character-taking verb has to be looked at rather than absorbed.
    (&["wow", "teleport"], "--char", "char_name"),
    (&["wow", "paperdoll"], "--char", "name"),
    (&["wow", "char-progress"], "--char", "name"),
    (&["wow", "achievements"], "--char", "name"),
    // `--entries` is bash's word for the comma-separated id list; `dml-wow`
    // parses it into `ItemIds` and names it for the ids it holds.
    (&["wow", "item-info"], "--entries", "ids"),
    // bash's `--file` is `dml-wow`'s `name`, and the arm's own doc says why:
    // it is a file NAME from the `config files` allowlist, never a path.
    (&["wow", "config", "raw-read"], "--file", "name"),
    (&["wow", "config", "raw-write"], "--file", "name"),
];

/// The `dml-wow` positional this row's flag promotes to.
fn positional_id(verb: &[&str], flag: &str) -> String {
    for (v, from, to) in ALIASES {
        if *v == verb && *from == flag {
            return (*to).to_string();
        }
    }
    flag.trim_start_matches('-').replace('-', "_")
}

/// The leaf subcommand `argv` selects, walked through clap's own metadata.
/// Stops at the first token that names no subcommand — that token is the first
/// positional value.
fn leaf<'a>(root: &'a clap::Command, argv: &[String]) -> &'a clap::Command {
    let mut cur = root;
    for tok in argv {
        let Some(next) = cur
            .get_subcommands()
            .find(|s| s.get_name() == tok || s.get_all_aliases().any(|a| a == tok))
        else {
            break;
        };
        cur = next;
    }
    cur
}

/// The leaf's positionals, in the index order clap will fill them.
///
/// The caller MUST have called `Command::build()` on the root first: clap
/// assigns positional indices during the build, so on an unbuilt tree
/// `get_index()` answers `None` for everything and this returns an empty list —
/// which every prefix comparison below would then satisfy at length zero. A
/// vacuous pass that looks exactly like a clean one; the length floor in the
/// test is the second guard against it.
fn positionals_in_order(cmd: &clap::Command) -> Vec<String> {
    let mut v: Vec<(usize, String)> = cmd
        .get_arguments()
        .filter(|a| a.is_positional())
        .filter_map(|a| a.get_index().map(|i| (i, a.get_id().as_str().to_string())))
        .collect();
    v.sort_by_key(|(i, _)| *i);
    v.into_iter().map(|(_, id)| id).collect()
}

/// THE TEST. Every promoted flag lands on the positional that means the same
/// thing, in the same order.
#[test]
fn every_promoted_flag_lands_on_the_positional_that_means_the_same_thing() {
    use clap::CommandFactory;
    // `build()` is not optional — see `positionals_in_order`. Without it every
    // positional's index is `None` and the whole test passes by comparing
    // empty lists.
    let mut root = Cli::command();
    root.build();

    let mut checked = 0usize;
    let mut multi = 0usize;
    // COLLECTED, not asserted one at a time: a table-wide invariant should
    // report the whole truth in one run, or fixing five rows takes five runs
    // and the fifth is a surprise.
    let mut wrong: Vec<String> = Vec::new();

    for row in TABLE.iter().filter(|r| r.target == Target::DmlWow && !r.positionals.is_empty()) {
        let t = translate_for_arch(row.sample);
        let cmd = leaf(&root, &t.argv);
        let have = positionals_in_order(cmd);
        let want: Vec<String> =
            row.positionals.iter().map(|f| positional_id(row.verb, f)).collect();

        checked += 1;
        if want.len() >= 2 {
            multi += 1;
        }
        if have.len() < want.len() {
            wrong.push(format!(
                "{:?} promotes {} flags but `dml-wow {}` declares only {} positionals: {have:?}",
                row.verb,
                want.len(),
                cmd.get_name(),
                have.len()
            ));
            continue;
        }
        if have[..want.len()] != want[..] {
            wrong.push(format!(
                "{:?} promotes {:?} -> {want:?}, but `dml-wow {}` fills {:?} IN THAT ORDER",
                row.verb,
                row.positionals,
                cmd.get_name(),
                &have[..want.len()]
            ));
        }
    }

    // NON-VACUITY. A filter that matched nothing, or an unbuilt clap tree
    // (every `get_index()` `None`, so every `have` empty and every comparison
    // trivially true at length zero), must fail here rather than pass.
    assert!(checked >= 30, "only {checked} rows with promoted flags checked");
    assert!(
        multi >= 15,
        "only {multi} rows promote two or more flags — those are the only ones an ORDER \
         error can corrupt, and the scan found too few of them to be working"
    );
    assert!(
        wrong.is_empty(),
        "these rows send their values to the wrong `dml-wow` positionals:\n  {}\n\n\
         An order error here is SILENT: two positionals of the same type parse either way \
         round, so the command still runs — with the values swapped. Fix the row's `pos` \
         order in crates/dml-core/src/vocab.rs, or, if the two vocabularies really do spell \
         one argument differently, add it to ALIASES above WITH ITS REASON.",
        wrong.join("\n  ")
    );
}

/// THE INCIDENT, spelled out. The generic guard above covers all ~19 rows; this
/// names the one the reviewer mutated, end to end and in the launcher's own
/// words, because a swap here kicks the wrong character out of the wrong party.
#[test]
fn party_kick_sends_the_player_first_and_the_bot_second() {
    let t = translate_for_arch(&["wow", "party", "kick", "--player", "Bob", "--bot", "Zug"]);
    assert_eq!(t.argv, vec!["party", "kick", "Bob", "Zug"]);

    let argv: Vec<&str> =
        std::iter::once("dml-wow").chain(t.argv.iter().map(String::as_str)).collect();
    let parsed = Cli::try_parse_from(&argv).expect("the translation must parse");
    match parsed.command {
        dml_wow_cli::cli::Cmd::Party {
            cmd: dml_wow_cli::cli::PartyCmd::Kick { player, bot },
        } => {
            assert_eq!(player, "Bob", "the --player value must arrive as the player");
            assert_eq!(bot, "Zug", "the --bot value must arrive as the bot");
        }
        other => panic!("expected Party Kick, got {other:?}"),
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

// ---------------------------------------------------------------------------
// T3 — does the streaming flag in the table agree with `run.rs`?
//
// WEAKER THAN T1 BY DESIGN, and stated so it is not read as more: this is a
// source scan of ONE file. It proves an arm CALLS the streaming helper, not
// that the orchestration behind it emits a terminal event — that second
// property is `out.rs::stream_exit`'s own tests' job. It is a separate, weaker
// test rather than folded into T1 for exactly that reason.
//
// It matters because the launcher picks `run_stream` vs `run_json` per call
// site: a verb the table calls streaming but whose arm prints one envelope
// leaves the UI waiting for a terminal event that never comes, and the reverse
// hands NDJSON to an envelope parser.
// ---------------------------------------------------------------------------

/// Rust source with comments removed.
///
/// NOT shared with the launcher's copy: that one lives in a `#[cfg(test)]`
/// module in another crate, and a dev-dependency crate to hold sixty lines of
/// test utility is more machinery than the problem deserves.
///
/// It is not optional here. `run.rs` carries the line "Every arm from here down
/// STREAMS (see [`stream_dispatch`])" as a COMMENT sitting between two arms —
/// scan the raw text and `Cmd::Motd` inherits it and reads as streaming.
fn strip_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    let (mut in_str, mut in_raw, mut in_ch) = (false, false, false);
    while i < b.len() {
        let c = b[i] as char;
        let n = if i + 1 < b.len() { b[i + 1] as char } else { '\0' };
        if in_str {
            if c == '\\' && !in_raw {
                out.push(c);
                if i + 1 < b.len() {
                    out.push(n);
                }
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
                in_raw = false;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if in_ch {
            if c == '\\' {
                out.push(c);
                if i + 1 < b.len() {
                    out.push(n);
                }
                i += 2;
                continue;
            }
            if c == '\'' {
                in_ch = false;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && n == '/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && n == '*' {
            i += 2;
            let mut depth = 1usize;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            out.push(' ');
            continue;
        }
        if c == 'r' && (n == '"' || n == '#') {
            let mut j = i + 1;
            while j < b.len() && b[j] == b'#' {
                j += 1;
            }
            if j < b.len() && b[j] == b'"' {
                in_str = true;
                in_raw = true;
                for k in i..=j {
                    out.push(b[k] as char);
                }
                i = j + 1;
                continue;
            }
        }
        if c == '"' {
            in_str = true;
            in_raw = false;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '\'' {
            let closes = (i + 2 < b.len() && b[i + 2] == b'\'')
                || (n == '\\' && i + 3 < b.len() && b[i + 3] == b'\'');
            if closes {
                in_ch = true;
                out.push(c);
                i += 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Every match-arm body for `arm` (e.g. `Cmd::Restart`). More than one is
/// normal — `dispatch_account` matches `AccountCmd::Create` twice, once to
/// build the command and once to render the envelope.
fn arm_bodies(code: &str, arm: &str) -> Vec<String> {
    let b = code.as_bytes();
    let mut bodies = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = code[from..].find(arm) {
        let at = from + rel;
        from = at + arm.len();
        // Must START a match arm: only whitespace between it and the line start.
        let line_start = code[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        if !code[line_start..at].trim().is_empty() {
            continue;
        }
        // The next char must not continue the path, so `Cmd::Start` cannot
        // match a hypothetical `Cmd::StartSomething`.
        let after = at + arm.len();
        if code[after..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        // `=>` at pattern depth 0 — the `{ id }` binding braces come first.
        let mut i = after;
        let mut depth = 0i32;
        let mut fat = None;
        while i + 1 < b.len() {
            match b[i] {
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth -= 1,
                b'=' if depth == 0 && b[i + 1] == b'>' => {
                    fat = Some(i + 2);
                    break;
                }
                _ => {}
            }
            if depth < 0 {
                break;
            }
            i += 1;
        }
        let Some(mut j) = fat else { continue };
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        let start = j;
        if j < b.len() && b[j] == b'{' {
            let mut d = 0i32;
            while j < b.len() {
                match b[j] {
                    b'{' => d += 1,
                    b'}' => {
                        d -= 1;
                        if d == 0 {
                            j += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
        } else {
            let mut d = 0i32;
            while j < b.len() {
                match b[j] {
                    b'(' | b'[' | b'{' => d += 1,
                    b')' | b']' | b'}' => {
                        if d == 0 {
                            break;
                        }
                        d -= 1;
                    }
                    b',' if d == 0 => break,
                    _ => {}
                }
                j += 1;
            }
        }
        bodies.push(code[start..j.min(code.len())].to_string());
    }
    bodies
}

/// `None` when run.rs defines no such arm — which must FAIL rather than pass,
/// or a typo'd arm name silently makes the streaming check vacuous.
fn arm_streams(code: &str, arm: &str) -> Option<bool> {
    let bodies = arm_bodies(code, arm);
    if bodies.is_empty() {
        return None;
    }
    Some(bodies.iter().any(|b| b.contains("stream_dispatch")))
}

#[test]
fn the_streaming_flag_matches_run_rs() {
    let code = strip_comments(include_str!("../src/run.rs"));

    let mut streaming_found = 0usize;
    let mut checked = 0usize;
    for row in TABLE.iter().filter(|r| r.target == Target::DmlWow) {
        assert!(
            !row.arm.is_empty(),
            "dml-wow row {:?} names no run.rs arm, so its streaming flag is unpinned",
            row.verb
        );
        let Some(streams) = arm_streams(&code, row.arm) else {
            panic!("row {:?} names arm {:?}, which run.rs does not define", row.verb, row.arm);
        };
        checked += 1;
        if streams {
            streaming_found += 1;
        }
        assert_eq!(
            streams, row.streams,
            "row {:?} says streams={} but {}'s arm in run.rs {} stream_dispatch",
            row.verb,
            row.streams,
            row.arm,
            if streams { "calls" } else { "does not call" }
        );
    }

    // NON-VACUITY. A scanner answering "everything streams" passes the count
    // and fails the Version probe; one answering "nothing streams" fails the
    // count and the Restart probe.
    assert!(checked >= 70, "only {checked} arms checked");
    assert!(streaming_found >= 12, "only {streaming_found} streaming arms found");
    assert_eq!(
        arm_streams(&code, "Cmd::Version"),
        Some(false),
        "the scanner reports the simplest non-streaming arm as streaming"
    );
    assert_eq!(
        arm_streams(&code, "Cmd::Restart"),
        Some(true),
        "the scanner cannot see stream_dispatch in an arm that plainly calls it"
    );
}

/// The comment hazard, proven directly rather than assumed: `run.rs` really
/// does name `stream_dispatch` in prose between two arms, and the scan really
/// is immune to it.
#[test]
fn a_comment_naming_stream_dispatch_does_not_make_an_arm_stream() {
    let raw = include_str!("../src/run.rs");
    assert!(
        raw.contains("Every arm from here down STREAMS (see [`stream_dispatch`])"),
        "the prose this test exists to survive has moved; re-check the scan"
    );
    let code = strip_comments(raw);
    assert!(!code.contains("Every arm from here down"), "comments were not stripped");
    // `Cmd::Motd` is the arm immediately before that comment block.
    assert_eq!(arm_streams(&code, "Cmd::Motd"), Some(false));
}
