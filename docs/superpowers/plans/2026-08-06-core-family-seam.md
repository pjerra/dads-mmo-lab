# Core-Family Seam Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a `CoreFamily` seam so a title's emulator family is resolved once, from evidence, and refused when unknown — delivering a WoW-only Library and fixing the hardcoded database names in the same shape.

**Architecture:** A `CoreFamily` enum in `crates/dml-wow/src/family.rs` says *which questions to ask* (conf location, conf filenames, DB-name keys, SOAP URN); the installed server says *what the answers are*. Resolution is install-record → compose-file inference → refusal, never a default. The family ships with only the `AzerothCore` variant, so adding CMaNGOS in sub-project #2 is a compile error at every site that must learn about it.

**Tech Stack:** Rust (`dml-core`, `dml-wow`, `launcher/src-tauri`), bash (`cli/src/*.sh`), Svelte 5 + TypeScript (`launcher/src`), vitest, bats, cargo test.

**Spec:** `docs/superpowers/specs/2026-08-06-core-family-seam-design.md`
**Branch:** `feat/core-family` (off `rust-main` at `72f15be`)

## Global Constraints

- **Mirror rule.** `cli/` bash and `crates/` Rust mirror each other. Any change to the title catalog lands on BOTH surfaces in the SAME commit. Details: root `CLAUDE.md`.
- **Refuse, never default.** Every uncertainty about a title's family resolves as a named error. A wrong guess reads `acore_characters` from a database called `characters` and answers `ok:true` with numbers that are not the server's.
- **Mutation proof per task.** Make the code wrong in the specific way the test exists to catch, watch it go RED, restore with an **Edit** — never `git checkout`, which rewrites line endings (`core.autocrlf=true`, no `*.rs`/`*.ts`/`*.svelte` rule in `.gitattributes`).
- **Never judge a bats run by a piped tail.** `bats tests/ | tail` reports `tail`'s exit code, always 0. Redirect to a file and read the code.
- **bats and jq are NOT on the Git Bash PATH.** They live in the distro. Run: `wsl -d dml-arch -u dml -- bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && bats cli/tests/ > /tmp/bats.out 2>&1; echo EXIT=$?'` then read counts in a SEPARATE call.
- **Never run bats and the cargo parity suites at the same time.** Every bats `setup()` runs `bash cli/build.sh`, which rewrites `cli/dml` in place while the parity suites spawn it as their oracle.
- **Cargo lives at the repo root**, not `launcher/src-tauri`. If `cargo` is not found, prepend `$HOME/.cargo/bin` to PATH.
- **Existing baselines to preserve** (measured 2026-08-06 on this branch's base): `cargo test --workspace` 1667 passed / 0 failed; `npm test` 63 files / 772; `npm run check` 333 files 0 errors; bats 840 ok / 0 not ok.

## Scope

This plan covers **I0 (Tasks 1–4)** and **I1 (Task 5)** from the spec.

**I2–I5 are deliberately NOT planned here.** They are independent of each other but all consume the API that Task 1 defines and Task 5 first exercises. Planning their code now would mean inventing that API rather than discovering it. Their content and order are already fixed by spec §6; they get a follow-on plan written against the real signatures once Task 5 lands. This plan produces working, shippable software on its own: a WoW-only Library and a real AzerothCore bug fix.

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/dml-wow/src/family.rs` | **Create.** `CoreFamily`, `FamilyVerdict`, pure inference from container names. Nothing else — this file must stay small enough to hold in context | 1 |
| `crates/dml-wow/src/lib.rs` | **Modify.** `pub mod family;` | 1 |
| `crates/dml-wow/src/destructive.rs` | **Modify.** `TitleRow` gains `family`; the six rows gain their value | 2 |
| `cli/src/80-titles.sh` | **Modify.** `_title_registry` gains a 6th pipe field; `_title_family` accessor | 2 |
| `cli/src/90-main.sh` | **Modify.** `games catalog` emits `family` per title | 3 |
| `launcher/src/lib/title-install.ts` | **Modify.** `TitleInfo` gains `family`; `normalizeCatalog` carries it; `visibleTitles` filter | 3 |
| `launcher/src/lib/title-install.test.ts` | **Modify.** Filter + normalisation tests | 3 |
| `crates/dml-wow/src/family_scan_tests.rs` *(or an inline `#[cfg(test)] mod`)* | **Create.** The exactly-one-resolver structural guard | 4 |
| `crates/dml-wow/src/db.rs` | **Modify.** `Database::name()` → conf-resolved | 5 |

---

## Task 1: `CoreFamily` and the resolution verdict

**Files:**
- Create: `crates/dml-wow/src/family.rs`
- Modify: `crates/dml-wow/src/lib.rs`
- Test: inline `#[cfg(test)] mod tests` in `crates/dml-wow/src/family.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub enum CoreFamily { AzerothCore }`
  - `pub enum FamilyVerdict { Known(CoreFamily), Unsupported { family: &'static str }, Unknown }`
  - `pub fn family_from_container_names<'a>(names: impl Iterator<Item = &'a str>) -> FamilyVerdict`
  - `pub const ERR_FAMILY_UNKNOWN: &str = "TITLE_FAMILY_UNKNOWN";`
  - `pub const ERR_FAMILY_UNSUPPORTED: &str = "TITLE_FAMILY_UNSUPPORTED";`

- [ ] **Step 1: Write the failing test**

Create `crates/dml-wow/src/family.rs` containing ONLY this test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn azerothcore_is_identified_by_its_worldserver() {
        let names = ["ac-database", "ac-worldserver", "ac-authserver"];
        assert_eq!(
            family_from_container_names(names.iter().copied()),
            FamilyVerdict::Known(CoreFamily::AzerothCore)
        );
    }

    /// RECOGNISED BUT NOT SUPPORTED is a different answer from UNKNOWN, and the
    /// difference is what the user reads. "Vanilla servers are not supported
    /// yet" is true and actionable; "unknown server type" is neither.
    #[test]
    fn a_cmangos_stack_is_recognised_and_refused_by_name() {
        for names in [
            vec!["vanilla-db", "vanilla-mangosd", "vanilla-realmd"],
            vec!["tbc-db", "tbc-mangosd", "tbc-realmd"],
        ] {
            assert_eq!(
                family_from_container_names(names.iter().copied()),
                FamilyVerdict::Unsupported { family: "CMaNGOS" },
                "{names:?}"
            );
        }
    }

    #[test]
    fn nothing_identifiable_is_unknown_not_a_default() {
        for names in [vec![], vec!["mysql"], vec!["some-other-game-db"]] {
            assert_eq!(
                family_from_container_names(names.iter().copied()),
                FamilyVerdict::Unknown,
                "{names:?}"
            );
        }
    }

    /// A compose file holding BOTH families is not a majority vote. Guessing
    /// here sends urn:AC at a MaNGOS server (or the reverse) and every read
    /// lands in the wrong database while answering ok:true.
    #[test]
    fn a_mixed_stack_refuses_rather_than_picking_one() {
        let names = ["ac-worldserver", "vanilla-mangosd"];
        assert_eq!(
            family_from_container_names(names.iter().copied()),
            FamilyVerdict::Unknown
        );
    }

    /// EXACT names, never "contains". The repo already ate a false refusal from
    /// a compose file that merely MENTIONED an AC image, which is why
    /// parse_stack_owners anchors on the container_name key.
    #[test]
    fn a_lookalike_name_does_not_match() {
        for n in ["not-ac-worldserver", "ac-worldserver-backup", "mangosd"] {
            assert_eq!(
                family_from_container_names([n].iter().copied()),
                FamilyVerdict::Unknown,
                "{n} must not match"
            );
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

**First add `pub mod family;` to `crates/dml-wow/src/lib.rs`** (alphabetical position among the existing `pub mod` lines). A `.rs` file in `src/` that no module declaration references is never compiled by cargo — without this line the run reports unrelated passing tests instead of the failure, which is a vacuous green dressed as a red. Corrected 2026-08-06 after the Task 1 implementer hit it.

Run: `cargo test -p dml-wow --lib family`
Expected: FAIL to compile — `cannot find function family_from_container_names`, `cannot find type FamilyVerdict`.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/dml-wow/src/family.rs` (above the test module):

```rust
//! Which emulator family a title's server is.
//!
//! THE SPINE: the family says which questions to ask; the installed server says
//! what the answers are. This type never holds a value the server already knows
//! — database names, container names and ports all come from the install, which
//! is what keeps this from becoming the recorded TWO-RESOLVERS-FOR-ONE-VALUE
//! bug.
//!
//! An enum rather than a string so that adding a family is a COMPILE ERROR at
//! every match rather than a silent fallthrough. `backend::from_override`'s
//! `_ => Backend::Wsl` catch-all is the live counter-example: it makes
//! `DML_BACKEND=auto` resolve Native and then run as Wsl.

/// A family this launcher can operate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreFamily {
    AzerothCore,
}

/// What inference concluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyVerdict {
    /// Identified, and this launcher can operate it.
    Known(CoreFamily),
    /// Identified, and this launcher cannot operate it YET (sub-projects #2/#3).
    /// Carrying the name is the whole point: the user gets "vanilla is not
    /// supported yet" instead of "unknown".
    Unsupported { family: &'static str },
    /// Nothing in the evidence identifies a family. NOT a default.
    Unknown,
}

/// Error code for [`FamilyVerdict::Unknown`].
pub const ERR_FAMILY_UNKNOWN: &str = "TITLE_FAMILY_UNKNOWN";
/// Error code for [`FamilyVerdict::Unsupported`].
pub const ERR_FAMILY_UNSUPPORTED: &str = "TITLE_FAMILY_UNSUPPORTED";

/// AzerothCore's two identifying containers. EXACT matches only.
const AC_MARKERS: &[&str] = &["ac-worldserver", "ac-authserver"];
/// CMaNGOS names its world/auth servers `<title>-mangosd` / `<title>-realmd`.
const CMANGOS_SUFFIXES: &[&str] = &["-mangosd", "-realmd"];

/// Pure: infer the family from a compose file's `container_name:` values.
///
/// Feed this the output of [`crate::install_native::parse_stack_owners`], never
/// a bare grep of the file — the repo already ate a false refusal from a compose
/// that merely MENTIONED an AC image.
///
/// A stack showing BOTH families is `Unknown`, not a majority vote: guessing
/// there sends `urn:AC` at a MaNGOS server and reads `acore_characters` from a
/// database called `characters`, which fails in the silently-wrong direction.
pub fn family_from_container_names<'a>(
    names: impl Iterator<Item = &'a str>,
) -> FamilyVerdict {
    let mut saw_ac = false;
    let mut saw_cmangos = false;
    for n in names {
        let n = n.trim();
        if AC_MARKERS.contains(&n) {
            saw_ac = true;
        }
        if CMANGOS_SUFFIXES.iter().any(|s| n.ends_with(s)) {
            saw_cmangos = true;
        }
    }
    match (saw_ac, saw_cmangos) {
        (true, false) => FamilyVerdict::Known(CoreFamily::AzerothCore),
        (false, true) => FamilyVerdict::Unsupported { family: "CMaNGOS" },
        _ => FamilyVerdict::Unknown,
    }
}
```

Note `"mangosd"` alone must NOT match: `ends_with("-mangosd")` requires the hyphen, which is what the `a_lookalike_name_does_not_match` case pins.

(`pub mod family;` was already added in Step 2 — see the note there for why it cannot wait until here.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dml-wow --lib family`
Expected: PASS, 5 tests.

- [ ] **Step 5: Mutation — prove the mixed-stack refusal is load-bearing**

Change the match arm `_ => FamilyVerdict::Unknown` to:

```rust
        (true, true) => FamilyVerdict::Known(CoreFamily::AzerothCore),
        _ => FamilyVerdict::Unknown,
```

Run: `cargo test -p dml-wow --lib family`
Expected: FAIL on `a_mixed_stack_refuses_rather_than_picking_one`.
Then restore with an Edit (not `git checkout`).

- [ ] **Step 6: Mutation — prove exact-match is load-bearing**

Change `AC_MARKERS.contains(&n)` to `AC_MARKERS.iter().any(|m| n.contains(m))`.

Run: `cargo test -p dml-wow --lib family`
Expected: FAIL on `a_lookalike_name_does_not_match`.
Then restore with an Edit.

- [ ] **Step 7: Verify the whole crate still builds green**

Run: `cargo test -p dml-wow --lib`
Expected: PASS, previous count + 5.

- [ ] **Step 8: Commit**

```bash
git add crates/dml-wow/src/family.rs crates/dml-wow/src/lib.rs
git commit -m "feat(family): CoreFamily, and a verdict that refuses rather than guesses

Recognised-but-unsupported is a DIFFERENT answer from unknown, and the
difference is what the user reads: 'vanilla is not supported yet' is true and
actionable, 'unknown server type' is neither.

A mixed stack is Unknown, not a majority vote. Guessing sends urn:AC at a
MaNGOS server and reads acore_characters from a database called characters --
ok:true with numbers that are not the server's.

Mutations RED: majority-vote on a mixed stack, and contains() instead of exact
container-name matching."
```

---

## Task 2: The `family` column, on both catalog surfaces

**Files:**
- Modify: `cli/src/80-titles.sh:54-64` (the `_title_registry` heredoc)
- Modify: `crates/dml-wow/src/destructive.rs:76-82` (`TitleRow`) and `:85-130` (the six rows)
- Test: `crates/dml-wow/src/destructive.rs` inline tests; `cli/tests/games-titles.bats`

**Interfaces:**
- Consumes: `CoreFamily` (Task 1) — for the *doc* link only; the column is a string, because bash has no enum and the two surfaces must carry the same bytes.
- Produces:
  - `TitleRow.family: &'static str` — one of `"azerothcore"`, `"cmangos"`, `"other"`
  - bash `_title_family <id>` → prints the 6th field

The column is a plain string on purpose. `CoreFamily` is the *operating* type resolved from the installed server (Task 1); this column is a catalog **default** for a title that may not be installed yet. Conflating them would make the catalog authoritative over the server, which is exactly backwards — see spec §5.

- [ ] **Step 1: Write the failing test (Rust)**

Add to `crates/dml-wow/src/destructive.rs`'s existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn every_title_declares_a_family() {
        for row in TITLE_REGISTRY {
            assert!(
                matches!(row.family, "azerothcore" | "cmangos" | "other"),
                "{}: family {:?} is not one of the three known values",
                row.id,
                row.family
            );
        }
    }

    #[test]
    fn the_wow_titles_declare_the_family_their_installer_builds() {
        for (id, family) in [
            ("wow-server-playerbots", "azerothcore"),
            ("wow-vanilla-server", "cmangos"),
            ("wow-tbc-server", "cmangos"),
            ("maplestory-server", "other"),
            ("runescape-server", "other"),
            ("muonline-server", "other"),
        ] {
            let row = title_row(id).unwrap_or_else(|| panic!("{id} missing from the registry"));
            assert_eq!(row.family, family, "{id}");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dml-wow --lib destructive`
Expected: FAIL to compile — `no field family on type TitleRow`.

- [ ] **Step 3: Add the field on the Rust surface**

In `crates/dml-wow/src/destructive.rs`, extend the struct:

```rust
pub struct TitleRow {
    pub id: &'static str,
    pub name: &'static str,
    pub installer: &'static str,
    pub kind: &'static str,
    pub launcher: &'static str,
    /// Which emulator family this title's installer BUILDS — a catalog default
    /// for a title that may not be installed yet, not a claim about any
    /// installed server. The operating answer is resolved from the server
    /// itself (`family::family_from_container_names`); this is what the Library
    /// filters on and what a fresh install records.
    ///
    /// `"azerothcore" | "cmangos" | "other"`. A string, not `CoreFamily`,
    /// because bash carries the identical value in `_title_registry`'s 6th
    /// field and the two surfaces must be byte-comparable.
    pub family: &'static str,
}
```

Add `family:` to each of the six rows: `wow-server-playerbots` → `"azerothcore"`; `wow-vanilla-server` and `wow-tbc-server` → `"cmangos"`; `maplestory-server`, `runescape-server`, `muonline-server` → `"other"`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dml-wow --lib destructive`
Expected: PASS.

- [ ] **Step 5: Add the field on the bash surface**

In `cli/src/80-titles.sh`, update the comment above `_title_registry` and the heredoc:

```bash
# id|display name|installer script|kind(games=installer manages ~/games itself,
# home=legacy $HOME/<id> layout needing a post-install symlink)|launcher file|
# family(azerothcore|cmangos|other -- which emulator the installer BUILDS; the
# Library filters on it, and it MIRRORS destructive.rs's TitleRow.family)
_title_registry() {
cat <<'EOF'
wow-server-playerbots|WoW WotLK (Playerbots)|install-wow-wotlk.sh|games|wow-playerbots-launcher.sh|azerothcore
wow-vanilla-server|WoW Vanilla|install-wow-vanilla.sh|home|wow-vanilla-launcher.sh|cmangos
wow-tbc-server|WoW TBC|install-wow-tbc.sh|home|wow-tbc-launcher.sh|cmangos
maplestory-server|MapleStory v83|install-maplestory.sh|home|maplestory-launcher.sh|other
runescape-server|RuneScape|install-runescape.sh|home|runescape-launcher.sh|other
muonline-server|MU Online|install-muonline.sh|home|muonline-launcher.sh|other
EOF
}

# Prints the family for an id, or nothing.
_title_family() {
    local row
    row="$(_title_row "$1")" || return 1
    [ -n "$row" ] || return 1
    printf '%s' "$row" | cut -d'|' -f6
}
```

- [ ] **Step 6: Write the mirror test**

Add to `crates/dml-wow/src/destructive.rs`'s test module. This reads the bash file directly, which is how the repo pins its other mirrors:

```rust
    /// THE MIRROR. `_title_registry` and `TITLE_REGISTRY` are the same table on
    /// two surfaces; a family added to one and not the other is a Library that
    /// shows different titles depending on which binary answered.
    #[test]
    fn the_bash_registry_carries_the_same_families() {
        let sh = include_str!("../../../cli/src/80-titles.sh").replace("\r\n", "\n");
        let body = sh
            .split_once("_title_registry() {\ncat <<'EOF'\n")
            .expect("the _title_registry heredoc was renamed or reshaped")
            .1
            .split_once("\nEOF\n")
            .expect("unterminated _title_registry heredoc")
            .0;
        let rows: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            rows.len(),
            TITLE_REGISTRY.len(),
            "bash has {} rows, Rust has {}",
            rows.len(),
            TITLE_REGISTRY.len()
        );
        for (line, row) in rows.iter().zip(TITLE_REGISTRY) {
            let f: Vec<&str> = line.split('|').collect();
            assert_eq!(f.len(), 6, "row {line:?} does not have 6 fields");
            assert_eq!(f[0], row.id, "id mismatch in {line:?}");
            assert_eq!(f[5], row.family, "family mismatch for {}", row.id);
        }
    }
```

The `.replace("\r\n", "\n")` is mandatory: `.gitattributes` forces LF for `*.sh`, but a scan that assumed either ending has already cost this repo a debugging round.

- [ ] **Step 7: Run the test**

Run: `cargo test -p dml-wow --lib destructive`
Expected: PASS.

- [ ] **Step 8: Mutation — prove the mirror test bites**

Edit `cli/src/80-titles.sh` and change `wow-vanilla-server`'s trailing `|cmangos` to `|azerothcore`.

Run: `cargo test -p dml-wow --lib destructive`
Expected: FAIL on `the_bash_registry_carries_the_same_families` with "family mismatch for wow-vanilla-server".
Then restore with an Edit.

- [ ] **Step 9: Run bats to confirm the bash surface is unbroken**

Run:
```bash
wsl -d dml-arch -u dml -- bash -lc 'cd /mnt/c/Users/perzi/dads-mmo-lab && bats cli/tests/ > /tmp/bats.out 2>&1; echo EXIT=$?'
```
Then, in a SEPARATE call:
```bash
wsl -d dml-arch -u dml -- bash -lc 'grep -c "^ok" /tmp/bats.out; grep -c "^not ok" /tmp/bats.out'
```
Expected: 840 ok, 0 not ok. Any `not ok` means a consumer was reading `cut -d'|' -f5` as the last field.

- [ ] **Step 10: Commit**

**`cli/dml` IS A COMMITTED BUILD ARTIFACT.** Any change under `cli/src/` must be
followed by `bash cli/build.sh` and the rebuilt `cli/dml` committed WITH it —
otherwise the source has the family column and the binary that actually runs
does not. (bats' own `setup()` rebuilds it as a side effect, so a bats run
leaves the tree dirty; that dirt is the real artifact and must be committed, not
discarded.) Added 2026-08-06 after the Task 2 implementer caught the omission.

```bash
bash cli/build.sh
git add cli/src/80-titles.sh crates/dml-wow/src/destructive.rs cli/dml
git commit -m "feat(titles): every title declares the emulator family it builds

Mirrored, in one commit, because the catalog exists on both surfaces and a
family added to one is a Library that shows different titles depending on
which binary answered.

A STRING, not CoreFamily: this is a catalog default for a title that may not
be installed yet, and bash must carry byte-identical values. The operating
answer is resolved from the installed server.

Mutation RED: flipping vanilla's family in the bash heredoc reddens the mirror
test. bats 840/0."
```

---

## Task 3: The Library shows only WoW (delivers sub-project #0)

**Files:**
- Modify: `cli/src/90-main.sh` — the `catalog)` arm (~line 1662, the `tout+=` line)
- Modify: `launcher/src/lib/title-install.ts:187-210`
- Test: `launcher/src/lib/title-install.test.ts`

**Interfaces:**
- Consumes: `_title_family` (Task 2), `TitleRow.family` (Task 2).
- Produces:
  - TS `TitleInfo.family?: string`
  - `export function visibleTitles(titles: TitleInfo[]): TitleInfo[]`
  - `normalizeCatalog` passes `family` through unchanged.

- [ ] **Step 1: Write the failing test**

Add to `launcher/src/lib/title-install.test.ts`:

```ts
describe("visibleTitles", () => {
  const t = (id: string, family?: string) => ({ id, name: id, family }) as TitleInfo;

  it("shows the WoW families and hides everything else", () => {
    const out = visibleTitles([
      t("wow-server-playerbots", "azerothcore"),
      t("wow-vanilla-server", "cmangos"),
      t("maplestory-server", "other"),
      t("runescape-server", "other"),
    ]);
    expect(out.map((x) => x.id)).toEqual(["wow-server-playerbots", "wow-vanilla-server"]);
  });

  /**
   * FAILS OPEN, exactly like normalizeCatalog's install_supported. An older
   * `dml` in dml-arch that predates the family column omits it, and hiding
   * every title there would replace a working Library with an empty one --
   * swapping one wrong story for another.
   */
  it("shows a title whose family the CLI did not report", () => {
    const out = visibleTitles([t("wow-server-playerbots", undefined)]);
    expect(out.map((x) => x.id)).toEqual(["wow-server-playerbots"]);
  });

  it("hides an unknown family rather than showing it", () => {
    expect(visibleTitles([t("something", "runescape")])).toEqual([]);
  });
});
```

Extend the existing `normalizeCatalog` describe block:

```ts
  it("carries the family through untouched", () => {
    const out = normalizeCatalog({ titles: [{ id: "a", name: "A", family: "cmangos" }] as TitleInfo[] });
    expect(out.titles[0].family).toBe("cmangos");
  });
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd launcher && npx vitest run src/lib/title-install.test.ts`
Expected: FAIL — `visibleTitles is not exported`.

- [ ] **Step 3: Implement**

**CORRECTED 2026-08-06** — `TitleInfo` is DECLARED in `launcher/src/lib/api.ts`;
`title-install.ts` only imports it. Add `family?: string;` to the interface where
it actually lives, then append the filter below to `title-install.ts`:

```ts
/**
 * The families the Library shows. WoW only, by user ruling 2026-08-06 — the
 * non-WoW titles stay on disk because they are currently the suite's only
 * multi-title fixtures (`cli/tests/games-list.bats:15` installs runescape).
 * They get deleted once vanilla can replace them as that fixture.
 */
const VISIBLE_FAMILIES = ["azerothcore", "cmangos"];

/**
 * FAILS OPEN on a missing family, for the same reason `normalizeCatalog`'s
 * `install_supported` does: an older `dml` in dml-arch predating the family
 * column omits it, and hiding every title there turns a working Library into
 * an empty one.
 */
export function visibleTitles(titles: TitleInfo[]): TitleInfo[] {
  return titles.filter((t) => t.family === undefined || VISIBLE_FAMILIES.includes(t.family));
}
```

`normalizeCatalog` needs no change — it spreads `titles` through — but confirm the new test proves that.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd launcher && npx vitest run src/lib/title-install.test.ts`
Expected: PASS.

- [ ] **Step 5: Emit `family` from the bash catalog arm**

**CORRECTED 2026-08-06** — the first draft of this step read a `$trow` variable
that does not exist at this call site, and would also have left a real defect in
place. The `catalog)` arm is a positional read loop
(`cli/src/90-main.sh:1647`), and `cli/src/70-modules.sh:75-79` already records
why that matters: *"every registry consumer does a positional `IFS='|' read`
whose LAST variable swallows any trailing remainder, so appending a column would
silently corrupt the last field."* With five variables against six fields,
`tlauncher` silently becomes `"wow-playerbots-launcher.sh|azerothcore"`.

So extend the READ, which fixes the corruption and provides the value in one
move:

```bash
        while IFS='|' read -r tid tname tscript tkind tlauncher tfamily; do
```

If Task 2's fix round already made this change, verify it rather than repeating it.

Then extend the JSON object line to include it:

```bash
          tout+="{\"id\":\"$tid\",\"name\":\"$(json_escape "$tname")\",\"family\":\"$tfamily\",\"installed\":$tinst,\"running\":$trun,\"script_available\":$tscriptok}"
```

- [ ] **Step 5b: Pin the catalog JSON's family — this closes a gap Task 2 could not**

Task 2 fixed the six-variable read, then honestly reported that **reverting it to
five variables reddened nothing**: no assertion anywhere read `tkind`/`tlauncher`
out of the catalog JSON, so the corruption — and its fix — were both invisible.
Adding `family` to the JSON is the first time that loop's tail has an observable
consequence, so it is the first time the read can be pinned. Do not skip this.

Add to `cli/tests/games-titles.bats`, following the conventions already in that
file:

**CORRECTED 2026-08-06** — the first draft invoked a bare `run dml …`. `dml` is
not on PATH anywhere in this suite; every sibling test uses `run bash "$DML" …`.
The bare form would have gone red for the wrong reason, which makes a mutation
proof worthless — the same class as the recorded "anchor the ordering test on
the REFUSAL, not on X-appears-before-Y" lesson. Match the file's own convention.

```bash
@test "games catalog reports each title's family" {
  run bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .family')" = "azerothcore" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-vanilla-server") | .family')" = "cmangos" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .family')" = "other" ]
}
```

Then MUTATE the read back to five variables
(`... tkind tlauncher;`, dropping `tfamily`) and run this file. It must go RED —
`family` will be empty or absent. If it does NOT go red, the assertion is not
actually reaching the catalog arm and must be fixed before you continue.
Restore with an Edit, then rebuild `cli/dml`.

- [ ] **Step 6: Apply the filter in the Library**

Find the Library's title list (`grep -rn "gamesCatalog\|TitleCatalog" launcher/src/lib/pages/Library.svelte`) and wrap the rendered array in `visibleTitles(...)`, importing it from `$lib/title-install`.

- [ ] **Step 7: Pin the wiring with a source scan**

The filter is WIRING: `visibleTitles` can be perfectly unit-tested and simply not called. Add to `launcher/src/lib/title-install.test.ts`:

```ts
import { code, sourceFinder, blockOf } from "./source-scan";

