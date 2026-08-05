import { describe, it, expect } from 'vitest';
import { exitCopy, EXIT_STOP_FAILED_NOTE } from './exit-guard.svelte';

describe('exitCopy', () => {
  it('says plainly that closing stops the server', () => {
    const c = exitCopy('prompt_running');
    expect(c.title).toMatch(/running/i);
    expect(c.body).toMatch(/stop/i);
    expect(c.confirm).toBe('Stop server and close');
    expect(c.cancel).toBe('Cancel');
  });

  it('admits uncertainty rather than claiming a running server', () => {
    const c = exitCopy('prompt_unknown');
    // The honest wording matters: asserting a running server we could not
    // confirm is the same overclaiming the tri-state exists to prevent.
    expect(c.body).toMatch(/could ?n[o']t confirm/i);
    expect(c.body).toMatch(/may stop/i);
    // FIX ROUND 1 (2026-08-05): the two assertions above only check that the
    // hedge is PRESENT -- proved live that they stay green even when the copy
    // ALSO asserts the server is running as settled fact elsewhere (e.g. a
    // body starting "Your server is running. Couldn't confirm the exact
    // port, but..." satisfies both). Assert the overclaim is ABSENT too.
    // Matched by sentence position, not a flat substring ban: the correct
    // body legitimately contains the words "is running" -- inside the hedge
    // "whether your server is running" -- so banning that phrase outright
    // would also reject the honest copy.
    const bareRunningClaim = /(^|[.!?]\s+)(your |the )?server (is|isn'?t|is not) running\b/i;
    expect(c.title).not.toMatch(bareRunningClaim);
    expect(c.body).not.toMatch(bareRunningClaim);
  });

  it('never shouts an error word — this is a routine choice, not a failure', () => {
    for (const k of ['prompt_running', 'prompt_unknown'] as const) {
      const all = Object.values(exitCopy(k)).join(' ');
      expect(all).not.toMatch(/error|failed|fatal|warning/i);
    }
  });
});

/**
 * M8 (final review 2026-08-05). The failed-stop note used to read "The stop
 * reported a problem. The launcher is still closing." — true when written,
 * false the moment C1 made a failed stop KEEP the launcher open. All three
 * refuters found it independently, and both mutations were decisive: changed
 * to "XXX" and then to "", the full 784-test suite stayed green both times.
 *
 * A user told the launcher is leaving, watching it stay, with no visible
 * escape hatch (M7), reaches for Task Manager — a hard kill that skips
 * `RunEvent::Exit` and delivers the exact cut this plan exists to prevent.
 */
describe('the failed-stop note', () => {
  it('exists and is not empty', () => {
    expect(EXIT_STOP_FAILED_NOTE.trim().length, 'the note was emptied').toBeGreaterThan(20);
  });

  it('says the launcher is STAYING, never that it is closing', () => {
    expect(
      EXIT_STOP_FAILED_NOTE,
      'the launcher stays open on a failed stop (C1) — telling the user it is closing is ' +
        'what sends them to Task Manager when it does not'
    ).not.toMatch(/\b(still closing|is closing|closing anyway|will close|shutting down)\b/i);
    expect(EXIT_STOP_FAILED_NOTE).toMatch(/staying open|still open|remains open/i);
  });

  it('names the failure and the way out', () => {
    // Three facts, three decisions: the stop failed, the launcher is not
    // leaving, and there is a way to leave regardless.
    expect(EXIT_STOP_FAILED_NOTE, 'the note must say the stop failed').toMatch(
      /problem|failed|did ?n[o']t/i
    );
    expect(
      EXIT_STOP_FAILED_NOTE,
      'the note must point at the escape hatch — it is the only control that reliably ' +
        'works once the stop has failed'
    ).toMatch(/close anyway/i);
    expect(
      EXIT_STOP_FAILED_NOTE,
      'the note must not claim the server is down; the stop is exactly what failed'
    ).toMatch(/may still be running|might still be running/i);
  });
});
