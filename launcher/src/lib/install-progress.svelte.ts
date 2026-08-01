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
// The reducer below is PURE (its clock is injected) and is where the behaviour
// lives; the store is just where the answer is kept. Everything here is
// vitest-pinned rather than click-tested, because the thing it reacts to takes
// hours to produce.

import type { TermEvent } from "$lib/api";

/** Engine stage names — `install_native.rs`'s `Stage::name()`. */
export const BUILD_STAGE = "build";
export const READY_STAGE = "ready";

export interface InstallProgress {
  /** An install is in flight. Drives the status override. */
  active: boolean;
  /** The engine stage name, or null before the first `section_start`. */
  stage: string | null;
  /**
   * 0-100, or null when no honest number exists.
   *
   * Null is the normal state for parts of an install and is NOT zero: apt, the
   * cmake configure and git's server-side phases genuinely have no number, and
   * showing "0%" for real work would be a lie a spinner doesn't tell.
   */
  pct: number | null;
  /**
   * Seconds this stage may run before giving up, when it is a bounded WAIT.
   *
   * Only `ready` carries one. It is deliberately NOT turned into a percentage:
   * elapsed-over-timeout measures the clock, not the work, and the world can
   * come up at 20% or at 99%.
   */
  limitSecs: number | null;
  /** When the current stage began, epoch ms. */
  stageStartedAt: number | null;
  /** The clock the elapsed readout renders against, epoch ms. */
  nowMs: number;
}

export function emptyProgress(): InstallProgress {
  return { active: false, stage: null, pct: null, limitSecs: null, stageStartedAt: null, nowMs: 0 };
}

export const installProgress = $state<InstallProgress>(emptyProgress());

/**
 * One engine event → the next progress state. Pure; `nowMs` is injected rather
 * than read, so the elapsed readout is testable without waiting for a clock.
 *
 * Only four events matter. Everything else — including every `line`, which is
 * the overwhelming majority — leaves the state untouched, so this stays cheap
 * on a stream that emits thousands of lines.
 */
export function installProgressReduce(
  prev: InstallProgress,
  e: TermEvent,
  nowMs: number,
): InstallProgress {
  switch (e.event) {
    case "section_start": {
      const { name, limit_secs } = e as { name?: string; limit_secs?: unknown };
      // Entering ANY stage drops the percentage, which is what stops a stale
      // 99% from the build hanging over "Starting containers…". Re-entering a
      // stage on a resume clears it too, and should: the previous attempt's
      // denominator no longer applies.
      return {
        active: true,
        stage: name ?? null,
        pct: null,
        limitSecs: typeof limit_secs === "number" && Number.isFinite(limit_secs) ? limit_secs : null,
        stageStartedAt: nowMs,
        nowMs,
      };
    }

    case "pct": {
      const raw = (e as { value?: unknown }).value;
      if (typeof raw !== "number" || !Number.isFinite(raw)) return prev;
      // The engine already promises 0-100, monotonic, emit-on-change. Clamping
      // anyway costs nothing and means a future producer that forgets cannot
      // render a 4000%-wide bar here.
      const pct = Math.max(0, Math.min(100, Math.round(raw)));
      return { ...prev, active: true, pct, nowMs };
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

function apply(next: InstallProgress): void {
  installProgress.active = next.active;
  installProgress.stage = next.stage;
  installProgress.pct = next.pct;
  installProgress.limitSecs = next.limitSecs;
  installProgress.stageStartedAt = next.stageStartedAt;
  installProgress.nowMs = next.nowMs;
}

// The elapsed readout is the ONE thing here that changes without an event
// arriving: `ready` can sit silent for twenty minutes importing the world
// database. The ticker runs only while that stage is open, so an install
// spending hours in `build` costs nothing.
let tickHandle: ReturnType<typeof setInterval> | null = null;

function syncTicker(): void {
  const wanted = installProgress.active && installProgress.stage === READY_STAGE;
  if (wanted && tickHandle === null) {
    tickHandle = setInterval(() => {
      installProgress.nowMs = Date.now();
    }, 1000);
  } else if (!wanted && tickHandle !== null) {
    clearInterval(tickHandle);
    tickHandle = null;
  }
}

/** Apply one event to the shared store. */
export function noteInstallEvent(e: TermEvent): void {
  apply(installProgressReduce({ ...installProgress }, e, Date.now()));
  syncTicker();
}

/**
 * A fresh install is starting, or a dead one is being cleared.
 *
 * Called on both edges deliberately. `nativeInstallRunner` marks active BEFORE
 * the first event arrives (the engine's preflight takes a few seconds, and a
 * chip that reads "Stopped" for those seconds then jumps is worse than one that
 * commits immediately), and its IPC-rejection path clears it -- a store left
 * active forever would pin the chip to a stage that is no longer running until
 * the app restarts.
 */
export function setInstallActive(active: boolean): void {
  apply(active ? { ...emptyProgress(), active: true, nowMs: Date.now() } : emptyProgress());
  syncTicker();
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

/** `271` → `4:31`; `3871` → `1:04:31`. Seconds in, clock out. */
export function formatElapsed(totalSecs: number): string {
  const s = Math.max(0, Math.floor(totalSecs));
  const hh = Math.floor(s / 3600);
  const mm = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return hh > 0 ? `${hh}:${pad(mm)}:${pad(ss)}` : `${mm}:${pad(ss)}`;
}

/** How long the current stage has been running, in seconds, or null. */
export function elapsedSecs(p: InstallProgress): number | null {
  if (p.stageStartedAt === null) return null;
  return Math.max(0, (p.nowMs - p.stageStartedAt) / 1000);
}

/**
 * The label for a progress state, e.g. `Building… 62%`.
 *
 * Three shapes, and which one appears is a claim about what is known:
 *
 *  * `Building… 62%` — a real denominator exists (ninja steps, git objects,
 *    containers started).
 *  * `Waiting for the world… 4:31` — a bounded wait. Elapsed time, never a
 *    percentage, because the number would measure the clock and not the work.
 *  * `Downloading AzerothCore…` — work with no honest number yet.
 */
export function installStatusText(p: InstallProgress): string {
  const base = (p.stage && STAGE_COPY[p.stage]) || "Installing…";
  if (p.stage === READY_STAGE) {
    const secs = elapsedSecs(p);
    return secs === null ? base : `${base.replace(/…$/, "")}… ${formatElapsed(secs)}`;
  }
  return p.pct === null ? base : `${base.replace(/…$/, "")}… ${p.pct}%`;
}

/**
 * The second line Home's card shows under the headline, or null when the stage
 * has nothing extra worth saying.
 */
export function installDetailText(p: InstallProgress): string | null {
  if (p.stage === READY_STAGE && p.limitSecs !== null) {
    const secs = elapsedSecs(p);
    if (secs === null) return null;
    return `First boot imports the world database. Waited ${formatElapsed(secs)} of up to ${formatElapsed(p.limitSecs)}.`;
  }
  return null;
}
