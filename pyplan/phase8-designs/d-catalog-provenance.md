# Per-field provenance for the catalog's per-tree values

Design, and the guard that ships with it. Written 2026-09-08 against `dcc64543`.

This is the third of the four guards the 2026-09-08 audit found scoped so they
could not fail. The other three are code; this one was a design question first,
because the thing it guards had no representation at all.

## The ground, read before anything was written

Phase 8's exit line (`pyplan/checklist.md:2508`) asks, among six clauses, for

> every value still marked unverified in the catalog's operations block
> replaced by a measured one, **and that set enumerated by name in the catalog
> test rather than left to memory**

At `dcc64543`, measured rather than assumed:

* `git grep -i provenance dcc64543 -- pylauncher` returns **13 hits in 8
  files**, and no catalog TEST is among them. Twelve are a different subject
  entirely — compose-stack provenance in `families/dockerfile.py`,
  `support_compose.py` and their tests, and adoption of a folder of unknown
  provenance in `installer.py`.
* **The thirteenth is this design already written out by hand, once, in prose.**
  `AzerothCoreData.world_env`'s description (`yulon/catalog/catalog.py:202`)
  reads "PROVENANCE: WotLK carried 1600/2000, **copied from the ONE proven
  yulon-ubuntu install** … Never measured on another machine and never measured
  at all for RAM … Still owed an RSS reading by the first gate." That is an
  `inherited` kind, a citation and an owed debt, in one field, in English,
  reachable by no test. One field out of the catalog's hundreds happens to have
  it; the design below is that sentence made uniform and machine-checkable.
* the string `unverified` appears **nowhere** in `yulon/catalog/` — not in the
  data, not in a model, not in a description. So the exit line's "still marked
  unverified" names a marker **that does not exist**, and the set it asks to be
  enumerated could not be computed by anything.
* `catalog.json` writes **39** values about a specific tree under a game's
  `play` or `accounts` block, and not one of them carried a marker of any kind.

So there is nothing to widen here. There was no guard.

## What was actually wrong

"Measured" was real, and it lived in the model docstrings. The problem is the
shape of that record, not its honesty: **a docstring is written per FIELD and
the values are per field per GAME.** `Equipped`'s docstring
(`yulon/catalog/catalog.py:1021`) says the flat shape was "measured on the TBC
install 2026-09-06 and on the Vanilla one 2026-09-07" — four trees' worth of
values, two trees' worth of measurements, one sentence, and no way to ask a
given value which of the four it belongs to.

Three things then read identically in the file:

* a value this tree's own running server was asked for,
* a value read off this tree's own source and never asked,
* a value copied from a sibling emulator.

The third is the expensive one, and it is expensive precisely because it looks
correct. `per-tree facts are measured per tree, never inherited from a sibling
emulator` is a rule this project has already paid a night for. 8.4c is the
worked example: it found `mail_item_cap: 1` sitting in the catalog as what its
own README calls "a **source-cited prediction** — this tree's own
`src/game/Mails/Mail.h:49` … A prediction the server has never been asked is not
a measurement."

## The marker

Three kinds. The vocabulary is 8.4c's own, generalised rather than invented:

| kind | means | citation | checkable here |
| --- | --- | --- | --- |
| `measured-on` | this tree's own running install was ASKED, and this is its answer | a page under `pyplan/`, and it must resolve | yes — the file is opened |
| `read-from-source` | read off this tree's own source at a named line. A prediction | `path:line` or `path:first-last` | shape only — see below |
| `inherited` | carried over from a sibling tree; nobody asked this one | the game id it came from | yes — the source must exist and must itself be non-inherited |

What makes a `measured-on` a measurement is the **live server**, not the
transport: console, SOAP and SQL all count, and `SHOW TABLES LIKE
'account_access'` coming back empty on 8.3c's install is as much an answer as
the console's own sentence.

`read-from-source` is checked for shape and not for content, and that limit is
stated rather than hidden: the emulator trees are cloned by an install and are
not vendored in this repo, so there is no file to open. The shape rule catches a
row holding English where a citation belongs, which is how this field would
actually rot, and it catches nothing about whether the line says what the row
claims.

