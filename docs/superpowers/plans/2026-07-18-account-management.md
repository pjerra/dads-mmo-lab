# Account Management Implementation Plan (Round H)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `wow account create|set-password|set-gm` (SOAP), `gm_level` on the accounts list, and an Accounts page.

## Global Constraints

- Branch `feat/dml-launcher-windows`; NO merge. `cli/dml` committed artifact. `set -euo pipefail`; NO `local` in dispatch arms.
- Validators BEFORE any command-string build (console is space-delimited; sets exclude whitespace/quotes/XML chars): `_valid_account_user` `^[A-Za-z0-9_]{3,20}$`; `_valid_account_pass` `^[A-Za-z0-9_@#%+=!-]{4,16}$`; level `^[0-3]$`. Exact console commands: `account create U P` / `account set password U P P` / `account set gmlevel U N -1`.
- rc mapping identical to soap-exec (0 ok / 2 SOAP_FAULT with fault text / 3 SOAP_AUTH / 4 SOAP_UNREACHABLE). No delete verb.
- Accounts list: SELECT gains `COALESCE(g.gmlevel,0)` as column 3 (right after username) via `LEFT JOIN (SELECT id, MAX(gmlevel) AS gmlevel FROM acore_auth.account_access GROUP BY id) g ON g.id = a.id`; `_accounts_rows_to_json` (find it — it parses the TSV positionally) emits `"gm_level":N` per account; existing fields unchanged (additive).
- UI copy exact: GM-3 confirm `Level 3 grants full admin including SOAP. Continue?`; badge `GM <n>`.
- Gates: full bats; `npm test`; `npm run check`; `cargo test`. Entering H: bats 351, vitest 37, cargo 25, check 0/0. Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

### Task 1: CLI verbs + gm_level + bats

**Files:** `cli/src/90-main.sh` (validators near `_valid_charname`; `account)` arm after `accounts)`; accounts SELECT + helper change); create `cli/tests/wow-account.bats`. Commit regenerated `cli/dml`.

- [ ] **Step 1: bats first** (`cli/tests/wow-account.bats`; setup like soap tests: make_fixture + use_curl_stub + use_mysql_stub + HOME; a soap-ok fixture `resp.xml` with `<result>ok</result>` body built like wow-console.bats does):

```bash
@test "account create: happy path sends exact console command" — DML_STUB_CAPTURE; run wow account create --user Kiddo --pass secret1 --json → status 0, .data.created true; grep -q 'account create Kiddo secret1' capture.
@test "account create: invalid user / pass rejected before SOAP" — user 'ab' (too short), user 'has space' (quote it), pass 'x' (short), pass 'bad pass' → all status 1 BAD_ARG; capture file absent.
@test "account set-password: exact command with doubled pass" — → grep 'account set password Kiddo newpass1 newpass1'.
@test "account set-gm: exact command with -1 realm" — --level 2 → grep 'account set gmlevel Kiddo 2 -1'; --level 5 → BAD_ARG; --level abc → BAD_ARG.
@test "account create: SOAP fault surfaces as SOAP_FAULT" — soap-fault.xml fixture → status 1, error.code SOAP_FAULT.
@test "account create: unreachable -> SOAP_UNREACHABLE" — DML_STUB_CURL_EXIT=7 → status 1.
@test "accounts list: gm_level joined" — mysql stub rows: '1\tADMIN\t3\t\t\t\n2\tKid\t0\t7\tHypeer\t80\n' → .data.accounts[0].gm_level 3, [1].gm_level 0, [1].characters[0].name Hypeer (existing fields intact).
```

Write these as full bats tests (7).

- [ ] **Step 2: run — FAIL. Step 3: implement.** Validators (beside `_valid_charname` in 90-main.sh):

```bash
_valid_account_user() { [[ "$1" =~ ^[A-Za-z0-9_]{3,20}$ ]]; }
_valid_account_pass() { [[ "$1" =~ ^[A-Za-z0-9_@#%+=!-]{4,16}$ ]]; }
```

