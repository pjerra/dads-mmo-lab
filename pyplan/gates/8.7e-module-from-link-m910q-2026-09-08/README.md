# 8.7e — a module from a link or a folder, lane C — m910q, 2026-09-08

Lane C of `pyplan/phase8-designs/module-from-link-or-folder.md`, pressed on **m910q**
(`PK-M910q`, Linux 6.8.0-138-generic x86_64) against commit **`13fd052d`**.

**There is no `8.7e` box in `pyplan/checklist.md`.** The design says so itself, and this folder
does not invent one: the number, the definition of done and the gate line are the owner's session's
to write. The folder is named `8.7e-…` only so it sorts beside 8.7a–8.7d; the task's own file-set
line called it `module-from-link-wotlk-yulon-ubuntu-<date>`, and the live instruction that followed
named this one. Both are recorded rather than silently reconciled.

---

## What was NOT proved, first, because it is most of the feature

**Lanes A and B are on no branch.** Grepped across every remote ref on 2026-09-08:
`pylauncher/yulon/module_source.py` does not exist on `origin/yulon-phase8b`, `origin/yulon-phase8`,
`origin/Yulon`, `origin/main` or any `pr/*`; `apply.Applier.install` has no `folder=`/`complete=`
keywords; `manifest.Origin` and `ManifestStore(user_root=…)` do not exist. So on this branch **this
app cannot clone or copy a module from a user-supplied source at all**, and none of the following
was pressed here:

- no repository was cloned from a pasted link;
- no folder was copied into a `modules/` directory;
- no `conf/*.conf.dist` was discovered or activated, and no SQL step was discovered or reported;
- no manifest was persisted under `~/.local/share/yulon/manifests/user/`, so **the
  restart-persistence clause was not pressed** — the ground read below shows that directory does
  not exist, and it still does not;
- no Remove forgot a record on disk;
- **no rebuild**, which the design already excludes from every version of this gate (owner rule).
  The press that would settle whether a cloned module compiles and whether the running server reads
  its conf is, from the app: *Modules tab → "Rebuild the server…" → Yes*, on a box holding a WotLK
  install. Nothing here assumes its outcome.

**And the box could not have hosted those clauses anyway.** `0-ground.txt` records it: the WotLK
install `state.json` remembers at `/home/pk/games/wow-server-playerbots` **is not on disk**, and
there is no `modules/` directory anywhere under `~` except Tortoise's own source tree. The design's
live section assumed `yulon-ubuntu`'s AzerothCore at `/home/pk/wowserver`; m910q is a different
tree and was measured rather than assumed.

**The gate is RED**, on exactly one test, and that is a real finding rather than an accident — see
"The design gap" below.

---

## Ground, read before the first press

`0-ground.txt`, captured 2026-09-08 20:54:27 UTC, before anything was built or shown:

