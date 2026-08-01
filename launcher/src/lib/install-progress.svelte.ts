// What the status surfaces show while a native install is running.
//
// A native install takes HOURS, almost all of it one docker build. For that
// whole time the polled server verdict is not merely uninformative, it is
// actively misleading: there is no stack yet, so the chip said "Stopped" and
// Home said "Couldn't read world status" while the machine was working as hard
// as it ever does. This store is what lets both say what is actually happening.
//
// Module-level runes state for the same reason `restart-state.svelte.ts` is:
// the install runs from Library but the chip is on every page, so a
// component-local store would die the moment the user navigated away -- which
// is exactly when a multi-hour job most needs to keep reporting.
//
// The reducer below is PURE and is where the behaviour lives; the store is just
// where the answer is kept. Everything here is vitest-pinned rather than
// click-tested, because the thing it reacts to takes hours to produce.

import type { TermEvent } from "$lib/api";

/** The engine's stage names — `install_native.rs`'s `Stage::name()`. */
export const BUILD_STAGE = "build";

export interface InstallProgress {
  /** An install is in flight. Drives the status override. */
  active: boolean;
  /** The engine stage name, or null before the first `section_start`. */
  stage: string | null;
  /**
   * 0-100, or null when no honest number exists.
   *
   * Null is the normal state for most of an install and is NOT zero: the
   * clones, the apt work and the cmake configure genuinely have no number, and
   * showing "0%" for twenty minutes of real work would be a lie a spinner
   * doesn't tell.
   */
  pct: number | null;
}

export function emptyProgress(): InstallProgress {
  return { active: false, stage: null, pct: null };
}

export const installProgress = $state<InstallProgress>(emptyProgress());

/**
 * One engine event → the next progress state. Pure.
 *
 * Only four events matter. Everything else — including every `line`, which is
 * the overwhelming majority — leaves the state untouched, so this stays cheap
 * on a stream that emits thousands of lines.
 */
export function installProgressReduce(prev: InstallProgress, e: TermEvent): InstallProgress {
  switch (e.event) {
    case "section_start": {
      const stage = (e as { name?: string }).name ?? null;
      // Entering ANY stage drops the percentage, which is what stops a stale
      // 99% from the build hanging over "Starting containers…". Re-entering the
      // build stage on a resume clears it too, and should: the previous
      // attempt's step total no longer applies.
      return { active: true, stage, pct: null };
    }

    case "pct": {
      const raw = (e as { value?: unknown }).value;
      if (typeof raw !== "number" || !Number.isFinite(raw)) return prev;
      // The engine already promises 0-100, monotonic, emit-on-change. Clamping
      // anyway costs nothing and means a future producer that forgets cannot
      // render a 4000%-wide bar here.
      const pct = Math.max(0, Math.min(100, Math.round(raw)));
      return { ...prev, active: true, pct };
    }

    case "done":
    case "error":
      // Both terminal events clear `active`. An install that ended -- either
      // way -- must hand the display back to the polled verdict, which from
      // here on is the truthful one.
      return emptyProgress();

    default:
      return prev;
  }
}

/** Apply one event to the shared store. */
export function noteInstallEvent(e: TermEvent): void {
  const next = installProgressReduce({ ...installProgress }, e);
  installProgress.active = next.active;
  installProgress.stage = next.stage;
  installProgress.pct = next.pct;
}

/**
 * A fresh install is starting, or a dead one is being cleared.
 *
 * Called on both edges deliberately. `startInstall()` marks active BEFORE the
 * first event arrives (the engine's preflight can take a few seconds, and a
 * chip that reads "Stopped" for those seconds then jumps is worse than one that
 * commits immediately), and the runner's IPC-rejection path clears it -- a
 * store left active forever would pin the chip to a stage that is no longer
 * running until the app restarts.
 */
export function setInstallActive(active: boolean): void {
  const next = active ? { active: true, stage: null, pct: null } : emptyProgress();
  installProgress.active = next.active;
  installProgress.stage = next.stage;
  installProgress.pct = next.pct;
}

/** Per-stage copy. Unknown stages fall back to a truthful generic. */
const STAGE_COPY: Record<string, string> = {
  preflight: "Checking your PC…",
  guard: "Checking your PC…",
  "clone-core": "Downloading AzerothCore…",
  "clone-module": "Downloading Playerbots…",
  "generate-compose": "Writing config…",
  build: "Building…",
  up: "Starting containers…",
  ready: "Waiting for the world…",
};

/**
 * The label for a progress state, e.g. `Building… 62%`.
 *
 * The percentage is appended ONLY when there is one. During the build's apt and
 * cmake-configure phases there is no ninja counter yet, and "Building…" with no
 * number is the honest rendering of that.
 */
export function installStatusText(p: InstallProgress): string {
  const base = (p.stage && STAGE_COPY[p.stage]) || "Installing…";
  return p.pct === null ? base : `${base.replace(/…$/, "")}… ${p.pct}%`;
}
