# T47 — install resets an app-owned clone without asking what update now asks

**Status:** FILED — T44's open finding 2.
**Filed:** 2026-09-13 by the lead. Pre-existing, and made *visible* by T44 rather than caused
by it: T44 gave `update()` three refusals, and `install()` over the same folder still has none.
**Branch:** `fix/t44-open-findings`.

## The asymmetry

`Applier._require_own_clone()` (`yulon/apply.py:1580`) guards both destructive paths. Its
case 3 reads:

> 3. A checkout this app's own claim vouches for: allowed, and this is the ONLY way through.

"Allowed" means the path goes to the clone seam, which runs `git fetch` + `git reset --hard
FETCH_HEAD`. The claim answers *whose folder is this* — it does not answer *is there
anything in it worth keeping*. `_update_refusal()` (`apply.py:1313`) asks exactly that, in
three questions T44 added:

| question | `update()` | `install()` over the same clone |
|---|---|---|
| uncommitted changes in the checkout? | refuses, names the folder | **resets over them** |
| commits of its own the remote lacks? | refuses, says reflog | **moves off them** |
| could git/the remote not be reached? | refuses rather than guess | **proceeds** |

## The route that reaches it

A module installed through Yu'lon carries the app's claim. A user who then edits a file in
`modules/<id>` — patching a bug, adding a print, cherry-picking a fix — and presses Install
again loses it silently. The same folder, one button over, refuses to do this.

T44 left it out deliberately and said so: *"Pre-existing, unreachable from the row button, and
now asymmetric with `update()`. It should ask the same three questions."* Unreachable **from
the row button** is not unreachable — the custom-module route reaches it.

## What is NOT in reach

Case 0 (nothing at the path) stays as it is: a first install has nothing to lose and must not
grow a prompt. The three questions are asked only where a claimed checkout already exists.

## Definition of done

1. **The three questions are shared, not copied.** `_update_refusal()`'s body becomes
   reachable from both paths — one function, two callers — so the two can never drift again.
   A test asserts they refuse on the same three facts.
2. **Install over a claimed clone refuses with the same sentences**, naming the folder and
   ending "Nothing was changed."
3. **A first install is untouched** — asserted, because the easy mistake here is a guard that
   makes the ordinary case ask a question.

## Evidence the ticket owes

The gate's last line, a named mutation per test, and one test that drives the real
`install()` seam rather than `_update_refusal()` directly — the standing lesson is that a
fix proven on the function and not on the call site has proven nothing
(`reviews-check-functions-not-call-sites`).
