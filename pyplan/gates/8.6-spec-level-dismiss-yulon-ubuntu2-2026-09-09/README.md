# T5 live half — chosen spec, chosen level, dismiss all, pressed through the panel — yulon-ubuntu2, 2026-09-09

**Is 8.6's definition of done met in full? No.** Dismiss-all works against a live server and is
photographed doing it. The chosen **spec** could not take effect here at all — this install has no
premade specs deployed — though the run proved the app now offers exactly what the server has. The
chosen **level** did not take: the server accepted the command and the row kept its old level.

**Read the capture column before believing a row.** The first version of this page stated a dozen
console readings as fact and shipped no artifact for any of them; a reviewer caught it, and this
version marks every claim. **captured** means a file in this folder shows it. **asserted** means it
was read at the console during the run, was never written down, and is here only because removing it
would hide what the conclusions were actually built on. Nothing marked *asserted* is load-bearing
for any sentence in `party.py`.

Pressed from `182fc92a` plus the corrections this run forced. `--checks` ALL GREEN on yulon-fedora.

## The world was up throughout

| claim | capture |
|---|---|
| every press was answered, and the nine preconditions read `ok` including `bridge_answered` | **captured** — `panel-1`, `panel-4`, `panel-5`, `panel-6`, `panel-8`, `panel-9` all show `ok world_running` and the full list |
| `ac-worldserver` at pid 409560 at 12:38 and again at 13:21, never restarted | **asserted** — read with `pgrep -f worldserver` over ssh and not captured. `gate86b.liveness()` prints status, `started_at` and restart count and **no pid**, so the pid in the first version of this page came from the shell, not from the driver |
| every client capture is stamped with whether `Wow.exe` was alive | **captured** — `client-agent.log`, one `-- Wow.exe alive pid=2184` per `shoot` |

## 1. The spec picker reads THIS install's conf — and the run overturned how

`panel-1-spec-picker-warlock.png` (13:00, **captured**) shows the picker open on six warlock specs
with the master named and the party empty. Choosing `warlock` re-read the list live.

| class | what the picker offered | capture |
|---|---|---|
| `warlock` | `affli pve, demo pve, destro pve, affli pvp, demo pvp, destro pvp` | **captured** — `panel-1` |
| `warrior` (at start-up) | `arms pve, fury pve, prot pve, arms pvp, fury pvp, prot pvp` | **asserted** — console only |

Both lists came out of `playerbots.conf.dist`, and **that was the defect**. Asked through the bridge
what it had, the module answered — **captured**, `client-4-spec-list-from-the-module.png`:

```
To [Michaelah]: talents spec list
[Michaelah] whispers: Total 0 specs found
```

and refused every name, the invalid one and the valid one alike — **captured**,
`client-3-spec-refusal-in-game.png`:

```
To [Michaelah]: talents spec warglaive pvp
[Michaelah] whispers: Spec warglaive pvp not found
To [Michaelah]: talents spec destro pve
[Michaelah] whispers: Spec destro pve not found
```

That both whispers returned `outcome=yes` with **empty text** over the channel is **asserted**
(console only) — but the two frames above are the point on their own: the refusal is in the game
window, and nothing in the app's own surface ever showed it.

`sConfigMgr` loads the DEPLOYED conf; this box has only `playerbots.conf.dist`, so it has no premade
specs. `PLAYERBOTS_CONF` now reads the deployed file only, and
`panel-7-corrected-picker-agrees-with-server.png` (**captured**) shows the corrected picker offering
one row, `let the server pick` — the app agreeing with the server instead of overruling it.
`panel-8` and `panel-9` show the same single row in later presses.

**This is the ticket's own instruction working:** *"Measure it on the box, do not trust the list."*
The list was not to be trusted, and neither was the sentence written under it.

## 2. The chosen level — the readback works, the app's own level press does not

