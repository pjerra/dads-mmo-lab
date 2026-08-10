# Planned Features — the simple list

Distilled from [`docs/ROADMAP-TO-BETA.md`](docs/ROADMAP-TO-BETA.md) (beta path) and
[`docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md`](docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md)
(the exhaustive backlog — if this file and those disagree, they win). Updated 2026-08-10.

## Now — blocking the v0.1.0 beta

1. **VM acceptance run, Round 2 (you)** — the full consumer path on a bare VM:
   installer-delivered launcher, no side-loading (`docs/VM-ACCEPTANCE-TEST.md`).
   Then the merge decision for `feat/core-family`.
2. **Live gates batch (you)** — SOAP autosetup click-through, module-rebuild
   button on the VM, Unbound install/uninstall in game, tray/self-config
   observations, NATIVE-TAIL smoke checklist (0/22), migration live gate (Task 13).
3. **Release** — code-signing decision, one outside tester, tag v0.1.0,
   installers with honest notes.

## Shipped since the last update (was listed above as open)

- **Stale-resume fix (A1)** — done + verified 2026-08-03
- **Task 12 install gates** — leg 1 passed 2026-08-04, leg 2 (fresh VM) 2026-08-09
- **SOAP account autosetup** — committed; only the live click-through remains
- **`migrate-import` (Task 10)** — built 2026-08-03; live gate = Task 13
- **Wrath Unbound launcher wiring** — done 2026-08-02, and Unbound is IN v0.1.0
  (user amendment 2026-08-03)
- **Module rebuild actually compiles** — shipped + live-verified 2026-08-10
- **Statistics page, Item-DB tooltips, Defender build-tool exclusions** — shipped
- **Launcher character pickers** — instant load, bots/citybots/SOAP accounts
  hidden (2026-08-10)

## v0.2 — next release

1. **`.sh`-in-a-distro runner** — install Vanilla / TBC through a detected WSL
   distro (B4b, ~1 week; retires `dml-arch`)
2. **dmlpack** — approved 2026-08-08, source vendored; repo never hosts a pack
3. **Module SQL auto-apply after install** (Round 5.9 — `ac-db-import` is frozen
   at install time, so post-install module SQL never lands; found live 2026-08-10)

## Features you asked for (filed, not built)

### Pages & polish
1. **"Start the server first" greeting** with a Start button on server-needing pages (decided 2026-07-21)
2. **Item DB: one mail recipient at top, typeable recipient** (tooltips/icons shipped)
3. **Teleport locations grouped by zone** (accordion instead of one flat list)
4. **Modules: installed-first sorting + playerbots in Module tuning**; grey out Lua rows + Install ALE button
5. **Characters-online-over-time chart** on Statistics

### Server & bots
1. **Bots: log random bots out and back in** on demand (flush is not the tool)
2. **Eluna bridge auto-deploys on server start** — kills the three-step dance
3. **Internet play LAN fix** — set `localAddress` so LAN players aren't sent to the public IP
4. **One-click "Restart Docker in the distro"** on Tools (the actual fix from the network wedge night)
5. **mod-city-bots: port the duel-steering guard upstream + fix the far-continent
   placement hang** (both found live 2026-08-10; the guard exists only on the VM's checkout)

### Backups & tools
1. **Backup on/off toggles** for the two automatic backups (on-stop + 6h)
2. **Pin a backup** so pruning never deletes it
3. **Realmlist target picker** — local / LAN / Tailscale / public / custom
4. **Open `.wslconfig` in the default editor** from Tools

### Bigger backlog
1. **Multi-server tray + server naming** — blocked on 3 design answers from you (see roadmap "Open questions")
2. **Per-character export/import (.dmlbak)** — move a kid's character between servers
3. **Launcher self-update** — needs a signed release first
4. **Addon-from-URL** — both halves exist; needs joining + untrusted-archive defenses
5. **Keira3 integration** — undecided: it's a bulk DB writer vs. the read-only-MySQL policy

### Small / parked
- GM equip freedom (wear anything) · sheathed shield offset · Steam integration + item-DB favorites · terminal history across runs · retire the old C# tray · open the upstream PR (branch ready since 07-14) · Perf Advisor (2 Criticals first — NB `MapUpdate.Threads` must stay 1 with mod-ale, proven 2026-08-10) · per-install container names · promote CLI out of the installer here-doc · kill the bash CLI
