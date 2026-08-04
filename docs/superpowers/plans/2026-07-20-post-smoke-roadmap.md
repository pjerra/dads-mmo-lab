# Post-Smoke-Test Roadmap

**Status:** planning only — no building until the user has smoke-tested.

## Context

Two days of heavy building landed a lot at once:

- **50 features are locked (untested); 17 are tested.** Everything built
  2026-07-19/20 — the second-pass features and all four improvement groups —
  has never run against a live server.
- The branch is **394 commits ahead of `main`** and has never been merged.
- The server is currently **stopped** (stopped on request after the overnight
  run).

Live testing has repeatedly been where the real bugs surfaced (the stale `dml`
backend that silently broke Bot World; the 30-minute world-restart hang; the
character-reset-on-restart bug). So the sequencing below puts verification
first and treats "fix what testing finds" as a funded round, not an
afterthought.

---

## Round 0 — Before the smoke test (docs only, no code)

Do this first; it unblocks the test itself.

1. **SMOKE-TESTS.md navigation is stale.** It was rewritten for the tabbed
   layout ("Settings → Bot World tab", "Bots → My Party tab"), but the sidebar
   redesign promoted those tabs to sidebar dropdown children. Rewrite the step
   wording for the accordion sidebar (e.g. "Config ▸ Bot World").
2. **Check every new feature has a row.** The overnight features + the four
   improvement groups added flags; confirm §21+ covers each locked flag, and
   that no flag exists without a row (and vice versa).
3. *(Optional, do before sharing)* **README + FEATURES.md are out of date** —
   both predate ~50 features and the sidebar redesign. Anyone you send the
   project to today reads a stale description.

## Round 1 — Fix what the smoke test surfaces

Reserve real capacity here. Historically this is the highest-value round: live
testing finds what stubs cannot. Expect the locked flags to flip in batches as
rows pass.

## Round 2 — Known polish tail (small, already identified)

### Server-required UX (user request, 2026-07-21)

Many pages need the server running (Console, Players-online, GM tools, live
config-apply, item DB, Bot Browser, etc.); with the server stopped they error
or show empty/broken states. User wants either (a) grey out those sidebar
items when the server is down, or (b) let you open the page and greet you with
a clear "start the server first" message.

**DECIDED (user, 2026-07-21): (b) the friendly in-page greeting**, not
grey-out. Reasons: grey-out is confusing (no reason shown, looks broken),
blocks exploring what a page does, and needs the same per-page metadata anyway;
the greeting is discoverable and can carry a **Start server** button right
there, and several pages already do a rough version (Console's offline note,
Home's start card).

Build plan: a shared `<ServerRequired>` placeholder (message + Start button),
shown reactively off the existing `server-status` store when the verdict is not
`online`. Gate ONLY the pages that are useless while stopped — Console,
Character view, Teleport, GM Tools, Item Database, My Party, Bot Browser,
Accounts (all need the DB/SOAP up). Do NOT blanket-gate the mixed/offline-ok
pages — Config (file edits apply on next start), Modules/Library install,
Tools disk tools, Backups' Verify, Help — they stay usable stopped (they can
keep their own inline notices where a specific action needs the server).
Small-medium, frontend-only, no new flag (read-only UX). Not started — waiting
on the user's "build now vs after smoke testing".


Concrete items deferred from the improvements review, all low-severity but
real:

- `world-restart` precondition is broader than the hang requires — it also
  refuses a valid crashed-world / healthy-DB recovery restart.
- `party online` interpolates class/level into JSON unguarded — the same
  latent invalid-JSON bug already fixed in `players online`.
- Keep-awake never re-engages after it releases on a poll-failure streak, even
  once polling recovers while still online.
- The MySQL-expose generated script lacks the elevation self-check that the
  shrink-disk script got.
- Cosmetics: "1 bots" pluralization in the backup summary; taskbar progress
  isn't wired into every long streamed op; the Teleport empty state blames the
  filter even when the fetch errored.
- Spec picker/validator drift for non-lowercase custom spec names (the picker
  can offer a name `_valid_bot_spec` then rejects).

### Internet play breaks LAN players unless the router hairpins (user request, 2026-07-27)

`lan --internet on <addr>` sets only `realmlist.address`, so once the realm
advertises a public IP/hostname, players on the *home* network are also sent
out to the public address and only reach the world server if the router
supports NAT hairpinning. Many consumer routers don't; the symptom is "login
works, realm select hangs at Connecting" for everyone on the LAN while outside
friends are fine. Today the only fix is a hand-written UPDATE. The tooling
should cover it.

Fix shape: on the `--internet on` path also set
`realmlist.localAddress = <host LAN IP>` and `localSubnetMask = 255.255.255.0`;
on `off`/revert restore `localAddress = 127.0.0.1` (the stock default). AC
hands the local address to any client whose IP is inside that subnet, and to
loopback clients when neither address is loopback — so the host PC and every
LAN machine get the LAN IP while outside clients still get the public one. No
hairpin needed. **Verify against the deployed AC version before building**
(`localAddress`/`localSubnetMask` columns present, and the client-address
selection behaves as described) — this was reasoned from AC's
`Realm::GetAddressForClient`, not read off the live DB.

Notes that drive the effort:

- **The CLI cannot detect the LAN IP itself.** It runs inside `dml-arch`, where
  `hostname -I` returns the WSL2 NAT address (172.x), not the Windows host's
  LAN address. The launcher already detects the right value (`lanIp`, shown in
  step 1 of the Tools card) and must pass it down — new optional flag, e.g.
  `lan <title> --internet on <addr> --local <lan-ip>`. With the flag absent,
  keep today's behaviour rather than guessing.
- **Three surfaces, not one:** bash [`cli/src/90-main.sh`](cli/src/90-main.sh)
  (`_lan_set`), the Rust port [`crates/dml-wow/src/lan.rs`](crates/dml-wow/src/lan.rs)
  (already ported — parameterized UPDATE), and the Tools card's step 4/6
  wiring. The `lan` arm has a documented contract, so
  [`docs/cli-contract.md`](docs/cli-contract.md) needs the new flag too, and
  the bash/Rust parity tests must stay green.
- Plain LAN play (`lan on <lan-ip>`, no `--internet`) is unaffected — it works
  today precisely because `localAddress` stays at the 127.0.0.1 default.
- Still a realmlist-only write; does not touch character data or extend the
  sanctioned-write list.

### Eluna bridge should deploy itself on server start (user request, 2026-07-27)

Today enabling My Party / GM bridges is a three-step dance, and the middle
step is only there by inheritance:

1. the server must already be **running** — `bridge_setup_stream` opens with a
   SOAP `server info` preflight and bails `SOAP_UNREACHABLE` otherwise
   (`crates/dml-wow/src/bridge.rs:129-147`);