| # | press | what the row read | capture |
|---|---|---|---|
| A | warlock, level **42** → `Bafossan` | `Bafossan — level 1`, one bot in the party | **captured** — `panel-8` (13:12) |
| B | hunter, level **55** → `Jagyl` | `Jagyl — level 1`, and it stayed 1 for the 68 s from 13:17:35 to 13:18:43 | **captured** — `panel-9` (13:18) and `panel-transcript.log:61-130` |
| C | the same readings carry `Bafossan — level 42` and `Kenvadi — level 37` | so the column does show levels that have changed | **captured** — `panel-9`, transcript from 13:17:17 |

That is the whole captured case, and it is what `party.py`'s sentence now rests on: the server
accepted it, the row still reads the old value, and it stayed that way for as long as the panel
watched — with no claim about why.

**Everything below is asserted, not captured**, and no sentence in the app depends on it. It is
listed because it is how the earlier conclusions were reached, and a reader deserves to know that
the reasoning ran ahead of the evidence:

* a third panel press (warlock, 42 → `Michaelah`) with the row read six times over 30 s, still 1;
* `.character level Michaelah 42` by hand answering `You changed level of Michaelah to 42.` with the
  row still 1, and a `.saveall` then making it 42;
* press A's row staying 1 **through** a forced `.saveall`;
* `.character level Bafossan 42` by hand, then a save, reading 42;
* `dml_whisper … autogear` on a level-42 bot leaving it at 42;
* a fresh bot levelled 25 s after it appeared, holding at 37.

`panel-8` also carries a sentence this folder now contradicts: *"an online character's row is
written when it next saves, so the number here catches up later rather than now."* That explanation
was built on the fourth bullet above — an uncaptured reading — and `party.py` no longer says it. The
frame is kept rather than deleted: a page showing only the final wording would hide that it took two
tries to stop explaining and start reporting.

**Named as a finding, not fixed here:** the app's level press does not land, and the fix is a
readback and a re-send on the join poll's machinery. The timing that would make it correct was never
captured, so it is a ticket.

## 3. Dismiss all — proved, including the refusal the reviewers argued about

**First round** — three frames, clocks from the GNOME bar in each:

| frame | clock | what it shows |
|---|---|---|
| `panel-4-dismiss-all-armed.png` | 13:05 | armed on two — `Lexiguk — level 1`, `Michaelah — level 42`; button **Press again to dismiss 2 bots**; *"This uninvites 2 bots from Tfivepress's party -- Lexiguk, Michaelah -- and whispers each one to log out. Press Show this character's party to cancel."* |
| `panel-5-unconfirmed-party-refused.png` | 13:06 | a third bot has joined; three rows; button back to idle; *"Tfivepress's party is not the one that was confirmed: 2 bots were agreed to and the group table now holds Kiteema, Lexiguk, Michaelah. Nothing was sent — show the party again and confirm what is there now."* |
| `panel-6-dismiss-all-done.png` | 13:07 | re-confirmed and sent: no bots in the party, *"3 bots left the party: Kiteema, Lexiguk, Michaelah."* |

All three **captured**. That the third bot was added from the console with `dml_addclass`, and that
`group_member` read 0 server-wide afterwards, are **asserted** — but `panel-5` shows the party
gaining a row the panel had not confirmed, and `panel-6` shows the list empty, which is the claim
that matters.

**Second round** — fully **captured** in `panel-transcript.log`:

| time | state | what the panel said |
|---|---|---|
| 13:17:17 | 2 bots | party read: `Bafossan — level 42`, `Kenvadi — level 37` |
| 13:17:31 | 2 bots | *Working — the server is being asked.* |
| 13:17:34 | 2 bots | **the third panel press**: *"Jagyl joined the party, geared and specced. The server accepted the level and characters.level still reads 1, not 55…"* |
| 13:17:35 | 3 bots | the row arrives one reading late: `Jagyl — level 1` |
| 13:18:38 | 3 bots | armed: *"This uninvites 3 bots from Tfivepress's party -- Bafossan, Jagyl, Kenvadi --"* |
| 13:18:41 | 3 bots | *Working —* |
| 13:18:42 | 3 bots | **"3 bots left the party: Bafossan, Jagyl, Kenvadi."** |
| 13:18:44 | 0 bots | rows empty, *"This character's party has no bots in it yet."* |

