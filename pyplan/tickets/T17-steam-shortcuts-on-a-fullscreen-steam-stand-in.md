# T17 — 8.8 Steam integration, gated on fullscreen Steam on a Linux VM as the Deck stand-in

**Status:** BLOCKED on the owner (a Steam login for the VM, or a `shortcuts.vdf` captured from a real profile) — spec filed 2026-09-09 14:20 CEST by the lead (Fable)
**Hand:** Opus, worktree branched from `yulon-phase8b`; two halves, the read first
**File set:** to be fixed when the read half lands — expected `pylauncher/yulon/steam.py` (new), `pylauncher/yulon/platform.py` only at the SteamOS read-only helpers already there (`:132`, `:434-439`), a Steam group in `pylauncher/yulon/ui/controller_view.py` (declared before the code half starts; T14/T5/T7/T12 sites untouched), `pylauncher/yulon/catalog/catalog.json` only for artwork paths if the entries carry them, their tests, `pyplan/write-ledger.md`, and `pyplan/gates/8.8-steam-<box>-<date>/`.
**Box:** a Linux VM with Steam installed and logged in, run in fullscreen (Big Picture) mode as the Deck stand-in — the owner's answer of 2026-09-09 ("we can use full screen steam to emulate steam deck"). `yulon-arch` (Y: drive, docker, internet through the host) is the candidate; installing Steam there is a VM action, allowed; a Steam **login** is not something a hand can supply.

## The box's own words (checklist 8.8)

The server launcher and the game client added to the Steam library with artwork and the compatibility tool, written with Steam closed, backed up first, keyed so a second press replaces rather than appends; nothing drawn on Windows or macOS. Definition of done: both entries appear in a real Steam library after a Steam restart and both launch; a second press changes nothing; with Steam running the button refuses and says so. Prerequisite: **the `shortcuts.vdf` format is recorded nowhere in this repository and must be read from a real Steam profile before the writer is built**; the gate records whether the userdata folder sits outside SteamOS's read-only root, because the writer must not need an unlock.

## Half 1 — the read (needs the owner)

Either (a) the owner supplies a Steam login for the VM (a throwaway account is enough; Steam is free), or (b) the owner captures from their own PC: `<Steam>/userdata/<id>/config/shortcuts.vdf` before and after adding one non-Steam game by hand, plus `<Steam>/userdata/<id>/config/grid/` listing, and drops the three files in `pyplan/phase8-reads/steam/`. With (a) the hand does the same on the VM. The read half's deliverable: `pyplan/phase8-reads/steam/README.md` — the binary VDF layout as observed (field names, the `appid` derivation Steam uses for non-Steam entries, where artwork is keyed), and whether `userdata/` is under the read-only root on SteamOS (from the Deck's documented layout, cited, since no Deck is reachable).

## Half 2 — the writer and the button (after half 1)

- `steam.py`: find the Steam root and the profile (refuse when zero or more than one profile), refuse when Steam is running (a process check, named in the sentence), back the file up first (`shortcuts.vdf.yulon-bak-<stamp>`), parse the binary VDF, upsert two entries keyed by a stable `appid` derived the way Steam derives it (so a second press replaces), write artwork under `grid/`, set the compatibility tool only where the platform needs it (Linux client → Proton), write nothing on Windows/macOS (the button is absent there, not disabled).
- The button on the controller view, the confirmation naming the two entries and the file, the refusal sentences (Steam running; no profile; two profiles; unwritable).
- Tests, TDD, each naming its mutation: the parser round-trips the captured file byte for byte; upsert replaces on the second call; Steam-running refusal with no write; the backup exists before the write; no write on a non-Linux platform (platform seam).
- Live half on the VM: Steam closed → press → Steam started in fullscreen → both entries visible (frames) → both launch (the server launcher opens Yu'lon; the client launches its binary) → second press → file unchanged (sha256) → Steam running → press → refusal frame.

## Definition of done

Half 1: the README with the layout and the captured files. Half 2: `--checks` ALL GREEN, the live captures above, `test_no_secrets_in_evidence.py` green. Commits per half, `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` only; no push; the ticket file is not yours.


## Owner, 2026-09-10 18:26 CEST

A throwaway Steam account will be given later; blocked until then.

## Prior art read (the lead, 2026-09-10 21:21 CEST): `rust-main`'s `docs/reference/dmlpack/dmlpack.py`

- The format is already captured: `docs/reference/dmlpack/TurtleV2-manifest.json` (packed from a real Steam Deck, account `28006437`) carries two verbatim `shortcuts.vdf` entries with every field (`appid`, `AppName`, `Exe` quoted, `StartDir`, `icon`, `ShortcutPath`, `LaunchOptions`, `IsHidden`, `AllowDesktopConfig`, `AllowOverlay`, `OpenVR`, `Devkit`, `DevkitGameID`, `DevkitOverrideAppID`, `LastPlayTime`, `FlatpakAppID`, `sortas`, `tags`). Half 1's "captured `shortcuts.vdf`" ask is answered from the tree.
- The codec (`dmlpack.py:245-295`): types 0x00 map / 0x01 string / 0x02 int32 LE / 0x08 end; the document ends `08 08`, and a file missing the second byte makes Steam drop every non-Steam shortcut. `gen_appid` (`:297`): `crc32(quoted_exe + appname) | 0x80000000`, so grid art `<appid>*.png` follows without lookup; never rename.
- `register_shortcuts` (`:1558-1645`): refuses while `pgrep -x steam` finds Steam (it rewrites the file on exit and discards outside edits); detects the account (`steam_config_dir`, the `userdata/<id>/config` with a `shortcuts.vdf`, most recent wins); add-only by `AppName`; backup `shortcuts.vdf.backup-<stamp>`; round-trips the payload through its own parser and checks the terminator before writing.
- Two entries per game: the client (`WoW.exe`, Proton) and the server (`/usr/bin/konsole`, `--hold -e bash ~/<game>-launcher.sh`, Proton off). Gaming Mode needs nothing else (README `origin/main:283-296`).
- NOT automated upstream: the compatibility tool ("must be set on the shortcut after restore, since the appid carries no CompatToolMapping entry" -- that lives in `config/config.vdf`, text VDF, `CompatToolMapping/<appid>`), the artwork, and replace-on-second-press (upstream is add-only).
- Still owed by the owner: a Steam login on `yulon-arch` for the live proof only (Big Picture showing the two entries after the press); a throwaway account serves.