| read | value |
|---|---|
| commit under test | `13fd052d2f0c765e1de104037a38c34b5400b96f` |
| containers up | `r6` (`refute5img:latest`, up 3 days) — **not ours**; untouched |
| remembered WotLK install | `/home/pk/games/wow-server-playerbots` — **`No such file or directory`** |
| `modules/` under `~` | only `/home/pk/tortoise-server/src/tortoise-wow/modules` (another game's source) |
| user manifest layer | `~/.local/share/yulon/manifests` — **`No such file or directory`** |

`1-modules-tab-before.txt` is the second half of the ground, and it is the one that makes the frame
below mean anything: at **`13fd052d^`** the view has **no** `module_link_button` and **no**
`module_folder_button`, and `ControllerServices` has none of the four fields. The "before" frame was
taken from a tree extracted at that parent commit (`git archive 13fd052d^`), in its own process,
**before** the after-frames' process started — not re-labelled afterwards, which is the 8.2d false
artefact this gate was told not to repeat.

---

## Frames

Every frame logs the capturing process's PID and its liveness at the moment of the grab, in
`alive.txt`; all seven read `alive`. `capture.py` and `before_capture.py` are the scripts, run with
`QT_QPA_PLATFORM=offscreen` on the box's own venv.

| frame | what it shows | driven by |
|---|---|---|
| `1-modules-tab-before.png` | the Modules tab at `13fd052d^`: four buttons, none of them these | **real** `ControllerServices.for_entry()` |
| `2-modules-tab-buttons-dead.png` | the same tab at `13fd052d`: "Install from link…" and "Install from folder…" present and **greyed**, beside four live buttons | **real** `for_entry()` |
| `3-dead-buttons.txt` | their labels, `enabled=False`, and the no-route tooltip, read off the widgets | **real** |
| `4-link-dialog.png` + `.txt` | the dialog `ask_module_link` really opens, photographed **mid-modal** (a `QTimer` grabbed `QApplication.activeModalWidget()` and rejected it) and the `None` that rejection returned | **real** default asker |
| `5-buttons-live-when-wired.png` | the same two buttons, enabled, once a route is present | fake route |
| `6-link-report-and-custom-row.png` | the link press: the report `_format_report()` printed, and the list row `[module] mod-eluna — Custom module (cloned from a URL you provided).` selected and scrolled into view | **fake** route |
| `7-folder-report-and-custom-row.png` | the folder press and `[module] mod-hand-made — Custom module (copied from a folder you provided).` | **fake** route |
| `8-refused-not-mod.png` | a refusal shown verbatim and alone, with the list unchanged (21 modules, no custom row) and both buttons still live | **fake** route |
| `seams.json` | `for_entry()`'s answer on this box: the four new seams `None`, `store`/`applier`/`module_updates`/`module_sql` bound | **real** |
| `fake-route-calls.json` | what the fake route was actually asked: two derives, two installs (`mod-eluna` with `folder=None`, `mod-hand-made` with the path), and **zero** installs on the refusing route | fake route |

**Frames 5–8 photograph the VIEW, not an install.** They are the seven unit tests' own fakes
(`tests.test_controller_view._with_custom_route`) rendered at 1100×760, so what they prove is that
the tab draws, reports and re-lists correctly when a route exists — and nothing whatsoever about
cloning, copying, conf or SQL. Anyone reading them as an install would be reading a claim this
folder does not make.

**Frames 1–4 are the shipped wiring** and do carry a live clause of their own, which is the only one
lane C can close today:

> On a real Linux box, `ControllerServices.for_entry("wow-wotlk", …)` builds a tab whose two
> custom-module buttons are present, labelled, tooltipped with the reason, and **dead** — and the
> dialog behind the link button opens for real and answers `None` when dismissed.

The ground makes that non-vacuous: at the parent commit the buttons did not exist, so the assertion
was **false** before the press and true after it.

---

## The design gap: `tests/test_controller_packages_agree.py`

`--checks` on **yulon-fedora** and again on **m910q** ends
`=== --checks: RED (see !! lines above) ===` with `1 failed, 3629 passed, 6 skipped`, mypy ×3 green,
ruff green, black green. The one failure, identical on both boxes:

```
FAILED tests/test_controller_packages_agree.py::
       test_every_game_offers_the_whole_controller_surface_wotlk_does
AssertionError: wow-tbc is missing ['module_forget', 'module_from_folder',
  'module_from_link', 'module_install_custom'], which is not the module surface
  and so is a real gap against wow-wotlk
```

That test enumerates `fields(ControllerServices)` **dynamically** — "so a sixteenth capability added
to the view cannot be wired for WotLK and forgotten for the rest" — and requires every `None` seam
to be excused by a fact read off the catalog entry. Four seams that are `None` on the **reference**
game trip it, and the tripwire is right: on this branch WotLK genuinely cannot install a custom
module, because lane A is not there to wire it.

That file is in **no lane's file set** — the design assigns lane C only
`ui/controller_view.py`, `tests/test_controller_view.py` and this folder — and every earlier box
that added a `ControllerServices` field (8.1a's `dashboard`, 8.2a's `channel_setup`, 8.3a's
`accounts`, 8.4a's `play`, 8.5a's `bots`, 8.7a's `module_sql`/`module_updates`, 8.9a's `uninstall`)
added its self-closing clause there. **It was not touched**, per the lane's instruction. The clause
it needs is one more self-closing exception in the same shape as the others — something the four
seams can be read off, once lane A exists to decide it.

**What was deliberately NOT done to make the gate green:** wrapping the four seams in a non-`None`
container object so the tripwire never sees them. That would pass by construction while the thing it
guards is still broken, which is the failure the record already names twice ("guards that prove
declarations", "an incomplete artifact reads as fact").

---

## Deviation D1, recorded rather than quiet

The design (§3.3, §3.5) has this view call

```python
applier.install(m0, None, folder=FolderSource(path, copier), complete=services.module_complete)
```

with `module_copy_folder` and `module_complete` as view services. This lane instead puts the whole
call behind **one** seam, `CustomModuleInstall = Callable[[Manifest, Path | None], ApplyReport]`,
wired from `controller_<acronym>/modules.py` — the file whose job is already "binding the shared
applier to that game". Two reasons, in order of weight:

1. `apply.FolderSource` is lane B's and does not exist, so the design's call does not type-check on
   any tree that lacks lane B — which is every tree today.
2. A view that constructs `FolderSource` and hands the applier a copier knows one thing more about
   the apply engine than `ui/*_view.py` is allowed to (style-guide §3: *delegate*). Under D1 the
   view hands over only what the view alone knows — which manifest, and which folder the user chose.

`module_copy_folder` and `module_complete` therefore do not appear on `ControllerServices`; they live
on the far side of that seam, in lane A's binding. The design's test
`test_the_folder_button_hands_the_applier_a_folder_source_and_the_copier` is shipped as
`test_the_folder_button_hands_the_install_route_the_folder_it_was_given`, asserting the same
mutation (the folder not carried into the install call) at the seam that exists.

## Left as found

`r6` still up (never touched); no server started or stopped; nothing installed, cloned or copied;
`/tmp/gate87e-serverdir`, `/tmp/before87e` and `~/gate87e/` are the only things written on the box,
all outside any install. `~/.local/share/yulon/` is unchanged — `state.json` still names the same two
installs and `manifests/` still does not exist.
