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
  });

  it('never shouts an error word — this is a routine choice, not a failure', () => {
    for (const k of ['prompt_running', 'prompt_unknown'] as const) {
      const all = Object.values(exitCopy(k)).join(' ');
      expect(all).not.toMatch(/error|failed|fatal|warning/i);
    }
  });
});