**A fourth kind was considered and refused.** "Measured, but the reading is not
in this tree" would fit `wow-tortoise:accounts.scheme` exactly. It is a debt
below instead, because a kind meaning *trust the docstring* is the state this
whole design replaces.

## Where it lives, and why not in `catalog.json`

In `pylauncher/tests/catalog_provenance.py`, beside `write_sites.py`, and by
that module's precedent: `write_sites.py` holds the ledger's rules while
`pyplan/write-ledger.md` holds the table a person reads.

Three reasons, in the order they decided it.

1. **The exit line already put it there.** "…enumerated by name in the catalog
   TEST rather than left to memory." The clause names the location.
2. Provenance is a development-time fact. Nothing the app does at runtime reads
   it, and the shipped catalog would carry ~4 KB of gate-folder paths to no
   runtime end.
3. Drift is the obvious objection to keeping the fact away from the value, and
   the guard closes it in both directions in the same assertion: a value with no
   row is red, and a row naming a value the catalog no longer writes is red.

**The road not taken, named rather than dropped:** a `provenance` field on the
`Play` and `Accounts` models would put the marker in the file beside the value
and let the UI say *"this tree was never asked"* on a control it draws — which
is a real safety feature and a real Phase 9 question. It is not built, because
nobody asked for a surface and building one on the way past is how scope moves.

## The scoping rule: presence in the file

**Provenance is owed by every leaf `catalog.json` writes under a game's `play`
or `accounts` block.** A key written down is somebody's claim about this tree —
it was typed, per game, by a person with a reason. A key left out takes the
model's default, which is a claim about the SHAPE and is owned by that field's
own docstring in `catalog.py`.

The rule is computable, it needs no list of field names to stay in step with the
models, and a new field in either block goes red on arrival.

**Its hole, named now rather than discovered later: an absent key can also be a
per-tree claim.** Two live examples, both real today:

* `play.revive_offline` is null on `wow-wotlk` and `wow-tbc` *by being absent*,
  and that null is what withholds the button. `Play.revive_offline`'s own
  docstring says those two trees measured the wrong column — health, which is
  "exactly the column [the offline branch] does not touch" — and calls them "the
  trees whose own boxes have not looked again". That is a per-tree claim, it is
  known to rest on a refuted method, and it sits outside this guard.
* `accounts.scheme` is absent on `wow-wotlk`, so `azerothcore` is the model
  default — and 8.3a's Windows half *did* measure it, recomputing the stored
  verifier by hand from the salt and matching the new password byte for byte. A
  real per-tree measurement, outside the guard, because nobody had to type the
  value.

The alternative rule — mark every model field, present or absent — is measured
rather than estimated: `play` + `accounts` is **16 leaves per game, 64 across
four games**, against 39 today. Twenty-five more rows, most of which would read
"the model's default, nobody asked this tree", which is exactly the sentence the
`inherited` kind exists to make loud. It is a fair trade and it is the owner's
call, not this lane's.

## What is marked, and what is not

39 written-down values. **35 carry provenance; 4 are owed.**

| game | play | accounts | owed |
| --- | --- | --- | --- |
| `wow-wotlk` | 6 of 7 | 3 of 4 | `play.mail_item_cap`, `accounts.level.max_level` |
| `wow-tbc` | 5 of 5 | 2 of 3 | `accounts.level.max_level` |
| `wow-vanilla` | 6 of 6 | 3 of 3 | — |
| `wow-tortoise` | 8 of 8 | 2 of 3 | `accounts.scheme` |

34 of the 35 are `measured-on`, each citing a gate README that the guard opens.
The one `read-from-source` is `wow-tortoise:play.rename_offline_refusal`
(`Commands.cpp:12624`), and it is the one value here that **must not** be
measured: running it would `UPDATE characters SET name = guid` and throw a real
name away. 8.4d proved the online arm instead and withholds the offline one.

`operations` and `observability` are the other two per-tree blocks and are **not
marked**. They hold **64** written-down leaves between them by the same rule, so
covering all four blocks is 103. Out of scope by the task's own wording; the
number is here so the next lane does not have to count.

### The four debts, and what settles each

Three are ceilings and one is a citation. **No tree's table, column or command
is unmeasured** — that is worth saying, because it means the debts are all of
one shape: a press read a value back without ever asking for the answer the tree
would refuse.

