export type ExitPrompt = 'prompt_running' | 'prompt_unknown';

export interface ExitCopy {
  title: string;
  body: string;
  confirm: string;
  cancel: string;
}

/** Pure, so the wording is testable without mounting anything. */
export function exitCopy(kind: ExitPrompt): ExitCopy {
  const confirm = 'Stop server and close';
  const cancel = 'Cancel';
  if (kind === 'prompt_running') {
    return {
      title: 'Your server is running',
      body: 'Closing DML Launcher will stop it. Windows shuts the WSL distro down shortly after the launcher exits, so the server cannot keep running without it.',
      confirm,
      cancel
    };
  }
  return {
    title: 'Your server may be running',
    body: "Couldn't confirm whether your server is running. Closing DML Launcher may stop it, so it will be stopped cleanly first.",
    confirm,
    cancel
  };
}

/**
 * The one sentence the user reads when a confirmed stop FAILS.
 *
 * FIX ROUND 3 (2026-08-05) — M8. This used to read "The stop reported a
 * problem. The launcher is still closing." It was true when it was written and
 * false the moment C1 landed: `exit_stop_and_close` now stays up on a failure,
 * which is the whole point of that fix. A user told the launcher is leaving,
 * watching it stay, reaches for Task Manager — a hard kill that skips
 * `RunEvent::Exit` entirely and delivers the exact cut this plan exists to
 * prevent. This repo has twice recorded stale prose as actively harmful.
 *
 * A CONSTANT, not an inline literal, for the other half of M8: the string was
 * deletable. Changed to "XXX" and then to "", the full suite stayed at 784
 * passed both times. A shared constant is something a test can name.
 *
 * It has to say three things, because each one is a decision the user now has
 * to make: the stop failed, the launcher is NOT leaving, and there is a way
 * out anyway.
 */
export const EXIT_STOP_FAILED_NOTE =
  'The stop reported a problem, so the launcher is staying open and your server may still be running. Try again, or use Close anyway to leave regardless.';

/**
 * Module-level so it survives navigation, mirroring restart-state.svelte.ts.
 *
 * `failed` exists because `busy` could not carry this (M7): the failure arm
 * clears `busy` — correctly, the run is over — and the "Close anyway" button
 * was gated on `busy` alone, so the escape hatch disappeared at the exact
 * moment it became the only useful control. Spec line 117 says "if the stop
 * fails, report the failure and offer to close anyway"; before C1 that was
 * moot, and C1 made it violated.
 */
export const exitGuard = $state<{
  open: boolean;
  kind: ExitPrompt;
  busy: boolean;
  failed: boolean;
  note: string;
}>({
  open: false,
  kind: 'prompt_running',
  busy: false,
  failed: false,
  note: ''
});
