import { describe, it, expect } from 'vitest';
import { exitCopy } from './exit-guard.svelte';

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
