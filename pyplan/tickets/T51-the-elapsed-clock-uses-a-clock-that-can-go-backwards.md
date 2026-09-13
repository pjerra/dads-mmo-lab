# T51 — the elapsed clock is measured with a clock that can move backwards

**Status:** FILED — not fixed here. `yulon/ui/widgets/log_panel.py` is upstream's and every
fork ticket forbids editing it, so this is a report, not a patch.
**Filed:** 2026-09-13 by the lead, from a gate failure on the `0.8.67-fixtest` release tree.

## What happened

```
FAILED tests/test_log_panel.py::test_the_elapsed_clock_counts_from_this_run_and_not_from_the_last_one
  AssertionError: the second run kept the first run's zero, so its elapsed clock is wrong
  assert (1789323322.6352537 is not None and 1789323322.6352537 > 1789323323.6829975)
```

**The message is wrong about its own failure**, which is what makes this worth writing down.
It says the second run "kept the first run's zero" — that would make the two numbers *equal*.
They are not: the second stamp is **1.05 s EARLIER** than the first, with a `time.sleep()`
between them. Nothing in the code can produce that.

## The cause

Both sides measure with the wall clock:

```python
# yulon/ui/widgets/log_panel.py:678
self._started_at = time.time()
# :816
self._elapsed_label.setText(_elapsed(time.time() - self._started_at))
```

`time.time()` is not monotonic. An NTP correction or a WSL2 clock resync after the host
suspends moves it backwards, and both the stamp and the assertion move with it. This session
had been running for many hours when it fired.

Ruled out, in this order:

- **Not the change under test.** The release tree merges `fix/windows-remove-readonly-git` and
  `fix/build-log-kept-by-docker-desktop`; `git diff upstream/Yulon HEAD` over `log_panel.py`
  and `test_log_panel.py` is **empty**.
- **Not a broken branch.** Both branches gated green alone (`4369 passed` each).
- **Not deterministic.** 5/5 green in isolation, and the next full run was `4371 passed`.

## What is actually wrong, beyond the flake

An elapsed-time field answers "how long has this run been going", which is a *duration*.
`time.monotonic()` is the clock for durations and cannot go backwards. Today a clock
correction does not merely fail a test — it makes the running app's elapsed field jump, and on
a backwards correction `time.time() - self._started_at` goes **negative**, so a user watching
a two-hour install can be shown a negative or wildly wrong elapsed time at the moment they
most want to trust it.

The test is honest about the risk in its own docstring — *"worse than no field, because it
reads as a measurement"* — and then measures with the wrong clock.

## Suggested fix, for whoever owns the file

`self._started_at = time.monotonic()` and the same in the tick, keeping `time.time()` only if
something needs a wall-clock timestamp to display. The test's `first_start` comparison then
becomes sound rather than probabilistic. One line each, and the assertion message should say
what it actually checks — "the second run did not re-stamp its zero" — rather than asserting
an ordering it cannot guarantee.
