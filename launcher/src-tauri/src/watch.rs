//! Auto-shutdown state machine (Batch 2 F5): pure "poll result in → action
//! out" logic, kept free of process/timer/IPC concerns so it can be
//! cargo-tested exhaustively. The impure watcher loop in lib.rs owns the 5s
//! cadence, the `tasklist` probe, and the actual CLI stop; this module only
//! decides WHEN.
//!
//! Lifecycle: DISARMED until Wow.exe is first seen while the watcher is
//! enabled; once ARMED, two consecutive polls without Wow.exe fire the stop
//! (a single missed poll is debounced -- tasklist can transiently miss a
//! process during heavy load). Firing auto-disarms: the machine returns to
//! DISARMED and will not fire again until Wow.exe reappears and vanishes
//! again.

/// What the impure loop should do after feeding one poll result in.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum WatchAction {
    /// Nothing to do.
    None,
    /// Wow.exe was seen for the first time since (re-)arming -- surface
    /// "armed" state to the UI.
    Armed,
    /// Wow.exe has been gone for two consecutive polls -- run the graceful
    /// stop (subject to the caller's own "server actually up" guard).
    Fire,
}

/// Debounce threshold: consecutive polls without Wow.exe before firing.
pub const MISS_THRESHOLD: u8 = 2;

#[derive(Debug, Default)]
pub struct WatchMachine {
    armed: bool,
    misses: u8,
}

impl WatchMachine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one poll result (was Wow.exe running?) and get the action.
    pub fn step(&mut self, wow_running: bool) -> WatchAction {
        if wow_running {
            let was_armed = self.armed;
            self.armed = true;
            self.misses = 0;
            if was_armed {
                WatchAction::None
            } else {
                WatchAction::Armed
            }
        } else if self.armed {
            self.misses += 1;
            if self.misses >= MISS_THRESHOLD {
                // Auto-disarm: back to waiting-for-wow, never a second fire
                // until Wow.exe is seen (and lost) again.
                self.armed = false;
                self.misses = 0;
                WatchAction::Fire
            } else {
                WatchAction::None
            }
        } else {
            WatchAction::None
        }
    }

    /// True once Wow.exe has been seen and the machine is waiting for it to
    /// close (drives the UI's "armed" vs "waiting for WoW" status line).
    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

/// Pure parse of `tasklist /FI "IMAGENAME eq Wow.exe" /FO CSV /NH` output.
/// A matching process yields a CSV row whose first field is the image name
/// (`"Wow.exe","1234",...`); no match yields an INFO: line (or nothing).
/// Case-insensitive: the on-disk binary can be wow.exe/WoW.exe.
pub fn tasklist_shows_wow(output: &str) -> bool {
    output
        .lines()
        .any(|l| l.trim_start().to_ascii_lowercase().starts_with("\"wow.exe\""))
}