const LIB = import.meta.glob(["../lib/pages/Library.svelte"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;
const findLib = sourceFinder(LIB);

describe("the Library actually applies the filter", () => {
  it("calls visibleTitles on the rendered list", () => {
    const src = code(findLib("Library.svelte"));
    expect(
      src,
      "Library renders the raw catalog. visibleTitles can be perfectly unit-tested " +
        "and never called -- that is the wiring failure this repo paid for twice on 2026-08-05.",
    ).toContain("visibleTitles(");
  });
});
```

- [ ] **Step 8: Mutation — prove the scan bites**

Remove the `visibleTitles(` call from `Library.svelte`, leaving the import.

Run: `cd launcher && npx vitest run src/lib/title-install.test.ts`
Expected: FAIL on "calls visibleTitles on the rendered list".
Then restore with an Edit.

- [ ] **Step 9: Full frontend gates**

Run: `cd launcher && npx vitest run` → expected 63 files, 772 + your new tests.
Run: `cd launcher && npm run check` → expected 333 files, 0 errors.

- [ ] **Step 10: Commit**

`cli/dml` is a committed build artifact — see Task 2 Step 10. This task edits
`cli/src/90-main.sh`, so rebuild and commit it in the same commit.

```bash
bash cli/build.sh
git add cli/src/90-main.sh cli/dml launcher/src/lib/title-install.ts launcher/src/lib/title-install.test.ts launcher/src/lib/pages/Library.svelte
git commit -m "feat(library): show WoW titles only

Sub-project #0. The three non-WoW titles stay on disk -- they are currently
the suite's only multi-title fixtures (games-list.bats installs runescape) --
and are hidden, not deleted, until vanilla can replace them as that fixture.

The filter FAILS OPEN on a missing family, like normalizeCatalog's
install_supported: an older dml in dml-arch that predates the column would
otherwise turn a working Library into an empty one.

Wiring is scan-pinned, because visibleTitles can be perfectly unit-tested and
simply never called. Mutation RED on removing the call."
```

---

## Task 4: The structural guard — exactly one family resolver

**Files:**
- Create: `crates/dml-wow/src/family.rs` — append a `#[cfg(test)] mod resolver_scan_tests`
- Test: same file

**Interfaces:**
- Consumes: `family_from_container_names` (Task 1).
- Produces: nothing at runtime. A build-time guard.

This is the task most likely to be skipped and the one with the highest value. The games-dir incident happened because two functions independently answered one question and **only the fallback disagreed** — and the 18 parity suites structurally could not see it, because they pass explicit paths precisely to avoid the process env. A fixed file list cannot see a second resolver arriving in a NEW file, so this walks the directory.

- [ ] **Step 1: Write the failing test**

Append to `crates/dml-wow/src/family.rs`:

```rust
/// EXACTLY ONE PLACE DECIDES A TITLE'S FAMILY.
///
/// Modelled on the games-dir incident: two resolvers answered one question,
/// they agreed on the happy path, and only the FALLBACK disagreed — so every
/// read fell through to defaults and answered `ok:true` with numbers that were
/// not the server's. A second family resolver would be worse: it decides which
/// DATABASE to read and which SOAP namespace to send.
///
/// A runtime directory walk, not a fixed file list, because the failure mode is
/// a second resolver arriving in a file this test has never heard of.
#[cfg(test)]
mod resolver_scan_tests {
    use std::collections::BTreeSet;

    /// WHAT THIS CAN AND CANNOT CATCH — read before changing it.
    ///
    /// The obvious marker list (`"ac-worldserver"`, `"-mangosd"`, …) is WRONG
    /// and was tried first: those container names legitimately appear in 10+
    /// files, because `composegen` generates them and `lifecycle` stops them.
    /// Mentioning a container name is not deciding a family.
    ///
    /// So this guard pins two narrower things: exactly one production CALL of
    /// the resolver, and the marker tables living only in `family.rs`. It
    /// catches the two realistic accidents — a second caller with its own
    /// fallback (the games-dir shape) and a copy-pasted marker table. It does
    /// NOT catch a cleverly-rewritten independent implementation; nothing
    /// textual would. That residue is covered by review, and it is named here
    /// rather than papered over.
    const RESOLVER_CALL: &str = "family_from_container_names(";
    const MARKER_TABLES: &[&str] = &["AC_MARKERS", "CMANGOS_SUFFIXES"];

    /// The ONLY file allowed to define the tables.
    const OWNER: &str = "family.rs";

    fn strip_comments(src: &str) -> String {
        let mut out = String::with_capacity(src.len());
        let b: Vec<char> = src.chars().collect();
        let (mut i, mut in_str, mut in_line, mut in_block) = (0usize, false, false, 0usize);
        while i < b.len() {
            let c = b[i];
            let next = b.get(i + 1).copied().unwrap_or('\0');
            if in_line {
                if c == '\n' { in_line = false; out.push(c); }
            } else if in_block > 0 {
                if c == '*' && next == '/' { in_block -= 1; i += 2; continue; }
                if c == '/' && next == '*' { in_block += 1; i += 2; continue; }
                if c == '\n' { out.push(c); }
            } else if in_str {
                out.push(c);
                if c == '\\' { if let Some(n) = b.get(i + 1) { out.push(*n); } i += 2; continue; }
                if c == '"' { in_str = false; }
            } else if c == '/' && next == '/' {
                in_line = true;
            } else if c == '/' && next == '*' {
                in_block = 1; i += 2; continue;
            } else {
                if c == '"' { in_str = true; }
                out.push(c);
            }
            i += 1;
        }
        out
    }

    #[test]
    fn only_family_rs_decides_what_a_stack_is() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut scanned = 0usize;
        let mut calls = 0usize;
        let mut offenders: BTreeSet<String> = BTreeSet::new();
        for entry in std::fs::read_dir(&dir).expect("crates/dml-wow/src is unreadable") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            scanned += 1;
            let src = strip_comments(&std::fs::read_to_string(&path).expect("read"));
            // Production half only: the tests in family.rs call the resolver
            // many times and must not count.
            let production = src.split("#[cfg(test)]").next().unwrap_or("");
            calls += production.matches(RESOLVER_CALL).count();
            if name == OWNER {
                continue;
            }
            for m in MARKER_TABLES {
                if src.contains(m) {
                    offenders.insert(format!("{name} contains the marker table {m:?}"));
                }
            }
        }
        // NON-VACUITY: a walk that found nothing would pass against anything.
        assert!(
            scanned >= 40,
            "the directory walk found only {scanned} .rs files — the scan is broken, not the code"
        );
        assert!(
            offenders.is_empty(),
            "a SECOND place carries the family marker table: {offenders:?}\n\
             Two resolvers that agree on the happy path and differ in the fallback is \
             the games-dir incident, and this one picks the database and the SOAP \
             namespace."
        );
        // CORRECTED 2026-08-06. The first draft matched the literal
        // `family_from_container_names(` and capped at 2, reasoning that the
        // definition contributed one occurrence. Both halves were wrong: the
        // definition is `pub fn family_from_container_names<'a>(`, so the
        // generic sits between the name and the paren and the literal never
        // matched it — an ACCIDENTAL exclusion that would silently shift the
        // count if the lifetime were ever removed — and with the real
        // production count at 0, a cap of 2 was near-vacuous (it took THREE
        // planted callers to trip it). Match the bare symbol, exclude
        // definitions explicitly with the `ends_with("fn")` idiom this repo
        // already uses in `vocab_coverage_tests`, and cap at ONE.
        //
        // The count is 0 today because no consumer exists yet on this branch;
        // the first real caller arrives with a later increment. Do not read 0
        // as "the guard is broken".
        assert!(
            calls <= 1,
            "the resolver is called from {calls} production sites. At most ONE may call it: \
             two callers with different fallbacks is exactly the games-dir incident this \
             guard exists to prevent, and this one picks the database and the SOAP namespace."
        );
    }

    /// The stripper must not be fooled by prose — this file's own doc comments
    /// name every marker above.
    #[test]
    fn the_stripper_removes_comments_and_keeps_code() {
        assert!(!strip_comments("// ac-worldserver\n").contains("ac-worldserver"));
        assert!(!strip_comments("/* -mangosd */").contains("-mangosd"));
        assert!(strip_comments("let s = \"ac-worldserver\";").contains("ac-worldserver"));
        assert!(strip_comments("let s = \"// not a comment\";").contains("// not a comment"));
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p dml-wow --lib resolver_scan`
Expected: PASS. (If it FAILS, a second resolver already exists — that is a real finding; report it rather than weakening the marker list.)

- [ ] **Step 3: Mutation — prove the walk detects a copied marker table**

Add to any other file in `crates/dml-wow/src/` (e.g. `lifecycle.rs`), at module scope:

```rust
#[allow(dead_code)]
const AC_MARKERS: &[&str] = &["ac-worldserver"];
```

Run: `cargo test -p dml-wow --lib resolver_scan`
Expected: FAIL naming `lifecycle.rs contains the marker table "AC_MARKERS"`.
Then remove it with an Edit.

- [ ] **Step 3b: Mutation — prove the call-site cap detects a second caller**

**CORRECTED 2026-08-06** — with the cap at ONE, a single planted caller is
LEGAL. Plant **two** and confirm the cap trips. Add to any other file(s) in
`crates/dml-wow/src/`, at module scope:

```rust
#[allow(dead_code)]
fn second_resolver(names: &[&str]) -> crate::family::FamilyVerdict {
    crate::family::family_from_container_names(names.iter().copied())
}
#[allow(dead_code)]
fn third_resolver(names: &[&str]) -> crate::family::FamilyVerdict {
    crate::family::family_from_container_names(names.iter().copied())
}
```

Run: `cargo test -p dml-wow --lib resolver_scan`
Expected: FAIL — "the resolver is called from 2 production sites".
Then remove both with an Edit.

- [ ] **Step 3c: Mutation — prove the definition exclusion is deliberate, not accidental**

Temporarily remove the lifetime from the definition, making it
`pub fn family_from_container_names(`. The counted total must **not** change.

Run: `cargo test -p dml-wow --lib resolver_scan`
Expected: PASS, unchanged. If it fails, the exclusion is matching on the generic
rather than on `fn` — which is the accidental exclusion this step exists to
catch. Restore with an Edit.

- [ ] **Step 4: Mutation — prove the non-vacuity floor bites**

Change `let dir = ...join("src")` to `.join("src/nonexistent")` — this should panic on `read_dir`, proving the walk is real rather than silently empty. Restore with an Edit.

- [ ] **Step 5: Commit**

```bash
git add crates/dml-wow/src/family.rs
git commit -m "test(family): exactly one place decides what kind of stack this is

Modelled on the games-dir incident: two resolvers agreed on the happy path and
differed only in the FALLBACK, so every read fell through to defaults and
answered ok:true with numbers that were not the server's -- and the 18 parity
suites structurally could not see it, because they pass explicit paths to avoid
the process env.

A second family resolver would be worse: it picks the DATABASE and the SOAP
namespace. A runtime directory walk, not a fixed file list, because the failure
mode is a second resolver arriving in a file this test never heard of.

Mutations RED: a marker planted in lifecycle.rs is named; the non-vacuity floor
catches an empty walk."
```

---

## Task 5: `Database::name()` reads the server's conf (I1)

**Files:**
- Modify: `crates/dml-wow/src/db.rs:45-70`
- Test: inline in `crates/dml-wow/src/db.rs`

**Interfaces:**
- Consumes: `CoreFamily` (Task 1) for the conf location.
- Produces:
  - `pub fn parse_database_info(line: &str) -> Option<&str>` — the schema name from a `*DatabaseInfo` value.
  - `pub fn database_names_from_conf(conf: &str) -> Option<DatabaseNames>`
  - `pub struct DatabaseNames { pub world: String, pub characters: String, pub auth: String }`
  - `pub const ERR_DB_NAMES_UNRESOLVED: &str = "DB_NAMES_UNRESOLVED";`

**Why this is a real bug today:** `db.rs:61-63` hardcodes `acore_world` / `acore_characters` / `acore_auth`, and **nothing in the repo reads `*DatabaseInfo` at all**. Any AzerothCore user who renames their databases in `worldserver.conf` breaks Dashboard, Item DB, Characters and Bots — silently, in the wrong direction.

The conf is host-readable at `<title dir>/env/dist/etc/worldserver.conf` (verified on the live server). `Playerbots` keeps its hardcoded name for now: it comes from `AC_PLAYERBOTS_DATABASE_INFO` in the compose env rather than `worldserver.conf`, and widening this task to cover it would mix two evidence sources. Record it as a follow-up.

- [ ] **Step 1: Write the failing test**

Add to `crates/dml-wow/src/db.rs`'s test module:

```rust
    /// The value is `host;port;user;pass;dbname` — the schema is the LAST
    /// field, and the password may itself contain anything, so split from the
    /// right and take one field. Never parse left-to-right by index.
    #[test]
    fn the_schema_is_the_last_semicolon_field() {
        assert_eq!(
            parse_database_info("127.0.0.1;3306;acore;pw;acore_world"),
            Some("acore_world")
        );
        assert_eq!(
            parse_database_info("\"127.0.0.1;3306;acore;p;w;d;my_world\""),
            Some("my_world"),
            "a password containing semicolons must not shift the schema field"
        );
        assert_eq!(parse_database_info(""), None);
        assert_eq!(parse_database_info("nosemicolons"), None);
    }

    #[test]
    fn conf_names_are_read_not_assumed() {
        let conf = "\
LoginDatabaseInfo     = \"127.0.0.1;3306;acore;pw;my_auth\"
WorldDatabaseInfo     = \"127.0.0.1;3306;acore;pw;my_world\"
CharacterDatabaseInfo = \"127.0.0.1;3306;acore;pw;my_chars\"
";
        let n = database_names_from_conf(conf).expect("all three keys present");
        assert_eq!(n.auth, "my_auth");
        assert_eq!(n.world, "my_world");
        assert_eq!(n.characters, "my_chars");
    }

    /// REFUSE, do not fall back. Falling back to the old constants restores the
    /// exact bug this change exists to fix, invisibly — the reader would go on
    /// answering ok:true against a database that is not the server's.
    #[test]
    fn a_missing_key_refuses_rather_than_defaulting() {
        let conf = "LoginDatabaseInfo = \"h;3306;u;p;my_auth\"\n";
        assert_eq!(database_names_from_conf(conf), None);
    }

    #[test]
    fn commented_out_keys_are_not_read() {
        let conf = "\
# WorldDatabaseInfo = \"h;3306;u;p;WRONG\"
LoginDatabaseInfo     = \"h;3306;u;p;a\"
WorldDatabaseInfo     = \"h;3306;u;p;w\"
CharacterDatabaseInfo = \"h;3306;u;p;c\"
";
        let n = database_names_from_conf(conf).expect("all three present");
        assert_eq!(n.world, "w", "a commented key must not win");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p dml-wow --lib db::`
Expected: FAIL to compile — `cannot find function parse_database_info`.

- [ ] **Step 3: Implement the parser**

Add to `crates/dml-wow/src/db.rs`:

```rust
/// Error code when the server's conf could not answer.
pub const ERR_DB_NAMES_UNRESOLVED: &str = "DB_NAMES_UNRESOLVED";

/// The three schema names, read from the server's own conf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseNames {
    pub world: String,
    pub characters: String,
    pub auth: String,
}

/// The schema name out of a `host;port;user;pass;dbname` value.
///
/// SPLIT FROM THE RIGHT. A password may contain semicolons, and indexing from
/// the left would then shift the schema field and connect to whatever happened
/// to land at index 4.
pub fn parse_database_info(line: &str) -> Option<&str> {
    let v = line.trim().trim_matches('"');
    let (_, last) = v.rsplit_once(';')?;
    let last = last.trim();
    if last.is_empty() { None } else { Some(last) }
}

/// Read all three names, or refuse. Never a partial answer and never a default.
pub fn database_names_from_conf(conf: &str) -> Option<DatabaseNames> {
    fn value_of<'a>(conf: &'a str, key: &str) -> Option<&'a str> {
        conf.lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == key).then_some(v)
            })
    }
    Some(DatabaseNames {
        auth: parse_database_info(value_of(conf, "LoginDatabaseInfo")?)?.to_string(),
        world: parse_database_info(value_of(conf, "WorldDatabaseInfo")?)?.to_string(),
        characters: parse_database_info(value_of(conf, "CharacterDatabaseInfo")?)?.to_string(),
    })
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p dml-wow --lib db::`
Expected: PASS.

- [ ] **Step 5: Route `Database::name()` through it**

Replace `Database::name()`'s constants with a lookup against a resolved `DatabaseNames`, keeping the enum. Change the signature to `pub fn name(self, names: &DatabaseNames) -> &str` and update every call site the compiler reports. `Database::Playerbots` keeps its literal `"acore_playerbots"` with a comment naming the follow-up.

Run `cargo build -p dml-wow` and fix each error the compiler names — that list IS the call-site inventory, which is exactly why the enum stays.

- [ ] **Step 6: Prove it against the live server**

The user's stack is up. With `DML_GAMES_DIR=C:\Users\perzi\dml-native`:

Run: `cargo test -p dml-wow --tests -- --nocapture`
Expected: the 18 parity suites RUN (not SKIP) and pass. Any suite reporting SKIP means the live gate did not actually exercise this.

- [ ] **Step 7: Mutation — prove the refusal is load-bearing**

Change `database_names_from_conf`'s `?` on the `WorldDatabaseInfo` lookup to a fallback:

```rust
        world: value_of(conf, "WorldDatabaseInfo").and_then(parse_database_info).unwrap_or("acore_world").to_string(),
```

Run: `cargo test -p dml-wow --lib db::`
Expected: FAIL on `a_missing_key_refuses_rather_than_defaulting`.
Then restore with an Edit.

- [ ] **Step 8: Full workspace gate**

Run: `cargo test --workspace`
Expected: 1667 + your new tests, 0 failed.

- [ ] **Step 9: Commit**

```bash
git add crates/dml-wow/src/db.rs
git commit -m "fix(db): schema names come from the server's conf, not from constants

db.rs hardcoded acore_world/acore_characters/acore_auth and NOTHING in the repo
read *DatabaseInfo at all -- so any AzerothCore user who renamed their
databases in worldserver.conf had Dashboard, Item DB, Characters and Bots all
reading schemas that do not exist, silently.

Split from the RIGHT: a password may contain semicolons, and indexing from the
left would shift the schema field and connect to whatever landed at index 4.

A missing key REFUSES (DB_NAMES_UNRESOLVED) rather than falling back. Falling
back restores the bug invisibly, which is worse than an error because nothing
looks broken.

Mutation RED on reintroducing the fallback. Parity suites run live against the
AzerothCore server, zero SKIP."
```

---

## Self-Review

**Spec coverage.**

| Spec section | Task |
|---|---|
| §4 what the seam carries — enum, exclusions | 1 |
| §5 code location, `family.rs` in `dml-wow` | 1 |
| §5 resolution: compose inference, refusal | 1 |
| §5 mirror obligation | 2 |
| §2 ruling: hide not delete | 3 |
| §6 I0 (family + catalog + filter) | 1–4 |
| §6 I1 (`Database::name()` conf-resolved) | 5 |
| §7 `TITLE_FAMILY_UNKNOWN` | 1 |
| §7 `DB_NAMES_UNRESOLVED` | 5 |
| §8 mutation proof per increment | every task |
| §8 source scans for wiring | 3 |
| §8 structural guard against a second resolver | 4 |
| §5 install-record resolution step | **NOT covered — see gap below** |
| §6 I2–I5 | Deferred by design; see Scope |
| §7 intent × family coverage, console-safe refusal | Deferred with I3 |
| §7 bot-prefix refusal | Deferred with I4 |

**Gap found and accepted:** spec §5 step 1 (the `.dml-install.json` `family` field) has no task here. It is unreachable until something *writes* an install record with a family, which is `install_native`'s job in sub-project #2 — and the user's own server has no record at all, so step 2 is the live path. Adding a reader for a field nothing writes would be untestable ceremony. **This is recorded as owed by sub-project #2** and the spec's §5 ordering is unchanged.

**Placeholder scan:** no TBD/TODO; every code step carries real code; no "similar to Task N".

**Type consistency:** `FamilyVerdict` / `CoreFamily` / `family_from_container_names` used identically in Tasks 1 and 4. `TitleRow.family` (Rust, Task 2) and `_title_registry` field 6 (bash, Task 2) and `TitleInfo.family` (TS, Task 3) all carry the same three string values. `DatabaseNames` field names (`world`/`characters`/`auth`) match between Task 5's struct, its parser and its tests.

---

## Owed after this plan

- **I2–I5**, their own plan, written against Task 1's real API.
- `Database::Playerbots` conf-resolution (Task 5 note).
- The `.dml-install.json` `family` field, with sub-project #2.

### Task 6 — the qualified-literal half of I1 (raised by Task 5's review, 2026-08-06)

Task 5's review confirmed that `Database::name()` sets only the **connection's
default schema**, so I1 as built fixes 1½ of the four features the spec names.
Every SQL string that qualifies a table with a literal schema is untouched, and
those are the majority of the DB surface:

| Feature | After Task 5 | Why |
|---|---|---|
| Item DB | fixed | `pages.rs` `FROM item_template` — unqualified |
| Characters (list) | fixed | unqualified |
| Characters (Accounts, paperdoll) | **still broken** | `pages.rs` `FROM acore_auth.account`; `paperdoll.rs` `JOIN acore_world.item_template` |
| Dashboard | **still broken** | `stats.rs` — `acore_playerbots.…`, `acore_auth.account` |
| Bots | **still broken** | `botid.rs` — same two literals |

29 dotted literals across 8 files (`botid` 7, `stats` 5, `migrate` 5, `pages` 4,
`moduletail` 4, `paperdoll` 2, `unbound` 1, `config` 1), plus the undotted
schema lists in `backup.rs`'s dump set and `modmgr.rs`.

Two of those are worse than a broken read. `backup.rs`'s dump list would
silently **dump nothing** on a renamed server — a backup that reports success
and contains no data. `migrate.rs`'s emptiness guard would answer
`MIGRATE_TARGET_UNKNOWN` and refuse, which is the safe direction but for the
wrong reason.

The failure mode is the same silent-wrong-direction class I1 exists to close,
merely relocated from connect-time to query-time: a renamed world DB makes
paperdoll raise `DbError::Query`, which `db_err_to_cmd` collapses into
`DB_UNREACHABLE` — the launcher then says "Is ac-database running?" about a
server that is up and healthy.

**Also owed here: the bash mirror.** Root `CLAUDE.md`'s rule is "a fix on ONE
surface only half-ships", and bash does not merely lag — it *contradicts*.
`cli/src/30-db.sh:26-28` hardcodes the three names and **line 41 is an
allowlist** (`acore_world|acore_characters|acore_auth) ;;`) that would REFUSE any
conf-resolved name. Same in `cli/src/48-stats.sh:60` and
`cli/src/60-backup.sh:53-54`. Either bash gets the same resolution or the
divergence gets a stated reason recorded in `cli/CLAUDE.md` — silence is not an
option, because today the two surfaces answer different questions about the same
server.

**Contract surface:** `DB_NAMES_UNRESOLVED` is absent from `docs/cli-contract.md`
and from every frontend consumer, so the launcher currently renders it as a bare
unknown code. Fix with this task.