`account)` arm (after the `accounts)` arm's `;;`, inside the `wow` case):

```bash
      account)
        asub="${1:-}"; shift || true
        auser=""; apass=""; alevel=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --user) _need_flag_val "$1" $#; auser="$2"; shift 2 ;;
            --pass) _need_flag_val "$1" $#; apass="$2"; shift 2 ;;
            --level) _need_flag_val "$1" $#; alevel="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        _valid_account_user "$auser" || { json_err BAD_ARG "Invalid username (3-20 letters/digits/_)" ""; exit 1; }
        case "$asub" in
          create|set-password)
            _valid_account_pass "$apass" || { json_err BAD_ARG "Invalid password (4-16 chars, letters/digits/_@#%+=!-)" ""; exit 1; }
            if [[ "$asub" == create ]]; then acmd="account create $auser $apass"
            else acmd="account set password $auser $apass $apass"; fi
            ;;
          set-gm)
            [[ "$alevel" =~ ^[0-3]$ ]] || { json_err BAD_ARG "--level must be 0-3" ""; exit 1; }
            acmd="account set gmlevel $auser $alevel -1"
            ;;
          *) json_err UNKNOWN_COMMAND "Unknown account subcommand: $asub" "Try: dml wow account create|set-password|set-gm --json"; exit 1 ;;
        esac
        if out="$(soap_exec "$acmd")"; then rc=0; else rc=$?; fi
        case "$rc" in
          0)
            case "$asub" in
              create) json_ok "{\"created\":true,\"user\":\"$auser\"}" ;;
              set-password) json_ok "{\"password_set\":true,\"user\":\"$auser\"}" ;;
              set-gm) json_ok "{\"gm_set\":true,\"user\":\"$auser\",\"level\":$alevel}" ;;
            esac ;;
          2) json_err SOAP_FAULT "$(_soap_text_decode "$out")" "The worldserver rejected the command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach SOAP at $(soap_url)" "Is the worldserver running?" ; exit 1 ;;
        esac
        ;;
```

Accounts list: change the SELECT to

```
SELECT a.id, a.username, COALESCE(g.gmlevel,0), COALESCE(c.guid,''), COALESCE(c.name,''), COALESCE(c.level,'')
FROM acore_auth.account a
LEFT JOIN (SELECT id, MAX(gmlevel) AS gmlevel FROM acore_auth.account_access GROUP BY id) g ON g.id = a.id
LEFT JOIN characters c ON c.account = a.id
WHERE a.username NOT LIKE 'RNDBOT%' AND a.username <> 'AHBOT'
ORDER BY a.id, c.level DESC;
```

and update `_accounts_rows_to_json` (grep for it; it parses the TSV positionally) to read the new column 3 and emit `"gm_level":N` on each account object (default 0 when non-numeric). Existing fields/order otherwise unchanged.

- [ ] **Step 4: rebuild; new file 7/7; FULL — expect 358 (351 + 7).** If any pre-existing accounts test pins the old 5-column stub row shape, update ONLY its fixture rows (report it). **Step 5: commit** `feat(cli): wow account create/set-password/set-gm + gm_level in accounts list`.

---

### Task 2: Rust + api.ts

- lib.rs (after `wow_accounts`): `wow_account_create(user: String, pass: String)`, `wow_account_set_password(user: String, pass: String)`, `wow_account_set_gm(user: String, level: u8)` — argv per the CLI (`["wow","account","create","--user",user,"--pass",pass]` etc.; set-gm sends `--level` as `level.to_string()`); all run_json_cmd; register all three.
- api.ts: `Account` gains `gm_level: number`; wrappers `wowAccountCreate(user, pass)`, `wowAccountSetPassword(user, pass)`, `wowAccountSetGm(user, level)` with the obvious return shapes.
- Gates: cargo 25, vitest 37, check 0/0. Commit `feat(launcher): account management commands`.

---

### Task 3: Accounts page + nav

**Files:** Create `launcher/src/lib/pages/Accounts.svelte`; `nav.ts` (Server section gains `{ id: "accounts", label: "Accounts" }` after `console`), `nav.test.ts` (ids pin gains `"accounts"` after `"console"`), `+page.svelte` (import + mount).

**Binding requirements** (read ModuleManager/GMTools for the confirm + inline-action patterns):
- List from `wowAccounts()`: per row — username (bold), `#<id>` muted, `GM <n>` badge (gold-ish) when `gm_level > 0`, character names as a muted comma line.
- **Create account** card: username + password inputs with client-side regex mirrors (`/^[A-Za-z0-9_]{3,20}$/`, `/^[A-Za-z0-9_@#%+=!-]{4,16}$/`) + inline hints (`3-20 letters, digits or _` / `4-16 chars; no spaces`); Create disabled until both match; success note `Account <user> created.`; errors inline (SOAP_FAULT message as-is — that's where "already exists" surfaces).
- Per-row actions: `Set password` toggle revealing password input + Apply (same client validation); `GM level` select 0-3 + Apply — selecting 3 requires a two-step confirm with copy exactly `Level 3 grants full admin including SOAP. Continue?`.
- Single busy flag; refresh after every successful action; per-action inline errors, never a page error card (except the initial list load failing → the standard error-card).
- vitest: nav pins (+ ids order). Gates: `npm test` (37, nav pin updated in place) + `npm run check` (0/0). Commit `feat(launcher): Accounts page — create, password, GM level`.