/// Classify one tasklist invocation into a tri-state observation:
/// `Some(true)` = Wow.exe seen, `Some(false)` = a genuine "not running"
/// answer (tasklist's INFO line), `None` = NO usable observation.
///
/// The None case matters: a spawn failure, a nonzero exit ("ERROR: The RPC
/// server is unavailable" under session/load trouble), or empty output are
/// correlated failure modes -- treating them as "WoW is gone" would let two
/// such failures 5s apart clear the 2-poll debounce and gracefully stop the
/// server out from under a player who is still in-game. The watcher skips
/// the machine step on None (no observation), so only a REAL "gone" answer
/// (the non-empty INFO line -> Some(false)) can ever count toward a stop.
pub fn classify_tasklist(exit_ok: bool, stdout: &str) -> Option<bool> {
    if !exit_ok {
        return None;
    }
    if stdout.trim().is_empty() {
        return None;
    }
    Some(tasklist_shows_wow(stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_disarmed_while_wow_never_appears() {
        let mut m = WatchMachine::new();
        for _ in 0..10 {
            assert_eq!(m.step(false), WatchAction::None);
        }
        assert!(!m.is_armed());
    }

    #[test]
    fn arms_on_first_wow_sighting_then_stays_quietly_armed() {
        let mut m = WatchMachine::new();
        assert_eq!(m.step(true), WatchAction::Armed);
        assert!(m.is_armed());
        assert_eq!(m.step(true), WatchAction::None);
        assert_eq!(m.step(true), WatchAction::None);
        assert!(m.is_armed());
    }

    #[test]
    fn one_missed_poll_is_debounced_and_a_resighting_resets_the_count() {
        let mut m = WatchMachine::new();
        m.step(true);
        assert_eq!(m.step(false), WatchAction::None); // 1 miss: no fire
        assert_eq!(m.step(true), WatchAction::None); // back -- resets misses
        assert_eq!(m.step(false), WatchAction::None); // 1 miss again
        assert_eq!(m.step(false), WatchAction::Fire); // 2nd consecutive: fire
    }

    #[test]
    fn fires_after_two_consecutive_misses_and_only_once() {
        let mut m = WatchMachine::new();
        m.step(true);
        assert_eq!(m.step(false), WatchAction::None);
        assert_eq!(m.step(false), WatchAction::Fire);
        // Auto-disarmed: further absence never re-fires.
        for _ in 0..10 {
            assert_eq!(m.step(false), WatchAction::None);
        }
        assert!(!m.is_armed());
    }

    #[test]
    fn rearms_and_can_fire_again_after_wow_returns() {
        let mut m = WatchMachine::new();
        m.step(true);
        m.step(false);
        assert_eq!(m.step(false), WatchAction::Fire);
        assert_eq!(m.step(true), WatchAction::Armed); // second session
        assert_eq!(m.step(false), WatchAction::None);
        assert_eq!(m.step(false), WatchAction::Fire);
    }

    #[test]
    fn never_fires_when_disabled_because_a_fresh_machine_starts_disarmed() {
        // "Disabled" is modeled by the caller dropping the machine (the
        // watcher thread exits); re-enabling builds a fresh one, which must
        // not fire from stale state even if WoW is already gone.
        let mut m = WatchMachine::new();
        for _ in 0..5 {
            assert_eq!(m.step(false), WatchAction::None);
        }
    }

    #[test]
    fn tasklist_parse_matches_the_csv_row_case_insensitively() {
        assert!(tasklist_shows_wow("\"Wow.exe\",\"4242\",\"Console\",\"1\",\"1,556,000 K\"\r\n"));
        assert!(tasklist_shows_wow("\"wow.exe\",\"1\",\"Console\",\"1\",\"1 K\""));
        assert!(tasklist_shows_wow("\"WOW.EXE\",\"1\",\"Console\",\"1\",\"1 K\""));
    }

    #[test]
    fn tasklist_parse_rejects_no_match_and_lookalike_output() {
        assert!(!tasklist_shows_wow(""));
        assert!(!tasklist_shows_wow(
            "INFO: No tasks are running which match the specified criteria.\r\n"
        ));
        // Another process mentioning wow.exe in a later field must not count.
        assert!(!tasklist_shows_wow("\"cmd.exe\",\"9\",\"Console\",\"1\",\"wow.exe K\""));
        assert!(!tasklist_shows_wow("\"Wowhead.exe\",\"9\",\"Console\",\"1\",\"1 K\""));
    }

    #[test]
    fn classify_tasklist_is_a_real_observation_only_on_success_with_output() {
        // A live game.
        assert_eq!(
            classify_tasklist(true, "\"Wow.exe\",\"4242\",\"Console\",\"1\",\"1 K\"\r\n"),
            Some(true)
        );
        // A genuine "gone" answer: tasklist's INFO line.
        assert_eq!(
            classify_tasklist(true, "INFO: No tasks are running which match the specified criteria.\r\n"),
            Some(false)
        );
    }

    #[test]
    fn classify_tasklist_returns_none_on_probe_failure() {
        // Nonzero exit (RPC unavailable etc.) -> no observation, NOT "gone".
        assert_eq!(classify_tasklist(false, ""), None);
        assert_eq!(classify_tasklist(false, "ERROR: The RPC server is unavailable."), None);
        // Success but empty stdout is equally unusable.
        assert_eq!(classify_tasklist(true, "   \r\n"), None);
    }

    #[test]
    fn two_probe_failures_never_fire_a_stop_when_skipped() {
        // The watcher skips machine.step on None. Model that: an armed
        // machine that only ever sees real observations must not fire from
        // probe failures alone.
        let mut m = WatchMachine::new();
        assert_eq!(m.step(true), WatchAction::Armed); // WoW seen
                                                      // ...two probe failures happen here; the loop skips step() for both.
                                                      // The next REAL observation still shows WoW running:
        assert_eq!(m.step(true), WatchAction::None);
        assert!(m.is_armed()); // never fired
    }
}
