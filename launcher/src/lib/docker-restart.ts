// "Restart Docker in the distro" (Tools) -- pure decision + copy helpers.
//
// Why this exists at all: the 2026-07-21 incident was a wedged Docker network
// inside dml-arch. The fix was one command (`systemctl restart docker`) that
// was not clickable anywhere in the launcher, and Home's soap_unreachable card
// could only describe it in prose.
//
// Kept free of Svelte/Tauri so the three user-visible decisions -- does the
// card render at all, is the button armed, and what does a failure SAY -- are
// vitest-pinned (same shape as server-gate.ts).

import type { NativeSetupStatus } from "./api";

/**
 * Typed confirmation, deliberately NOT the two-click arm pattern LAN play
 * uses. Blast radius picks the pattern: LAN's arm guards a realmlist row that
 * one more click undoes, while restarting dockerd kills every running
 * container -- dockerd only gives containers its shutdown grace (seconds),
 * where the worldserver needs minutes to save, so a live world goes down HARD
 * and unsaved. That is Restart WSL territory, so it gets Restart WSL's typed
 * confirmation.
 */
export const DOCKER_RESTART_CONFIRM = "restart-docker";

/**
 * Whether the card renders. WSL-backend ONLY: native mode drives Docker
 * Desktop on Windows and has no dml-arch distro to restart at all (it gets the
 * "Start Docker Desktop" button in the Native setup card instead).
 *
 * A null status (the backend probe hasn't answered yet) hides the card rather
 * than showing it optimistically: for a destructive action, "we don't know
 * which backend this is" must not render a button that would target the wrong
 * daemon.
 */
export function dockerRestartCardVisible(status: Pick<NativeSetupStatus, "native"> | null): boolean {
  return status !== null && !status.native;
}

/** Exact-match typed confirmation, mirroring Restart WSL's `restart-wsl`. */
export function dockerRestartConfirmed(input: string): boolean {
  return input === DOCKER_RESTART_CONFIRM;
}

/** The three gates on the Restart button: feature lock, in-flight, confirmed. */
export function dockerRestartButtonDisabled(o: {
  busy: boolean;
  input: string;
  locked: boolean;
}): boolean {
  return o.busy || o.locked || !dockerRestartConfirmed(o.input);
}

/**
 * What to say after a call that did NOT throw. `restarted: false` means the
 * command came back without confirming the daemon is up again -- not the same
 * story as success, so it does not get success's copy.
 */
export function dockerRestartNote(restarted: boolean): string {
  return restarted
    ? "Docker restarted inside dml-arch. Your containers come back on their own (they restart unless you stopped them) — give the world a minute or two, then check Home."
    : "The restart ran, but Docker didn't confirm it came back. Wait a few seconds and re-check from Home — if it stays down, the Restart WSL card is the bigger hammer.";
}

/**
 * Failure copy. Every code in the fixed CLI contract becomes a sentence that
 * says what happened and what to do instead -- a raw NO_SUDO on screen means
 * nothing to the parent this launcher is for.
 */
export function dockerRestartErrorText(e: unknown): string {
  const err = e as { code?: string; message?: string; hint?: string };
  // HINT first, message second. The CLI's envelope splits a failure into a
  // generic sentence (message) and the underlying cause (hint) -- for
  // RESTART_FAILED that is systemctl's own stderr tail, "Unit docker.service
  // is masked." and friends (cli/src/90-main.sh:1724, pinned by
  // cli/tests/wow-docker-restart.bats). Reading `message` for the cause put a
  // tautology on screen ("It said: Could not restart the Docker daemon") and
  // dropped the one line the user could act on. The fallback keeps this
  // honest if a hint is ever empty.
  const detail = (err?.hint ?? "").trim() || (err?.message ?? "").trim();
  // systemctl's stderr already ends in a full stop; the CLI's own sentences do
  // not. Quote it as-is and only supply the missing period, so neither shape
  // renders "…is masked.." on screen.
  const said = detail ? ` It said: ${/[.!?]$/.test(detail) ? detail : `${detail}.`}` : "";
  switch (err?.code) {
    case "NOT_SUPPORTED":
      return "This distro doesn't run systemd, so the launcher can't restart Docker for it. Open the DML shell card and start Docker the way this distro does, or use the Restart WSL card — that restarts everything.";
    case "NO_SUDO":
      return "Restarting Docker needs passwordless sudo inside dml-arch, and it isn't set up. Open the DML shell card and run the restart yourself, or use the Restart WSL card instead.";
    case "RESTART_FAILED":
      // The only code whose envelope carries a per-run cause; the other four
      // are fully static on the CLI side (their hints just repeat the "DML
      // shell" / "Restart WSL" route these sentences already give), so they
      // stay hand-written copy.
      return `Docker's restart command failed inside dml-arch.${said} If it keeps failing, the Restart WSL card is the bigger hammer.`;
    case "RESTART_TIMEOUT":
      // NOT a failure story: dml-arch's docker.service is Type=notify with
      // TimeoutStartSec=0, so a dockerd wedged during startup leaves the
      // restart command waiting for a READY=1 that may still arrive. The CLI
      // kills it at its cap; the daemon can come up anyway, so the copy says
      // "still running", never "failed".
      //
      // The CLI raises this code for TWO different stalls with DIFFERENT
      // remedies (cli/src/90-main.sh): the systemd probe timing out, where
      // systemd itself is wedged and waiting will never help (the fix is
      // wsl --shutdown / Restart WSL), and the restart command timing out,
      // where Docker genuinely may still arrive. Only the hint tells them
      // apart, so this case MUST quote it -- the previous static sentence
      // told a user with a wedged systemd to "give it a minute", which is
      // the one thing that cannot work.
      return `Docker didn't finish restarting inside dml-arch — whatever the launcher was waiting for was still running when it stopped waiting.${said} If it stays down, the Restart WSL card is the bigger hammer.`;
    case "DOCKER_STILL_DOWN":
      return "Docker was restarted, but the daemon still isn't answering. Give it another few seconds and re-check — if it stays down, use the Restart WSL card.";
    default:
      // Same shape as Tools' own fmtErr, for anything outside the contract
      // (an IPC failure, a backend that grew a new code).
      return `${err?.message ?? String(e)}${err?.hint ? ` — ${err.hint}` : ""}`;
  }
}
