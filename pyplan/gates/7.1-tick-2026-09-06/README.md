# The citation pass 7.1's tick cost — 2026-09-06, on `yulon-fedora`

7.1 was ticked on 2026-09-06 on an owner decision (`pyplan/phase7-decisions.md`, **Appendix E**;
the tick record is the first bullet under the 7.1 line in `pyplan/checklist.md`). This folder is the
half of that tick a reader can re-derive: what `tests/test_docs_pins.py` said before the box was
ticked, what the pass changed on `pyplan/phase7-plans/7.1-spine-azerothcore-linux.md`, and what the
whole gate said afterwards.

Everything here was produced on **2026-09-06** against branch `lane/b71`, whose base is
**`2b6a9c6b`** (the merge of lane 710 into `yulon-phase7`). The test box was **`yulon-fedora`**,
Python **3.13.15**, pytest **9.1.1**, driven by `run-tests-vm.sh`, which syncs the branch plus the
uncommitted `pylauncher/` and `pyplan/` edits.

## Why a pass was owed at all

`test_docs_pins.py::test_every_test_these_pages_name_by_hand_actually_exists` reads
`checklist.md` and widens to every `phase7-plans/` page whose phase line is `- [x]`. A plan cites
tests it intends a future task to write, so a dead citation there is a forward reference while the
phase is open and a claim about the tree once it closes — **the tick is what flips it**. The cost
had been measured twice before this lane and came out at 13 of 141 both times: once on 2026-09-02 at
`f6ed1b9a` (recorded in the guard's own docstring, which does not name the box) and once on
2026-09-04 on `m910q` (recorded on the 7.1 line in `checklist.md`). It was measured a third time
here, because a number carried forward is not a measurement.

## The files

| file | what it is |
|---|---|
| `before-docspins.txt` | `YULON_TEST_BOX=yulon-fedora bash run-tests-vm.sh tests/test_docs_pins.py -v` with the 7.1 line temporarily reading `- [x] 7.1 ` and **nothing else changed** — the runner's own `==> overlaying 1 uncommitted file(s)` line says so. **1 failed, 3 passed in 0.49s**; the failure names this page and thirteen names. |
| `after-docspins.txt` | the same command after the pass, with the tick kept: **4 passed in 0.24s**. |
| `widening.txt` | the guard's own `_plans_whose_phase_the_checklist_ticks()` and `_cited_as_live()`, run on `yulon-fedora` in the checkout `run-tests-vm.sh` had just synced. Four passing tests look the same whether the guard widened or not, so the widening was asked for rather than assumed: it answered `['7.1-spine-azerothcore-linux.md', '7.2-retire-bash.md', '7.3-cmangos-family.md']`, and over those three **135/0**, **44/0** and **219/0** (live / unresolved). |
| `citations-before.txt` | the thirteen names and every line they sat on, at `2b6a9c6b`. |
| `citations-after.txt` | the same rule run over both sides: **13** names no longer counted live, **7** newly counted and all seven resolve. |
| `citation-scan.py` | the script those two came from — a copy of the guard's rule that can be pointed at a git revision, which pytest cannot. Its own docstring says it is a copy and what that costs. |
| `checks-yulon-fedora.txt` | `run-tests-vm.sh --checks` over the pass's tree, before this folder existed: **2792 passed, 4 skipped in 27.01s**, mypy ×3 `Success: no issues found in 72 source files` (this platform / win32 / darwin), ruff `All checks passed!`, black `140 files would be left unchanged`, `=== --checks: ALL GREEN ===`, exit 0. |
| `checks-yulon-fedora-final.txt` | the same gate run again over the finished tree — its own header line reads `==> syncing lane/b71 (ec77b8ef)` plus one overlaid uncommitted file, the plan page carrying the `git diff --numstat` paragraph: **2792 passed, 4 skipped in 20.73s**, the same three mypy passes, ruff, black `140 files`, `ALL GREEN`, exit 0. What that run did **not** contain is this row, the row above it and `checks-yulon-fedora-final.txt` itself, all three of which live under `pyplan/gates/` where nothing `--checks` runs reads them. |

**One thing to know about `before-docspins.txt` and `after-docspins.txt`:** the pytest header in them
names the per-run scratch checkout `run-tests-vm.sh` makes on the box. That path is not a place to
cite anything from — it is deleted and remade per run — and it is left in only because these are raw
transcripts and editing evidence to tidy a path is how evidence stops being evidence.

## The thirteen, before and after

Before the pass the page presented **141** test names as live and **13** of them resolved to nothing
under `pylauncher/tests/`, across **37** sites (8 inside fenced code blocks — six `def` signature
lines and two docstring lines — and 29 outside). After it: **135** live, **0** unresolved.

Each fate below was re-derived on 2026-09-06 with `git log -S'<name>' -- pylauncher/tests` and
`git grep -c 'def <name>' <sha>^` / `<sha>`, so each names the commit that removed it rather than
saying it "is gone". The full argument, and the successor test for each, is the dated `CITATIONS`
section now at the top of `pyplan/phase7-plans/7.1-spine-azerothcore-linux.md`.

| name | sites | what became of it |
|---|---|---|
| `test_native` (the module) | 20 | `tests/test_native.py` `git mv`'d to `tests/test_families_azerothcore.py` at **`95dcb9a3`** (2026-08-31) |
| `test_the_platform_decides_which_engine_installs_and_linux_keeps_the_script` | 3 | deleted at `95dcb9a3` |
| `test_script_platforms_defaults_to_platforms_so_old_entries_mean_what_they_said` | 3 | deleted at `95dcb9a3` |
| `test_the_entry_names_its_family_and_a_scriptless_entry_still_reads_as_scripted` | 1 | written at `95dcb9a3`, deleted at **`b9538286`** (2026-09-02) |
| `test_dispatch_until_a3_wotlk_without_script_platforms_reads_as_scripted_everywhere` | 2 | never written — `git log -S` finds no commit that ever added it |
| `test_wotlk_names_no_script_platform_but_still_ships_its_scripts_until_7_2` | 1 | added at `6ec8ee94`, deleted at `b9538286` — which is what A15 on the plan said would happen |
| `test_script_variant_keys_must_be_known_package_managers` | 2 | deleted at `b9538286`, with the `script_variants` field itself |
| `test_the_three_cmangos_entries_have_no_native_block_yet` | 1 | deleted at **`d51bd307`** (2026-09-01), the commit that made its claim false |
| `test_the_native_engine_never_hands_a_prompter_to_provisioning` | 5 | deleted at **`fd84a1fe`** (2026-08-31), which is the plan's own A.4 flip |
| `test_keep_awake_is_a_no_op_on_linux` | 1 | deleted at **`221b38ec`** (2026-08-31); the plan's line said only "is replaced" |
| `test_for_wotlk_takes_its_import_gate_and_password_from_install_wiring` | 2 | never written under this name; the `_and_password`-less one was written at `377f17da` |
| `test_declining_does_not_promise` | 1 | never a citation — a truncated `grep` argument; the prefix was extended to the whole name, which resolves |
| `test_the_fixed_password_` | 1 | the same shape: the page spelled the name with a trailing ellipsis and the guard read the stem |

**Nothing was repointed at a different live test to make the guard quiet.** Every name that stopped
being counted stopped because its own line now says what became of it, and not one of the 128 names
that were both live and resolving before the pass stopped being checked — that is what
`citations-after.txt`'s two lists are for.

## What this folder does NOT evidence

* **Clause 15 of the Ubuntu gate line.** It was graded MET by lane `b39`, whose record is
  `pyplan/gates/bug39-lan-press-2026-09-05/` on branch `lane/b39` at **`8f9de57f`** — a commit on
  top of `cfb4c04f` that was **not merged into `yulon-phase7`** when this folder was written. The
  7.1 tick record cites it by path and line and says the same thing there.
* **Any machine work on Fedora or Arch.** Nothing was installed, built or pressed for this tick. The
  Fedora/Arch sub-gate was re-scoped by the owner onto the evidence that already existed
  (`pyplan/gates/7.1-fedora44-*.log`, `pyplan/gates/7.1-arch/71-arch-appimage.log`), and the three
  items that re-scope drops are named on that line under CARRIED. `yulon-fedora` was used here only
  as the test box for pytest, mypy, ruff and black.