2. the `.lua` files are copied into
   `<server>/env/dist/etc/modules/lua_scripts`;
3. the server must be **restarted** so ALE loads them.

That preflight is not a requirement of the work — the deploy is a pure file
copy that needs no live server. It exists because the Rust arm is a
byte-parity port of the bash `bridge-setup` arm, and the bash arm was written
as an on-demand action. Nobody re-litigated the workflow during the port; the
doctrine was "reproduce bash exactly". That is the honest answer to "why
haven't we done this before" — it was never decided, only inherited.

**Fix shape:** run `deploy_scripts` as a step inside `games start`/`restart`,
*before* the containers come up, and drop the SOAP preflight from that path.
The restart is already happening, so the whole dance collapses to nothing.

Arguments for: it is already idempotent (`deploy_scripts` content-compares
each file and copies only on mismatch, so a no-change start writes nothing);
it fixes a real silent-staleness bug where a DML update ships changed bridge
scripts but the deployed copies stay old until someone remembers to redeploy;
and it removes a step users forget, which currently presents as "My Party
silently does nothing".

Arguments against / open decisions: it would overwrite a hand-edited deployed
script (true of the manual button too, just rarer); it writes into the server
directory on every start, which is a side effect nobody asked for; the copy
failure path must be **non-fatal** or a non-essential step can fail `start`;
and it needs a decision for the `mod-ale`-not-installed case — copy anyway
(harmless, the scripts just sit unused) or skip. Keep the manual "Deploy
server bridges" button as an explicit redeploy either way. Worth a short
design pass rather than a straight-to-code change.

### Lua modules tab: show the ALE requirement, don't hide the list (user request, 2026-07-27)

Half of this already exists. `ale_ready` is on the module-catalog envelope
(`modules.rs:342`, literally `cpp_installed("mod-ale")`), and
`ModuleManager.svelte:790` already swaps the whole "Lua scripts (ALE)" list
for a note: *"Install the ALE module (mod-ale) first — it's in the C++ modules
list above."*

What the user actually asked for is the difference: **grey the lua rows out
instead of hiding them** (so you can see what you would get), and put an
**Install ALE** button right there instead of "scroll up and find it".

Note the precedent above ("Server-required UX", DECIDED 2026-07-21): grey-out
was rejected there for being confusing and un-explanatory, in favour of a
clear message carrying the fixing action as a button. The same reasoning
mostly applies here — but this case differs in one way that argues for
showing the rows: the lua list is a *catalog of things you could install*, so
hiding it hides the reason to install ALE at all. Suggested resolution: keep
the explanatory note, render the rows disabled beneath it, and add the
one-click **Install ALE** button. Frontend-only, small; the data is already
plumbed.

### Item Database: hover tooltips + icons, and a better mail recipient (user request, 2026-07-27)

Three asks on the Items page ([`launcher/src/lib/pages/Items.svelte`](launcher/src/lib/pages/Items.svelte)).

**1. Wowhead hover tooltip + icon on each result row.** Today the results
table is plain text (name coloured by quality, quality/ilvl/reqlvl columns)
with a 🔗 button that opens Wowhead in an external browser (`Items.svelte:183-201`).

