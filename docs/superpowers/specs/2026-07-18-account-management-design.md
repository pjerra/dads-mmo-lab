# Account Management — Design Spec (Round H)

**Date:** 2026-07-18 · **Branch:** `feat/dml-launcher-windows` · Design review waived (standing instruction). First item of the post-gap-analysis shortlist.

## What & why

Create accounts, set passwords, and grant GM levels from the launcher — the one
family-LAN essential neither The Lab nor the DML manager put in their UI (both defer to
the worldserver console). Everything rides the existing SOAP path; no new attack surface
beyond three tightly-validated console commands.

## CLI (`wow account …`, request-response, SOAP-backed like `soap-exec`)

- `wow account create --user U --pass P --json` → `.account create U P`
- `wow account set-password --user U --pass P --json` → `.account set password U P P`
- `wow account set-gm --user U --level N --json` → `.account set gmlevel U N -1` (all realms)

Validation BEFORE any command string is built (the console is space-delimited, so the
character sets exclude whitespace/quotes/XML specials entirely):
- `_valid_account_user`: `^[A-Za-z0-9_]{3,20}$`
- `_valid_account_pass`: `^[A-Za-z0-9_@#%+=!-]{4,16}$` (AC caps passwords at 16)
- level: `^[0-3]$`

rc mapping identical to `soap-exec` (0 → `json_ok` `{ok-payload per verb}`, 2 →
`SOAP_FAULT` with the fault text — this is how "account already exists" surfaces, 3 →
`SOAP_AUTH`, 4 → `SOAP_UNREACHABLE`). Server must be running (inherent to SOAP).
No account-delete verb (deliberate — destructive, absent from both references).

**`wow accounts` (existing list) additionally emits `gm_level`** per account
(LEFT JOIN `account_access`, `COALESCE(gmlevel,0)`, realm -1/any row) — additive field,
existing consumers unaffected.

## UI — new **Accounts** page (Server section, after Console)

- Account list (from the extended `wowAccounts`): username, id, `GM <n>` badge when
  `gm_level > 0`, character names line.
- **Create account** card: username + password inputs (client-side mirrors of the CLI
  regexes with inline hints), Create button → success note / inline error (SOAP_FAULT
  text shown as-is — that's where "already exists" appears).
- Per-account actions: **Set password** (inline reveal: password input + Apply) and
  **GM level** (select 0-3 + Apply; two-step confirm when granting level 3 with copy
  `Level 3 grants full admin including SOAP. Continue?`).
- All actions disabled while the world is down? No — let SOAP_UNREACHABLE surface with
  its own hint (consistent with GM Tools).
- Nav: `{ id: "accounts", label: "Accounts" }` in Server section after `console`;
  nav.test ids pin updated.

## Testing

bats (`wow-account.bats`): validation rejections for each verb (bad user/pass/level,
whitespace/injection attempts); happy paths assert the EXACT console command text via
curl capture (`account create U P`, `account set password U P P`,
`account set gmlevel U 2 -1`); fault → SOAP_FAULT passthrough; unreachable →
SOAP_UNREACHABLE; accounts-list `gm_level` join (mysql stub row shape). vitest: nav
pins. Gates: full bats, vitest, cargo, check (entering H: bats 351, vitest 37, cargo 25,
check 0/0). Live gate: create a real account, log into the client with it, grant GM 1.

Out of scope: account deletion, ban/mute, expansion locks, per-realm gmlevels, RA.