The row arriving and leaving one reading late is the panel drawing what the group table says rather
than what it just asked for — the same honesty the 8.6 folder recorded.

**The bot manager's own timer moved no bot inside a confirm interval**: the only party change
between two presses in this run was the one made on purpose at 13:06. **Captured** for the second
round (the transcript is continuous across 13:18:38–13:18:42 with the rows unchanged); **asserted**
for the first.

## Frames and files

**The panel** — Hyper-V console captures of the VM (`vmshot.ps1` on the host), each carrying the
GNOME clock: `panel-0-opened.png` (12:57), `panel-1-spec-picker-warlock.png` (12:58),
`panel-2-class-spec-level-chosen.png`, `panel-4-dismiss-all-armed.png` (13:05),
`panel-5-unconfirmed-party-refused.png` (13:06), `panel-6-dismiss-all-done.png` (13:07),
`panel-7-corrected-picker-agrees-with-server.png`, `panel-8-level-42-sentence.png` (13:12),
`panel-9-level-did-not-take.png` (13:18).

**The game client** — screen captures on the Hyper-V host, each stamped with `Wow.exe` liveness in
`client-agent.log`: `client-1-in-world-no-party.png`, `client-2-party-frame-with-bot.png`
(Michaelah under Tfivepress in the party frame), `client-3-spec-refusal-in-game.png`,
`client-4-spec-list-from-the-module.png`, `client-5-party-frame-after-dismiss-all.png`, and `a-`…`i-`
from getting a character into the world.

**The driver** — `panel86t5.py` (the shipped `PartyPanel` in a window built with
`_build_my_party_group`'s own two lines; presses arrive as real `xdotool` clicks, never
`QPushButton.click()`), `agentT5.ps1`, `panel-coords.txt`, `panel-transcript.log` (13:16:34–13:20:41).

**Its own account.** Master `Tfivepress` on account `YULONT5` (id 114), both made by this lane. That
they were erased at the end, with no orphaned `account_access` rows and `group_member` empty, and
that `PERZI`'s `last_login`, `Pakka`'s level, `LootPet2.lua`, the realm row
(`100.99.204.5` on both address columns, mask `255.255.255.0`) and `Logger.ALE=4,Console Server` at
line 706 were all unchanged, is **asserted** — every one read over ssh and none captured. The realm
row was never edited, so no authserver restart was owed or made.

## What this does NOT show

* **The chosen level working.** The one clause of 8.6's line this run could not prove.
* **A chosen spec taking effect.** No premade specs are deployed here, so `talents spec` had nothing
  to pick from. Proved instead: the app offers exactly what the server has, an unlisted name is
  refused before it is sent, and the module's refusal is invisible over the channel. **A tree with a
  deployed `playerbots.conf` has not been pressed.**
* **The talent readback.** With zero specs loaded there was nothing to read back.
* **A console capture of any kind.** Every `ground`, `sql` and `send` output in this run was read in
  a terminal and never written to a file; the box belongs to another ticket now, so they cannot be
  re-taken here. That is the single biggest gap in this folder and the reason for the capture column.
* **The panel and the client in one frame.** Captured separately, on two machines.
* **The whole app.** The tab around the panel, and reaching it from a cold start, are not here.
* **One class per press, not ten.** warlock, mage, priest and hunter were pressed; six of
  `party.BOT_CLASSES` were not.
* **`8.6-panel-live-…/client-agent.log:28` carries a password** (`slow PANELGATE86`). This folder's
  agent redacts it — `client-agent.log:33`, `cmd 005 : secret <13 chars>`. The earlier folder is
  left as it was written and this sentence is the correction.