The important finding: **all the machinery already exists and is live on the
Character Sheet — this is a reuse job, not a new integration.** Round E built
`wow item-info` ([`cli/src/46-iteminfo.sh`](cli/src/46-iteminfo.sh), Rust port
`dml::iteminfo`), surfaced as `wowItemInfo(entries) -> ItemInfo[]`
([`api.ts:742-773`](launcher/src/lib/api.ts#L742-L773)). It returns
`icon_b64` (base64 JPEG), `wowhead.tooltip` HTML, and a `tooltip_html` local
fallback rendered from `item_template`, disk-cached in `~/.dml/wowhead-cache`.
Consequences worth knowing before scoping:

- **No CSP change and no remote JS.** The fetch happens host-side in
  Rust/bash and icons arrive as base64, so nothing loads from
  `wow.zamimg.com` into the webview. (This is why the Wowhead-widget
  approach — `power.js` + `whTooltips` — should *not* be used: it would put
  third-party JS in the launcher's webview alongside the Tauri IPC bridge.)
- **Custom items already work.** `source: "wowhead" | "local" | "unavailable"`
  — server-custom entries (the Casino/Gasino 990000 NPC's items, module-added
  gear) fall back to a locally-rendered tooltip instead of breaking, and
  `sanitizeTooltipHtml` ([`$lib/tooltip`](launcher/src/lib/tooltip.ts)) already
  guards the `{@html}` path.
- **Offline degrades cleanly** — `unavailable` just leaves today's plain row.
- **No CLI contract change.** `items search` already returns `entry` (and
  `displayid`), which is all `wowItemInfo` is keyed on. Pure frontend work.

The actual work is an **extraction**: the hover tooltip — positioning, edge
flip, post-render vertical clamp, and the plain-text-upgrades-when-the-batch-lands
behaviour — currently lives inline in
[`CharacterSheet.svelte`](launcher/src/lib/CharacterSheet.svelte) (~408-500,
741-746, styles ~800-813). Pull it into a shared component and mount it on
both pages. **Refactor risk:** CharacterSheet is live-verified, so the
extraction must not regress the paperdoll — that, not the Items page wiring,
is where the care goes. Also respect the existing batch ceiling (the
base64-icon payload is why one exists; see the note at `CharacterSheet.svelte:19`)
— fetch info for the rows actually on screen, not every search hit.

**2. Move the mail recipient to the top of the tab.** Today the recipient
picker only appears in the send box at the *bottom* of the page, after you've
clicked Send on a row (`Items.svelte:269-292`), and the gear-set "Mail to…"
row has a *second*, independent picker (`mailSetTo`, `Items.svelte:226`).
Proposed: one recipient selector at the top of the page, chosen once and
reused by every send on the page (single item and gear set), so mailing five
items to the same kid doesn't mean re-picking five times. **Confirm with the
user before building** — this unifies the two pickers, which is a real
behaviour change for the gear-set row.

**3. Let the recipient be typed, not just picked.** `CharPicker` is two
dropdowns (account → character) built from `wowAccounts()`
([`CharPicker.svelte:118-131`](launcher/src/lib/CharPicker.svelte#L118-L131)) —
there's no way to just type a name you already know, which is tedious with
many accounts and awkward for characters that aren't easy to find in the list.

Cheapest shape that keeps both modes: a `<input list=…>` combobox backed by a
`<datalist>` of the known characters — you can type freely *and* get
autocomplete. Two things it must carry:

- **Same validation.** CharPicker filters to `^[A-Za-z0-9_]{1,12}$` on
  purpose (`CharPicker.svelte:24`) because every action verb enforces it and
  an unfiltered name fails later with an opaque `BAD_ARG`. Free text must
  apply that regex client-side, before the call.
- **A real error path.** The dropdown made a wrong recipient impossible;
  typing makes typos the *normal* failure. A name that passes the regex but
  doesn't exist must surface the SOAP/CLI error clearly on the page rather
  than looking like a successful send. Worth deciding whether an unknown name
  warns before sending or just reports afterwards — mail to a nonexistent
  character is silently lost in-game.

Frontend-only across all three; no new sanctioned writes (mail already goes
through the existing `mail-item` verb and its feature lock).

### The release build is not self-configuring — it needs 4 env vars nobody sets (found live, 2026-07-27)

**This is the highest-ranked item in Round 2.** It is the difference between
the release exe working out of the box and appearing broken, and it was found
the first time the built exe was double-clicked rather than started from a
wrapper script.

Symptoms, in the order they appear:

1. The status card says the server is **offline** while it is demonstrably
   running. Not a bug: `DML_BACKEND` unset means `Backend::Wsl`
   (`crates/dml-core/src/backend.rs`), so the launcher asks the bash CLI inside
   the `dml-arch` distro — a DIFFERENT install, which really is stopped
   (verified: `dml status --json` → `wow-server-playerbots:stopped`). The
   launcher reports the truth about the wrong server.
2. With `DML_BACKEND=native` alone, "a lot of features" still fail, because the
   working dev-mode script sets FOUR variables, not one:
   `DML_BACKEND`, `DML_GAMES_DIR`, `DML_SCRIPT`, `DML_YQ_BIN`. `DML_SCRIPT` is
   the load-bearing one — the not-yet-ported features (install, some module
   operations, self-update) still shell the bash script, and the Eluna bridge
   derives its lua source root from `<parent of DML_SCRIPT>/lua`. A packaged
   install started from the Start menu or a taskbar pin has none of these.

The UI can already *read* the mode (`backend_mode` command; `Backups.svelte`
and others branch on it) but there is **no way to change it from inside the
app**, and no auto-detection. So the user's only recourse is an env var they
have no reason to know exists.

Fix shape, roughly in order of value:

- **Auto-detect the backend** instead of defaulting blindly: if a native title
  directory exists (`%USERPROFILE%\dml-native` or the configured games dir) and
  Docker Desktop is present, prefer Native; else WSL. Keep `DML_BACKEND` as an
  explicit override that always wins.
- **Derive the other three** rather than requiring them. `DML_GAMES_DIR` and
  `DML_YQ_BIN` already have launcher-side fallbacks; `DML_SCRIPT` should fall
  back to a `cli/dml` bundled with the installed app (the packaging question:
  the release currently ships no copy of the bash script at all, which is why
  it cannot self-heal).
- **Surface the mode in the UI**: a backend row in Settings (read-only at
  minimum, switchable ideally) and a status card that names which backend it is
  reporting on — "WSL install: stopped" beats a bare "offline".
- Until then, `start-launcher-native.bat` at the repo root sets all four for the
  built exe (release-build sibling of the desktop dev script), and the four vars
  have been set persistently for this user so a pinned/installed shortcut works.

Ranking note: everything else in Round 2 is polish. This one determines whether
a tester who installs the app sees a working product or a broken one, so it
should land before any shareable-release round.

### System-tray presence for the launcher (user request, 2026-07-27)

The user wants to reach the launcher from the **system tray** — the way the
old closed-source C# tray worked (CLAUDE.md still notes the legacy top-level
`list`/`status`/`start` text output exists precisely because "the old C# tray
parses it"). Today the Tauri launcher has **no tray icon at all**: `tauri` is
declared with `features = []`, so the `tray-icon` feature is off, and
`tauri.conf.json` has no `trayIcon` block. Closing the window exits the app,
so there is nothing to go back into.

Everything needed is already present — Tauri 2 has first-class tray support and
the bundle already ships the icon set (`icons/icon.ico` for Windows).

Fix shape:

- Enable the `tray-icon` feature on the `tauri` dependency and build a
  `TrayIconBuilder` in the `setup` hook.
- Menu: **Open DML Launcher**, a status line, **Start / Stop server**, **Quit**.
  Left-click shows/focuses the window.
- Intercept the window close event and **hide instead of exit**, so the app
  keeps living in the tray — this is the behaviour that makes "go into the
  launcher from the tray" true. Quit stays available from the tray menu.
- Reflect server state in the tray (icon variant and/or tooltip) off the
  existing `server-status` store, so the tray answers "is my server up?"
  without opening the window. That was the old C# tray's main value.

Open decisions: whether close-to-tray is the default or a setting (some users
expect close to mean close); whether to offer start-with-Windows, which is the
usual companion to a tray app but is a system-level change; and whether the
tray's Start/Stop reuse the same confirmation flow the Home card uses.

Small-to-medium, launcher-only, no new backend surface. Pairs naturally with
the release-build self-configuration item above: a tray app that launches at
login is exactly the case where "needs four env vars set by a wrapper script"
breaks down.

### Installer: offer Defender exclusions for the build folders (user request, 2026-07-27)

> **PARTLY SHIPPED 2026-07-28** (commit `45e222a`, merged from
> `feat/round2-launcher-batch`): the installer now offers an opt-in exclusion
> for **`$InstallRoot`** and the uninstaller removes it again, with the
> read-back verification and non-fatal handling below. What is NOT shipped is
> the half this entry opens with — the **build-tool** exclusions
> (`cargo`/`rustc`/`link`/`node` + `target/`), deliberately left manual because
> they only help someone building from source. If the cargo-rebuild drag is
> what you wanted fixed, that part is still open.

Defender scans every file `cargo` writes, which is a measurable drag on
`target/`-heavy rebuilds. The fix is two `Add-MpPreference` calls, but they
need elevation — so today it's a manual "open an admin terminal and paste"
step that nobody will remember on the next machine.

[`Install-DML.ps1`](guides/DML-Windows/Install-DML.ps1) is the right home: it
already carries `#Requires -RunAsAdministrator` (line 1) and already does
admin-only host work (portproxy + firewall, ~2726-2762). Adding the step
there costs the user **no extra UAC prompt** — the elevation is already spent.
Note there is no way to avoid that first prompt entirely; nothing unelevated
can grant itself admin. Exclusions are permanent once set, so this is a
one-time action, not something to re-apply on a schedule.

Design constraints:

- **Opt-in, with the tradeoff stated.** This narrows the user's AV coverage.
  An installer that does that silently is misbehaving — prompt, explain what
  is excluded and why, and default to *no* if unattended.
- **Two different audiences.** `cargo.exe`/`rustc.exe`/`link.exe`/`node.exe`
  plus the repo's `target/` only help someone building from source. A plain
  DML user never runs cargo; for them the candidate exclusions are the WSL
  vhdx and the games directory, which is a *separate* decision and should not
  be bundled in by default.
- **Compute the paths.** The repo location is per-user; the installer already
  knows `$InstallRoot`. Nothing hardcoded.
- **Non-fatal.** Tamper Protection can make `Add-MpPreference` fail even when
  elevated. On failure, warn with the Windows Security GUI route (Virus &
  threat protection → Manage settings → Exclusions) and carry on — an
  optional perf tweak must never fail the install.
- **Verify by read-back** (`Get-MpPreference -ExclusionPath`) rather than
  trusting the call's exit.
- **Uninstall symmetry.** [`Uninstall-DML.ps1`](guides/DML-Windows/Uninstall-DML.ps1)
  already tears down the portproxy rules (~171); it must remove these
  exclusions too, or uninstalling DML leaves Defender permanently ignoring a
  folder that no longer exists.

Editing-the-installer cautions (see CLAUDE.md): keep the change **outside**
the embedded CLI here-string (~836-1633) and away from `$ExpectedCliVersion`
(line 813) — this must not drag in the installer↔CLI sync work, which is its
own later plan. Put the step next to the existing portproxy/firewall block.
The file is PS 5.1 under the ANSI codepage, so any added text stays ASCII (or
the BOM rules from CLAUDE.md apply).

### Incident follow-ups (2026-07-21 docker-network wedge — diagnosed live)

Root cause that night: the distro's Docker network black-holed (connect
timeouts, not refusals) → in-game "can run but not interact", soap_unreachable
card, and a restart that crash-retried on "Can't connect to MySQL (110)" for
10 minutes while the stream said "world is loading". Fix was
`systemctl restart docker` in dml-arch (manual). Improvements earned:

1. **One-click "Restart Docker in the distro"** on Tools (+ the
   soap_unreachable card linking to it). The actual fix tonight is not
   clickable anywhere today.
2. **Boot-loop detection in the readiness wait**: while waiting, watch the log
   for repeated "Could not connect to MySQL" (or repeated boot banners) and
   say so — after N retries, suggest the Docker restart — instead of
   "world is loading" forever.
3. **Snapshot the worldserver log before stop/restart** (to ~/.dml/logs):
   compose recreate destroys the old container's log; we lost the freeze
   evidence twice.
4. **skip-saveall UX**: the unticked "save characters" box silently persisted
   from an earlier session and surprised the user during an incident.
   Consider not persisting it (default safe every app start) or a clearly
   visible "faster mode" badge. NB the skip path itself fired correctly in
   the wild (message shown, restart proceeded) — a live half-sighting of the
   [skip-saveall] row; character data verified intact afterward.

### Statistics page (user request, 2026-07-21 — DECIDED, build after smoke testing)

Beyond-parity feature (verified: The Lab has NO stats page). Scoped against the
live DB — every stat below ran sub-second. User picked the groups:

- **INCLUDE:** World Population (family vs bots, level spread, class/faction
  split, guilds) · Economy (gold in circulation, richest, AH stock, mail) ·
  Family's Journey (playtime, last seen, achievements, quests, honor,
  professions) · Server History (boots, lifetime hours, longest session,
  connection peak — from acore_auth.uptime) · Bot Watch (busiest zones,
  bots per continent, combined bot playtime).
- **EXCLUDED:** Fun & Games (casino/transmog/pets) — user opted out.
- Implementation notes (from scoping run wf_1f4d916a): new read-only `wow
  stats` CLI arm → wow_stats Tauri cmd → api.ts → Statistics.svelte in the
  Server sidebar group; load-on-mount + Refresh (NOT the 7s poll); needs
  client-side id→name maps (class/race exist in $lib/wow; zone map = biggest
  cost, drives Bot Watch's medium effort); label AH stock as shop stock (it is
  100% ahbot); arena ratings are seeded fiction — never show a ladder; deaths
  are not tracked in stock AC (impossible); server-stopped → the in-page
  greeting. Read-only only — never extend the sanctioned-write list.

### App icon — reuse the old DML Launcher mark (user request, 2026-07-28)

> **SHIPPED 2026-07-28** (commit `c84d30d`). Source supplied by the user as
> `C:\DML\dml.ico`; the 256x256 PNG frame inside it is committed as
> `launcher/src-tauri/icons/source-dml-256.png`. NB the mark is a purple LAB
> FLASK, not an hourglass as written below. Only open item: the source is
> 256px where tauri wants 1024, so the Windows Store tiles are upscaled.

Ship the launcher under the ORIGINAL DML Launcher icon: the purple one with an
hourglass. Today `launcher/src-tauri/icons/` still carries the stock Tauri
placeholder set, so the exe, taskbar entry and both installers all show a
generic mark.

- **Source asset first.** The original purple/hourglass art is not in this
  repo — it has to be recovered from the old launcher (the same place The Lab
  intel came from) or recreated. Everything below is blocked on having one
  square, transparent, high-res PNG (1024×1024 is the safe input size).
- **Regenerate the whole set, do not hand-edit.** `npm run tauri icon
  <source.png>` from `launcher/` rewrites every file in `src-tauri/icons/`
  (`icon.ico`, `icon.png`, `icon.icns`, `32x32`, `128x128`, `128x128@2x`, and
  the nine `Square*Logo.png` + `StoreLogo.png` Windows Store sizes). Replacing
  only `icon.ico` leaves the NSIS/MSI and Store tiles mismatched.
- **Feeds the system tray.** The tray round (plan
  `2026-07-27-launcher-self-config-and-tray.md`, Tasks 7–14) needs a tray
  icon; it should use this same mark. A 32×32 at tray size needs the hourglass
  to stay legible — check it before settling on the art, since a detailed logo
  usually turns to mush in the notification area.
- Verify by building (`npm run tauri build`) and confirming the icon on the
  bare `launcher.exe`, both installers, the taskbar, and the tray.

## Round 3 — Shareable release / installer

**The highest-value new capability**, given the stated goal of sharing this.
Today a recipient needs WSL2 + Node + Rust to run it from source.

- `npm run tauri build` → NSIS installer.
- Decide the signing story (unsigned = SmartScreen warning; a cert removes it).
- Publish via GitHub Releases so there's a real download link.
- This is also the prerequisite for launcher self-update (Round 5).

## Round 4 — Features blocked on install state

These were skipped because the box lacks the prerequisite, not because they're
hard. Smoke testing may resolve the blockers.

- **Battle Pass config editor** — needs the Battle Pass ALE mod actually
  deployed (the `battlepass_config` table doesn't exist yet).
- **Accountwide configurator live validation** — the module isn't installed on
  the box; the feature is fixture-tested only.
- **NPC Teleporter capital/starting-zone toggles** — needs the module install
  flow reworked (the toggles live only in `wow-manage.sh`).

## Round 5 — Larger backlog (ranked)

- **Launcher self-update** — needs Round 3 first (signed release manifests).
- **Per-character export/import (.dmlbak)** — genuinely useful for moving a
  kid's character; the GUID-remap step is the risky part and deserves careful,
  tested work rather than a quick pass.
- **Adopt / migrate an existing hand-built server** into the launcher.
- **NPCBots server variant** (separate bot mod, prebuilt-vs-compile choice).
- **Guest / visiting system** — only sensible after Tailscale play-together is
  proven working in practice.
- **Controller support** (ConsolePortLK + mapper) — large, Deck-centric
  upstream; a Windows port is a research task on its own.
- **HD graphics packs** — BLOCKED: the pinned source (btground.tk) is dead
  (HTTP 503, no Wayback copy). Needs a live mirror or it stays impossible.

## Round 2.5 — RECOVERED BACKLOG (filed 2026-07-28)

**Why this section exists.** Everything below was asked for or approved by the
user and then went missing, because its only record was a file under
`.superpowers/` — which is **gitignored**. Work parked there is invisible to
git, to this roadmap, and to every audit run against them: a 24-agent sweep of
outstanding work on 2026-07-28 could not see any of it. Two later sweeps (a
`.superpowers/` orphan hunt, and a read of all 9 session transcripts — 192 asks,
94 verified delivered) recovered the list. **Never let `.superpowers/` be the
only home for an approved decision — file it here the same day.**

### 2026-07-26 smoke-test feature batch (13 items, user-dictated, none built)

Captured verbatim during a live smoke run into `.superpowers/sdd/feature-batch-2026-07-26.md`.

1. **Backups: on/off toggles for the two automatic backups** (on-stop + 6h
   interval). Both are hard-wired always-on today; `interval_backup_watcher`'s
   own doc comment says there is no UI control. Backups.svelte has no settings
   section at all.
2. **Tools: open `.wslconfig` in the default editor.** The card edits it through
   curated fields with no way to see the raw file. `tauri_plugin_opener` is
   already registered app-wide and unused here.
3. **Tools: realmlist target picker** — local / LAN / Tailscale / public /
   custom typed address. UI-only gap: `realmlist_fix` already accepts an
   arbitrary target string and the Tailscale IP is already resolved on the same
   page; only the two fixed buttons exist.
4. **Statistics: characters-online over time.** The current-online tiles exist;
   the time series does not (no sampler, no history store). Shares
   infrastructure with the Performance Advisor below — build once, both consume.
5. **Modules: sort installed first; show playerbots in Module tuning.** The cpp
   list renders in raw registry order. mod-playerbots cannot appear in tuning at
   all: tuning cards derive from cpp catalog rows and there is no mod-playerbots
   entry — its keys live only under Config → Bot World.
6. **Teleport: group locations into per-zone dropdowns/accordion.** Still one
   flat chip list with a filter; no zone field in the derivation.
7. **Backups: pin/keep a backup** so the prune never deletes it and it does not
   count toward the 10. `prune()` currently deletes everything past the window
   with no exclusion.
8. **Search-box labelling** — the Bot Browser's "Name prefix" placeholder (the
   one misread during smoke) and the generic "Search keys…" browsers. Pure copy.
9. **Bots: log the random bots OUT and back IN on demand** (non-destructive).
   The `wow bots` namespace has only `list` and `flush`; the user explicitly
   said flush is NOT the tool for this.
10. **Surface that native mode is RUN-ONLY** — gate Rebuild / Core update /
    cpp-module install. Asked for FIRST as cheap interim safety before handing
    native to a tester. ModuleManager has no backend awareness; the native
    rebuild path runs to `compose up --build` with no check that the compose
    file even has a `build:` context, so the silent no-op is still reachable.
11. **Native standalone: real build support** (AC source + build-capable
    compose). Explicit user decision 2026-07-27: "YES — make native a true
    replacement, not a companion." Nothing built.
12. **Six small native ports** — `wow_config_files`, `wow_party_list`,
    `wow_party_setup`, `games_catalog`, `games_list`, `dml_version`. All still
    shell bash `dml`, i.e. Git Bash in native mode — the dependency the
    all-native goal exists to remove.
13. **Native install path** (`games install` / url install). The user's own note
    calls this the genuine blocker for a WSL-free story.

### Recovered from session transcripts

- **Tray multi-server + server naming** (asked 2026-07-28). The tray's
  Start/Stop are static items that act on a hardcoded `WOW_ID`; with several
  servers installed it always drives the WoW playerbots title. Wants: pick which
  server, name a server at install, rename it later, show that name in the tray.
  NEEDS DESIGN — see the open questions at the end of this section.
- **Open the upstream PR** to DadsMmoLab/dads-mmo-lab for the
  games-folder/mnt-hints/updater fixes. Built, committed and pushed to the fork
  on 2026-07-14 (`2e2b139` / `2931f60` / `38b3a9c`); the session ended one
  command short, and `gh pr list` confirms no PR exists. The branch is ready.
- **Retire the old C# tray app + port its WSL keepalive.** A fresh
  `Install-DML.ps1` run still installs `DML-Launcher.exe`, so a user ends up
  with TWO tray apps. Tray/sleep-block/autostart/single-instance were absorbed;
  the retirement half was silently dropped.
- **Promote the dml CLI out of the installer here-doc.** A fresh installer run
  still ships CLI **v2.6.0** against the repo's v3.0.0. CLAUDE.md calls this "a
  dedicated later plan"; no such plan exists, so nothing schedules it.
- **SOAP setup without a terminal.** `soap-setup` was meant to happen at
  install; it became a manual CLI verb with no GUI button, and the launcher's
  own error hints tell the user to run a terminal command.
- **Substrate wizard** (GUI owns first-run: WSL2 enablement, UAC, reboot-and-
  resume). A v1 decision, silently reversed in favour of "no elevation from the
  app" — but the design spec still lists it as v1 scope, so the docs contradict
  the shipped product. DECIDE: reinstate, or formally retire it in the spec.
- **Steam integration + item-DB favorites** — the last two Lab-parity features
  absent from both the app and the Round-5 "still missing" list.
- **`wow-manage.sh` shared-libs refactor** so the Deck TUI and the GUI run the
  same code. Plan 1 extracted the CLI from the installer instead, creating a
  SECOND implementation; the Deck TUI and the dml CLI now drift independently.
- **Keep terminal history across runs.** Asked during the Plan-2 live gate,
  answered "by design, offered as an enhancement", never filed. Round N made
  transcripts survive navigation, but `beginRun` still resets the buffer.
- **GM equip freedom** — macros to learn everything needed to wear anything,
  whether class-restricted items can be allowed for a GM character, and whether
  stripping class requirements is easier. Chat-only deliverables; note the third
  option needs an explicit exception to the read-only-MySQL rule.
- **Sheathed shield sits offset** on the back instead of centred (cosmetic, low
  priority). Still the single hardcoded SheatheType 4.
- **ConfigEditor backlog leftovers**: docs-aware Files tab and the dirty-guard.
  (Reset-to-default shipped; the Dashboard auto-poll was superseded by the
  shared polled status card; settings search exists only in the key browsers.)
- **Commit the youtube-transcript skill.** It lives only in
  `C:\Users\perzi\.claude\skills\`, which is not a git repository — a profile
  wipe loses it with no recovery path.

### Requested 2026-07-28, after the SHIP-LIST (QUEUED — not started)

Filed here immediately rather than acted on: the SHIP-LIST's standing rule is
no new features until Phase 4 (the release gate) ships. Recorded the same day
it was asked so it cannot go the way of the items above.

- **Install a WoW addon from a URL.** Paste an addon's URL in the launcher; it
  fetches it and drops it into the client's `Interface/AddOns` folder.
  Both halves already exist and only need joining:
  - the AddOns destination is already computed and written to today —
    `cli/src/70-modules.sh:549-550` copies BlackMarketUI into
    `<client>/Interface/AddOns/<name>/`, using the saved client path from
    `~/.dml/client-path` (`_client_path`, 70-modules.sh:326);
  - fetching-and-installing from a remote URL already exists too — the Wrath
    Unbound flow downloads its installer over `curl -fsSL --max-time 30` and
    validates the payload before use (`cli/src/90-main.sh:1061-1098`).
  So the work is a URL → archive → extract → verify → copy pipeline plus a UI,
  not new capability. Design notes for whoever builds it:
  - **Decide the accepted source shapes up front**: a direct `.zip` (CurseForge
    /WoWInterface style), a git repo (clone or tarball), or both. A raw `.lua`
    is NOT an addon and must be refused with a real message.
  - **This writes into the user's game client**, which nothing else in the app
    does casually — it needs the untested-feature lock, a named destination
    shown before the write, and a refusal to overwrite an existing addon folder
    without an explicit confirm.
  - **The archive is untrusted input.** Reject path traversal (`../`) and
    absolute paths inside the zip, cap the extracted size, and require the addon
    to land in exactly one top-level folder under AddOns — a malicious or merely
    sloppy archive must not be able to write outside `Interface/AddOns/`.
  - No client path saved yet → say so and point at the setting, rather than
    failing at copy time.

### Per-install container names — the prerequisite for the multi-server tray (filed 2026-07-30)

**Why this is filed and not built:** the user asked whether two WotLK servers can
run at once (e.g. one with Wrath Unbound + playerbots, one plain playerbots). The
answer today is **no**, and the modules are irrelevant — both want a container
literally named `ac-worldserver`, and container names are unique per Docker
ENGINE, not per compose project.

Everything else is ALREADY per-install, which is what makes this worth doing:
`name: {{PROJECT_NAME}}`, the images (`{{IMAGE_PREFIX}}…:{{IMAGE_TAG}}`), the
`db-data`/`client-data` volumes (unnamed → project-prefixed) and `ac-network` all
differ per install. Only five hardcoded `container_name:` lines in
`crates/dml-wow/data/native-compose.yml.tmpl` collide.

**What already works today, and may be enough:** two WotLK installs can coexist
ON DISK — separate directories, projects, volumes and images. Only one may be
UP at a time. So "try Wrath Unbound without endangering my playerbots server"
works now: stop one, start the other; neither can touch the other's data. The
tray could make that a one-click switch. The refusal
(`INSTALL_STACK_CONFLICT`) already names the owning stack.

**The real cost, measured 2026-07-30 (not guessed):** deleting/templating the five
`container_name:` lines is trivial. The work is the consumers — **20 bash call
sites** and **73 Rust references across 13 modules** (`backup`, `restore`,
`config`, `modmgr`, `maint`, `status`, `lifecycle`, `lan`, `destructive`,
`moduletail`, `logsnap`, `engine`, `install_native`) that address containers by
BARE NAME. Every one is a place where resolving the wrong container means acting
on the wrong server — the exact class of the 2026-07-28 log-snapshot incident,
where `docker logs ac-worldserver` answered for whichever title owned the name.
`docker exec ac-database mysqldump` is a WRITE path.

The pattern to follow already exists: `logsnap` resolves through
`compose ps -a -q ac-worldserver` in the stopping title's own compose dir.
Deleting `container_name:` entirely (letting compose name them
`<project>-<service>-1`) forces every consumer through that resolution, which is
the honest end state.

**Sequencing:** this must land BEFORE the parked `feat/multi-server-tray` work,
or the tray silently promises something the engine refuses. Mirror bash↔Rust; the
18 parity suites are the safety net. NOT a release blocker — the release is WSL
mode, and one WoW server at a time is fine for v0.1.0.

### Tailscale login fails with a timeout that cannot name its cause (found live on the VM, 2026-07-29) — FIXED 2026-07-29

**FIXED, mirrored on both surfaces, mutation-verified.** Diagnosis from the
VM's own tailscaled journal: `RegisterReq` at 22:37:52, `AuthURL is …` at
22:38:22 — the control plane took **30 seconds**, and `tailscale up` was waiting
**8**. The daemon was `active (running)` with TUN present the entire time. So the
login was SUCCEEDING and we threw it away, then reported a timeout.

What changed:
- the login wait defaults to **45s** (`DML_TS_UP_TIMEOUT` overrides it — the same
  seam name and default on both surfaces; the native arm had no seam at all and
  hardcoded 8s inside a 15s outer bound, so raising the inner one alone would
  have let our own kill land first — `TS_UP_OUTER_SLACK_SECS` now guarantees the
  outer bound outlives the inner);
- when `up` prints no URL, the **pending URL is read back from
  `tailscale status --json`'s `AuthURL`** — the daemon keeps it, which is exactly
  the state the VM was left in. grep, not jq (jq is test-only here);
- the daemon step's failure is **no longer discarded**: if it could not be started
  and is not answering, the arm refuses immediately with `TAILSCALE_DAEMON_FAILED`
  carrying systemctl's own words (or `SUDO_REQUIRED` when that is the cause)
  instead of spending the login timeout to say nothing;
- the stale doc comment claiming the native arm runs an MSI installer is gone.

Tests: 3 new bats (`cli/tests/wow-tailscale.bats`, suite now 17) + 4 new Rust
(`launcher/src-tauri/src/lib.rs`). All three bats tests were mutation-verified —
each one goes red against the pre-fix behaviour. Two harness bugs were found and
fixed while writing them: the stub used `${DML_STUB_TS_UP_URL:-default}`, so an
explicitly empty value silently fell back to a real URL and a "no URL" test
proved nothing.

**Still open** (needs a UI decision, not a bug): on native mode the Install
button only PROBES for `tailscale.exe` and cannot install it. The error copy is
honest ("Install the free Windows app from tailscale.com/download") but the
button's label is not. Either download+run the MSI or rename the action.

Original diagnosis, kept for the record:

Reported from the clean Windows 11 VM: Install Tailscale appears to work, then Log
in fails with

> Could not start Tailscale login — timeout waiting for Tailscale service to
> enter a Running state; check health with "tailscale status"

The quoted half is Tailscale's OWN CLI text, emitted by `tailscale up` when the
backend does not reach `Running` before `--timeout`. Ours is the
`TAILSCALE_UP_FAILED` wrapper, which pastes the tail of tailscale's output into
the hint. So the wrapper is working as designed — and that is the problem: it can
only ever report "it timed out", never WHY.

Three defects, all confirmed by reading, none needing the VM to see:

1. **The daemon-start failure is discarded, so the real cause is unrecoverable.**
   `cli/src/90-main.sh:6494-6510` brings `tailscaled` up best-effort
   (`sudo -n systemctl enable --now tailscaled`, else a detached
   `tailscaled --tun=userspace-networking`) and throws away the result — the
   comment says "`tailscale up` below surfaces the real error if it did not". It
   does not: `up` reports only its own timeout. A `sudo -n` refusal, a missing
   unit, a `tailscaled` that starts and dies, and a healthy daemon that is merely
   slow all produce the SAME message. `ts_daemon` is computed and returned but
   nothing gates on it. Fix: check the daemon reached a usable state before
   running `up`, and when it did not, fail with THAT cause
   (`systemctl status`/journal tail, or the sudo refusal) instead of a timeout.
   The `SUDO_REQUIRED` branch at :6541 only catches sudo text that reaches
   `up`'s own output, which the `-n` refusal on the *systemctl* call never does.

2. **8 seconds is too short for a first-ever login, and native has no override.**
   Bash uses `--timeout="${DML_TS_UP_TIMEOUT:-8s}"` (:6516) — an env seam, but
   undocumented and defaulted low. `launcher/src-tauri/src/lib.rs:4962`
   HARDCODES `["up", "--timeout=8s"]` inside a 15s bound with no seam at all. A
   cold daemon on a 2-vCPU VM can exceed that before it prints the auth URL, and
   the URL is the entire point of the flow: no URL means the user cannot even
   complete the login manually. Fix: raise the default, add the seam natively,
   and treat "no URL yet" as retryable rather than terminal.

3. **On native mode the Install button cannot install anything.**
   `tailscale_install_native` (`lib.rs:4914-4920`) only PROBES for
   `tailscale.exe` and returns `NOT_INSTALLED` when it is absent — yet the doc
   comment at :5004 describes "running its MSI installer", which no code does.
   The button's label promises an install it never performs. Fix: either
   download+run the MSI, or rename the action and say plainly that the Windows
   app must be installed from tailscale.com first.

Not yet known: which arm the VM actually hit (WSL vs native) — the message is
byte-identical on both surfaces, which is itself worth fixing, since a user
report cannot distinguish them. Awaiting the VM diagnostic.

### Also recovered: the Server Performance Advisor

Approved 2026-07-27; spec now committed at
[`docs/superpowers/specs/2026-07-27-perf-advisor-design.md`](../specs/2026-07-27-perf-advisor-design.md).
Auto-detects RAM/CPU/disk telemetry plus world-tick latency and says whether you
can add more bots or should give Docker/WSL more resources. It carries a real
golden acceptance test AND a verified A/B result (`MapUpdate.Threads 1→3` took
p99 from ~98ms to ~36ms on this box). A validated build plan exists; two
CRITICAL findings must be handled before executing it:

- the live box now runs `MapUpdate.Threads = 3`, so it can no longer reproduce
  the spec's single-thread diagnosis — the acceptance gate needs fixture data,
  not the live server;
- the stats JSON fragment shape is pinned in THREE bash places, not the one the
  plan names, so a naive edit ships asymmetric envelopes.

Further plan findings worth keeping: the sampler must not ride the unbounded
`run_json` seam (the DB query and `df` would have no timeout); it needs a
metrics-dir override or its own tests will prune the operator's real
`~/.dml/metrics`; and Rust std has no free-space API, so the disk-free step
needs a real mechanism chosen before it is built.

### Open questions for the user (blocking the tray/naming work)

1. Where should a server's display name live — a per-title file in the title
   directory, or `~/.dml/launcher.json`? (The title dir travels with the
   server; the launcher config is easier to edit from the GUI.)
2. Should the tray list EVERY installed server with its own Start/Stop, or keep
   one "active server" that the whole app follows?
3. Home is hardcoded to `wow-server-playerbots`. Does it follow the active
   server, or stay WoW-only for now?

## Round 5.5 — Keira3 integration (user request, 2026-07-31)

**Integrate https://github.com/azerothcore/Keira3 into the launcher.** Filed the
day it was asked for, because a request that lives only in a chat is a request
that gets lost — this repo has already lost a perf-advisor spec and a 13-item
feature batch that way.

Keira3 is AzerothCore's own database editor (Angular + Electron, AGPL-3.0 — the
same licence as this repo, so bundling is licence-compatible). It edits
`creature_template`, `quest_template`, loot, gossip, SmartAI and so on directly
over MySQL.

NOT SCOPED YET. The open questions, all of which change the size of the job:

1. **Embed or launch?** Three candidate shapes: (a) ship Keira3's web build
   inside a Tauri window and point it at the world DB; (b) detect/launch the
   user's installed Keira3 desktop app and hand it connection details;
   (c) link out and document it. (a) is the real product answer and the most
   work; (c) is a day.
2. **It needs a WRITE path to MySQL, which this project deliberately does not
   have.** The standing security posture is: MySQL access is strictly READ-ONLY,
   mutations go over SOAP GM commands, and `wow backup restore` is the ONE
   sanctioned write into character data (the LAN toggle's realmlist UPDATE being
   the only other write). Keira3 is a bulk world-DB editor — it exists to write.
   Deciding how that squares with the posture is the FIRST task, not an
   implementation detail. World DB is not character data, which is probably the
   line to draw, but it must be drawn explicitly and written down.
3. **Credentials.** Keira3 needs real MySQL credentials; today they are resolved
   per-call and never handed to a GUI.
4. **Backups.** A world-DB editor makes a "take a backup first" prompt close to
   mandatory. `wow backup create` already exists and should gate the first open.

Blocked on nothing technically, but it is a FEATURE, and SHIP-LIST's one rule is
no new features until Phase 4 is done. Post-beta.

## Round 5.6 — Fully automatic SOAP account setup (user request, 2026-08-01)

**Spec: `docs/superpowers/specs/2026-08-01-soap-account-autosetup-design.md`.**
Design approved by the user the same day it was asked for. **BUILT** the same
day (`crates/dml-wow/src/soap_autosetup.rs`, the `wow_soap_autosetup` /
`wow_soap_credentials` Tauri commands, the shell banner, the reveal control on
Home), plus two adversarial review waves whose fixes are folded into the spec —
the deleted `Latched` outcome and the `family_taken` guard being the two that
changed behaviour. **Remaining USER GATE: the live click-through** — start the
launcher against a server whose SOAP is rejecting it, confirm the banner appears
naming the account, that GM Tools and My Party then work without further input,
and that Home's health panel reveals the password.

The direct-write route (`srp6.rs`, `account_write.rs`, `wow_soap_account_create`)
already removed the worldserver-console step earlier that day, but three manual
acts remained at the end of a multi-hour install: the card renders only inside
`Library.svelte`, the user must invent and type a password, and the user must
click a button. This removes all three.

Four decisions, all user-taken 2026-08-01:

1. The launcher **generates** the password (16 chars — AzerothCore's own ceiling,
   enforced by `valid_account_pass`), revealable from Home's health panel.
2. It fires **any time SOAP is reachable and rejecting us**, off the existing
   status poll — so the migrated `dml-native` server self-heals too, not only
   fresh installs.
3. **Silent, with a dismissible shell-level banner** naming the account. Not a
   prompt.
4. On a name collision it creates `dmlsoap_<random>`. It **never** overwrites or
   resets an account it did not create.

Accepted risk, recorded here as well as in the spec: pointing the launcher at any
AzerothCore whose SOAP rejects it now creates a GM3 account. Guarded by
`Rejected`-only, never-overwrite, one attempt per launcher run, and the banner —
plus, from fix wave 2, a refusal once any `dmlsoap_*` account exists. The latch
only bounds a single run; that family check is what bounds the total, and
without it an exported `DML_SOAP_USER`/`PASS` pair (which outranks the file the
feature writes) turned every launcher start into one more GM3 account.

## Round 5.5 — MULTI-TITLE, and Turtle WoW (user, 2026-08-04)

Filed the day it was asked, per the standing rule that an approved ask living
only in a conversation is invisible to git, to the roadmap, and to any audit.
Both items came up while deciding how the Arch backend's setup chain should ask
"is a title installed?" — the answer depends on these, so they are recorded
rather than remembered.

* **Every WoW server installed at once, start whichever you want.** Today the
  Rust `dml-wow` binary is deliberately a PER-TITLE CLI, "fixed to one already
  installed title" — it has `games-remove` but no `games list`, and no notion of
  switching between titles. The bash CLI's `games` namespace is the only
  multi-title surface that exists. Making several servers co-resident needs
  three things that do not exist yet:
  - a cross-title lister and selector in the Rust surface (its home is an open
    question — `dml-wow` is per-title by design, so wedging it in there is
    probably wrong);
  - per-install container names. The `ac-*` `container_name`s are global to the
    docker ENGINE, which is why `INSTALL_STACK_CONFLICT` exists and why exactly
    one stack can run per PC today. This is already a recorded follow-up from
    `install_native.rs`; multi-title makes it a REQUIREMENT rather than a
    nicety.
  - port allocation. 3724 / 8085 / 7878 are refused when taken (`stack_port_refusal`),
    so co-resident stacks need distinct published ports or a "only one runs at a
    time" rule made explicit in the UI.
* **Turtle WoW** as a title. In production upstream as of 2026-08-04. Not
  AzerothCore — it is its own server core with its own client, so it does not
  reuse the WotLK installer, the playerbots module, or the AC-shaped compose
  trio. Treat it as a new title with its own installer and its own capability
  matrix, in the same family as the Vanilla/TBC work waiting on the
  `.sh`-in-a-distro runner (B4b) — which the Arch backend solves, since that
  backend IS an Arch box with systemd and passwordless sudo.

Neither item is in the v0.1.0 cut. Both are why the Arch setup chain answers the
"is a title installed?" question by looking at the games directory rather than by
calling a `games list` that would have to be invented in the wrong place.

## Round 5.6 — THE SECOND UNBOUNDED CALL (found 2026-08-04, fix approved)

A sibling of the `run_bounded_outcome` incident, in a different module and still
live. `cli_integration.rs`'s
`install_native_refuses_an_unreachable_docker_before_it_creates_anything` runs
**12+ minutes against a 30-second `PROBE_TIMEOUT`** — the docker-reachability
probe in `install_native.rs` / `preflight.rs` does not honour its own bound.

**FIXED 2026-08-05, and the diagnosis above was WRONG — recorded rather than
quietly edited, because the wrong guess is instructive.**

It was never the probe. Both probe paths already went through the fixed
`run_bounded_outcome` and returned in milliseconds against an unspawnable
program. The real cause was the **tri-state collapse, for the third time on this
branch**: `ensure_decision` took a plain `engine_up: bool`, so
`ProbeOutcome::ProgramMissing` — "the one genuinely definitive negative" — arrived
flattened into `Tri::No`, produced `Launch`, **launched the Docker Desktop GUI
from a unit test**, and then polled `docker info` through the CLI that does not
exist, 61 times, for an answer false by construction. Forcing the launch to fail
dropped the test to 4.43 s before a line of production code changed. Separately,
`engine_running` was a bare `cmd.status()` with **no bound at all** — and it is
the predicate that poll calls 61 times.

Measured: the test 184.48 s → 4.41 s; `cargo test --workspace` 261.91 s → 109 s,
1726 passed, 0 failed.

**The CI claim in the original entry did not reproduce and should not be
repeated.** At base the suite FINISHED, in 262 s, with zero failures — it was
slow, not hung, so "a green badge means the job was cancelled" was not
established. The slow path is gated on `docker_desktop_program()` being `Some`,
which is `None` on Linux by construction (its candidates come from
`LOCALAPPDATA`/`ProgramFiles`), so this was probably never what affected the
ubuntu job or its artifact step. **If CI is in fact blind, the cause is still
unfound and wants chasing against a real CI run.** On `windows-latest` it depends
on whether `Docker Desktop.exe` sits at one of the three candidate paths, which
cannot be verified from here.

The likeliest route to the originally-reported 12+ minutes: a unit test was
starting Docker Desktop, and on a cold box its WSL2 VM with it — inherently
non-deterministic, which is exactly why the timing would not reproduce. Now
`launches == 0`.

## Round 6 — Merge + housekeeping

- Decide when the 394 commits land on `main`.
- Consolidate `docs/SMOKE-TESTS.md` (it has grown to 21+ sections) and prune
  superseded specs in `docs/superpowers/specs/`.

---

## Sequencing rationale

Verify → fix → package → extend. Building more on 50 unverified features
compounds risk, and several of them touch real data (bot flush, ARAC client
patching, backup restore). Round 3 is pulled ahead of the larger features
because it converts everything already built into something actually
shareable, which is what the project is for.
