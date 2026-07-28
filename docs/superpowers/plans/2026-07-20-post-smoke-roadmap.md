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
