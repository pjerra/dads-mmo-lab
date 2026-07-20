import { invoke } from "@tauri-apps/api/core";

// Batch 4 (progress & empty states): taskbar progress cue for long streamed
// ops. Flips the OS taskbar button to an indeterminate "busy" state when the
// first long op starts and clears it once the LAST overlapping op finishes --
// so a minimized launcher still shows work is in flight (rebuild / flush /
// server-update / restart / backup).
//
// Best-effort: every failure (older shell, non-Tauri host, unsupported
// platform) is swallowed -- a cosmetic hint must never break the op it
// decorates.
//
// A depth counter (not a bare boolean) keeps the cue correct when two long
// ops overlap: `taskbarBusy()` only turns the cue ON for the first op, and
// `taskbarIdle()` only turns it OFF once every op has reported done. Callers
// pair them in try/finally so the count can't leak.

let active = 0;

async function apply(on: boolean): Promise<void> {
  try {
    await invoke("set_taskbar_progress", { active: on });
  } catch {
    // Cosmetic only -- ignore (e.g. running outside Tauri, or an older build
    // without the command).
  }
}

/** Enter a long op: turn the taskbar cue on (idempotent while nested). */
export function taskbarBusy(): void {
  active += 1;
  if (active === 1) void apply(true);
}

/** Leave a long op: clear the cue once the last overlapping op is done. */
export function taskbarIdle(): void {
  if (active === 0) return; // unbalanced idle -- nothing to clear
  active -= 1;
  if (active === 0) void apply(false);
}
