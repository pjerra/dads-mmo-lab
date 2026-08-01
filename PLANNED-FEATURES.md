# Planned Features — the simple list

Distilled from [`docs/ROADMAP-TO-BETA.md`](docs/ROADMAP-TO-BETA.md) (beta path) and
[`docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md`](docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md)
(the exhaustive backlog — if this file and those disagree, they win). Updated 2026-08-02.

## In progress right now

- **Fully automatic SOAP account setup** — generated password, self-healing, silent banner (Round 5.6). Built + two fix waves, still uncommitted; waiting on your live click-through

## Now — blocking the v0.1.0 beta

1. **Fix stale resume** — resume silently reuses the old broken compose file after a generator fix (A1)
2. **Live install gate (you)** — fresh build from the UI, kill mid-build, resume, real client login (Task 12)
3. **Docs pass** — reconcile docs with what shipped (Task 14)
4. **Release** — one outside tester, tag v0.1.0, installers with honest notes

## v0.2 — next release

1. **`.sh`-in-a-distro runner** — install Vanilla / TBC / Wrath Unbound through a detected WSL distro (B4b, ~1 week)
2. **Wrath Unbound launcher wiring** — `dml-wow unbound` exists but only as a binary
3. **`migrate-import`** — bring an existing server across (Task 10)

## Features you asked for (filed, not built)

### Pages & polish
1. **"Start the server first" greeting** with a Start button on server-needing pages (decided 2026-07-21)
2. **Item DB: hover tooltips + icons** (reuse Character Sheet machinery), one mail recipient at top, typeable recipient
3. **Teleport locations grouped by zone** (accordion instead of one flat list)
4. **Modules: installed-first sorting + playerbots in Module tuning**; grey out Lua rows + Install ALE button
5. **Statistics page** — population/economy/family/history/bot-watch (decided; + characters-online-over-time chart)

### Server & bots
1. **Bots: log random bots out and back in** on demand (flush is not the tool)
2. **Eluna bridge auto-deploys on server start** — kills the three-step dance
3. **Internet play LAN fix** — set `localAddress` so LAN players aren't sent to the public IP
4. **One-click "Restart Docker in the distro"** on Tools (the actual fix from the network wedge night)
5. **Native mode: gate Rebuild/Core-update as RUN-ONLY** until real build support is wired

### Backups & tools
1. **Backup on/off toggles** for the two automatic backups (on-stop + 6h)
2. **Pin a backup** so pruning never deletes it
3. **Realmlist target picker** — local / LAN / Tailscale / public / custom
4. **Open `.wslconfig` in the default editor** from Tools
5. **Build-tool Defender exclusions** (cargo/target) — the install-root half already shipped

### Bigger backlog
1. **Multi-server tray + server naming** — blocked on 3 design answers from you (see roadmap "Open questions")
2. **Per-character export/import (.dmlbak)** — move a kid's character between servers
3. **Launcher self-update** — needs a signed release first
4. **Addon-from-URL** — both halves exist; needs joining + untrusted-archive defenses
5. **Keira3 integration** — undecided: it's a bulk DB writer vs. the read-only-MySQL policy

### Small / parked
- GM equip freedom (wear anything) · sheathed shield offset · Steam integration + item-DB favorites · terminal history across runs · retire the old C# tray · open the upstream PR (branch ready since 07-14) · Perf Advisor (2 Criticals first) · per-install container names · promote CLI out of the installer here-doc · kill the bash CLI