1. `wow-wotlk:play.mail_item_cap` (12). 8.4a sent twelve worn items in one mail
   and split nineteen into two, so twelve **fits**. A cap is a ceiling, and
   nothing asked whether thirteen is refused. Both siblings have that reading:
   8.4b measured by exceeding it (`mail-cap.py`, 12 yes / 13 refused).
   **Settled by** 8.4b's probe pointed at this box's server.
2. `wow-wotlk:accounts.level.max_level` (3). Neither 8.3a press probed the
   ceiling — no refused level appears in either README, on `yulon-ubuntu` or on
   `yulon-win11-gate`. `AccountLevel.max_level`'s docstring names
   `SEC_ADMINISTRATOR` and gives no `path:line`, so there is not even a
   `read-from-source` citation to fall back on. **Settled by** 8.3d's probe
   (accept the ceiling, be refused one above it) run here.
3. `wow-tbc:accounts.level.max_level` (3). As above: 8.3b changed a level and
   read it back, and never asked for one the tree would refuse. The value is
   right in all likelihood, which is what makes it worth a row — a surface
   drawing the wrong ceiling here does not fail, it silently offers less than
   the tree has.
4. `wow-tortoise:accounts.scheme` (`mangos_sha`). Measured against a live server
   on 2026-08-26 per `Accounts`' docstring, the core logging its own INSERT and
   `SHA1(UPPER(user):UPPER(pass))` matching it exactly — and that predates
   `pyplan/gates/`, so no folder holds the reading. The measurement is not
   doubted; the **citation** cannot be resolved, which is the shape `dcc64543`
   caught in 8.3c's README (`tests/test_srp6.py`, a file never written under
   that name). **Settled by** committing the reading, or re-running it beside
   8.3d's transcript.

## The guard

Five tests, appended to `pylauncher/tests/test_catalog_invariants.py`:

* `test_every_value_the_catalog_writes_about_a_tree_says_where_it_came_from` —
  the rule. Every leaf is marked or owed, never both; no row names a value the
  catalog no longer writes.
* `test_the_provenance_debts_are_exactly_these_four` — the exit line's own
  clause, spelled out rather than derived, and exact in both directions so a
  discharged debt must be struck and a new one cannot be added quietly.
* `test_a_measured_provenance_cites_a_page_that_is_really_there` — opens all 34.
* `test_a_read_from_source_provenance_cites_a_line_and_not_a_sentence`.
* `test_an_inherited_value_names_the_tree_it_came_from_and_that_tree_measured_it`
  — driven against a fixture, because the shipped table has no `inherited` row
  and a rule never handed one is a rule nobody has run. The real table is
  asserted to have none, so a future row does not arrive believing it is covered
  by a live case.

Anti-vacuity, since that is what this whole round is about: the walk is asserted
non-empty and asserted to reach all four games **before** any rule runs. The
exact count of 39 is asserted **last**, deliberately — a change detector ahead
of the rule costs the rule its error message, and a new field should go red on
the sentence naming what to do about it rather than on a number.

Six mutations, each with `__pycache__` purged on both sides, each red, each
restored: a new unmarked leaf in `catalog.json`; a leaf deleted from it; a
`PROVENANCE` row deleted; a citation pointed one day earlier; a debt given a
provenance row as well; a debt struck from `OWED` with nothing measured.

## Prior art

`origin/rust-main` has no equivalent and could not have had one — it shipped one
emulator family, so there was no sibling to inherit from. What it does have is
the same fact recorded the same lossy way: `crates/dml-wow/src/destructive.rs:89`
documents `TitleRow.family` as "a catalog default for a title that may not be
installed yet, **not a claim about any installed server**". Exactly a provenance
statement, in a doc comment, checkable by nothing.

## Open for the owner

1. Presence-in-the-file, or every model field (39 rows against 64)? The two
   absent-but-claimed values above are the argument for the second.
2. Extend to `operations` and `observability` (a further 64 leaves)?
3. Is `wow-tortoise:accounts.scheme` discharged by committing the 2026-08-26
   reading into a gate folder, or does it want re-running?
4. Should this ever reach the surface — a control saying "this tree was never
   asked" — or does it stay a development-time guard?
