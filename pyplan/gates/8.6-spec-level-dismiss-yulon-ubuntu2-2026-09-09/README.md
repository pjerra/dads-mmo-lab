# T5 live half — chosen spec, chosen level, dismiss all, pressed through the panel — yulon-ubuntu2, 2026-09-09

**Is 8.6's definition of done met in full? No — and this run is why we know.** Two of the three
controls work against a live server and are photographed doing it. The third, the **chosen level,
does not**: the server accepts the command and the character keeps its old level, because the app
sends it in the first seconds after the bot appears and playerbots is still building the character
then. The app says so in its own words rather than reporting success, which is the only reason this
page can be trusted about the other two.

The run also **refuted a claim the code half shipped**: that a `playerbots.conf.dist` may be read
when no conf is deployed. It may not, the server had loaded **zero** premade specs, and the picker
was offering 63 names that could only ever fail — silently, in a chat window nothing in this app can
read. That is fixed in this commit.

Pressed from `182fc92a`, plus the two corrections this run forced (in the same commit as this page).
World `ac-worldserver` up throughout at **pid 409560** — the same pid at the first ground reading
(12:38) and at release (13:21), so nothing here restarted the world.

## What was pressed, and what answered

Every line below is the panel's own transcript (`panel-transcript.log`, and the session log for the
presses that preceded the corrections). The seam is `party.InstallParty`, built by
`gate86b.build()`; the widget is the shipped `PartyPanel`; the presses are real clicks through
`xdotool` on the VM's desktop, never `QPushButton.click()`.

### 1. The spec picker reads THIS install's conf, per class — proved, then proved wrong, then fixed

`panel-1-spec-picker-warlock.png` shows the picker open on six warlock specs and all nine
preconditions `ok`, including `bridge_answered`. Choosing `warlock` re-read the list live:

| class chosen | what the picker offered |
|---|---|
| `warrior` (at start-up) | `arms pve, fury pve, prot pve, arms pvp, fury pvp, prot pvp` |
| `warlock` | `affli pve, demo pve, destro pve, affli pvp, demo pvp, destro pvp` |

Both lists came from `playerbots.conf.dist` — and **that was the defect**. Asked through the bridge
what it actually had, the module answered (`client-4-spec-list-from-the-module.png`):

```
To [Michaelah]: talents spec list
[Michaelah] whispers: Total 0 specs found
```

and refused every name, the invalid one and the valid one alike
(`client-3-spec-refusal-in-game.png`):

```
To [Michaelah]: talents spec warglaive pvp
[Michaelah] whispers: Spec warglaive pvp not found
To [Michaelah]: talents spec destro pve
[Michaelah] whispers: Spec destro pve not found
```

Over the channel both whispers came back `outcome=yes` with **empty text** — the in-game-only
refusal the code half predicted, measured. `sConfigMgr` loads the DEPLOYED conf; this box has only
`playerbots.conf.dist`, so it has no premade specs at all. `PLAYERBOTS_CONF` now reads the deployed
file only, and `panel-7-corrected-picker-agrees-with-server.png` is the corrected picker offering
exactly one row, `let the server pick` — the app agreeing with the server instead of overruling it.

**This is the ticket's own instruction working:** *"Measure it on the box, do not trust the list."*
The list was not to be trusted, and neither was the sentence I wrote under it.

### 2. The chosen level — the readback works; the app's own level press does not

`characters.level` reaching the panel is proved: `panel-9-level-did-not-take.png` shows
`Bafossan — level 42` and `Kenvadi — level 37` in the group list, both read back out of the
database after their levels were changed.

What does not work is the level THIS app sends. Twice, through the panel:

```
Jagyl joined the party, geared and specced. The server accepted the level and characters.level
still reads 1, not 55 — so the level did NOT take. On this tree a level sent in the first seconds
after a bot appears has been seen not to hold; the same command a few seconds later does.
```

The measurements behind that sentence, in order:

| # | what was done | what was read |
|---|---|---|
| 1 | panel press: warlock, `destro pve`, level 42 → bot `Michaelah` | `characters.level` = 1, six readings over 30 s |
| 2 | `.character level Michaelah 42` by hand | `You changed level of Michaelah to 42.` — row still 1 |
| 3 | `.saveall` | row = **42** |
| 4 | panel press: level 42 → bot `Bafossan` | row = 1, **and still 1 after `.saveall`** |
| 5 | `.character level Bafossan 42` by hand, then `.saveall` | row = **42** |
| 6 | `dml_whisper … autogear` on Bafossan (level 42), then `.saveall` | row = **42** — autogear does not reset it |
| 7 | new bot `Kenvadi`, waited **25 s**, then `.character level Kenvadi 37`, `.saveall` | row = **37** |

Step 4 is the one that settles it: a forced save did not rescue the panel's level, so this is not a
write waiting for a save. Step 7 says the same command works when it is late. Step 6 clears the
whisper that follows it. What is left is the window between the bot's group row appearing — which is
what `add_bot` polls on — and playerbots finishing the character.

**An intermediate sentence in this folder is wrong on purpose.** `panel-8-level-42-sentence.png`
shows a first correction claiming the row *"catches up later"* when the character next saves. Step 4
refuted it an hour later, and `panel-9` is the sentence that survived. Both frames are kept: a page
that showed only the second would be hiding how the first was reached.

**Not fixed here, and named as a finding:** the fix is to read the level back and re-send — the
machinery is already in `add_bot`'s join poll — but the timing that would make it correct was
measured tonight for the first time, and a poll invented on top of one evening's numbers is the kind
of confident guess this feature has already paid for twice. It belongs in a ticket.

### 3. Dismiss all — proved, including the refusal the reviewers asked for

| time | what was pressed | what the panel said |
|---|---|---|
| 13:05:27 | Dismiss every bot (first press) | *This uninvites 2 bots from Tfivepress's party — Lexiguk, Michaelah — and whispers each one to log out. Press Show this character's party to cancel.* Button: **Press again to dismiss 2 bots** |
| 13:06:00 | a THIRD bot added from the console, panel not refreshed | group table now 3 bots; panel still showing 2 |
| 13:06:21 | Dismiss every bot (second press) | *Tfivepress's party is not the one that was confirmed: 2 bots were agreed to and the group table now holds Kiteema, Lexiguk, Michaelah. Nothing was sent — show the party again and confirm what is there now.* |
| 13:06:56 | re-read, armed on all three | *This uninvites 3 bots … — Kiteema, Lexiguk, Michaelah —* |
| 13:07:27 | second press | **3 bots left the party: Kiteema, Lexiguk, Michaelah.** `group_member` rows: **0** |

`panel-5-unconfirmed-party-refused.png` is round 2's must-fix photographed against a live server: a
party that gained a bot between the two presses is not the party that was confirmed, and the seam —
not the panel — is what refused it. The count on screen had not changed; only the group table had.
Repeated at 13:19:15 with `3 bots left the party: Bafossan, Jagyl, Kenvadi.` and the table back to 0.

Also recorded for the reviewer who asked: **the bot manager's own timer did not move a bot inside a
normal confirm interval.** The only party change between two presses in this run was the one this
gate made on purpose.

## Frames

**The panel** (Hyper-V console captures of the VM, `vmshot.ps1`) — `panel-0-opened.png` (opened,
nothing read), `panel-1-spec-picker-warlock.png`, `panel-2-class-spec-level-chosen.png`,
`panel-4-dismiss-all-armed.png`, `panel-5-unconfirmed-party-refused.png`,
`panel-6-dismiss-all-done.png`, `panel-7-corrected-picker-agrees-with-server.png`,
`panel-8-level-42-sentence.png` (the sentence that was later refuted),
`panel-9-level-did-not-take.png`.

**The game client** (screen captures on the Hyper-V host, each stamped with whether `Wow.exe` was
alive) — `client-1-in-world-no-party.png`, `client-2-party-frame-with-bot.png` (Michaelah in the
party frame under Tfivepress), `client-3-spec-refusal-in-game.png`,
`client-4-spec-list-from-the-module.png`, `client-5-party-frame-after-dismiss-all.png`, and
`a-`…`i-` from getting a character into the world.

**Its own account, and it is gone.** Master `Tfivepress` on account `YULONT5` (id 114), both made by
this lane and both erased at the end — verified: no `YULONT5`, no `Tfivepress`, **no orphaned
`account_access` rows**, `group_member` empty. `PERZI`'s `last_login` still reads 2026-09-08 23:24:16
and `Pakka` is still level 6; `LootPet2.lua` untouched. The realm row was never touched
(`100.99.204.5` on `address` and `localAddress`, mask `255.255.255.0`), so no authserver restart was
owed or made. `Logger.ALE=4,Console Server` still at line 706.

## What this does NOT show

* **The chosen level working.** It is the one clause of 8.6's line this run could not prove, and the
  page leads with it rather than burying it.
* **A chosen spec taking effect.** This install has no premade specs deployed, so `talents spec` had
  nothing to pick from. What is proved is that the app now offers exactly what the server has, that
  an unlisted name is refused by the app before it is sent, and that the module's refusal is
  invisible over the channel. A tree with a deployed `playerbots.conf` has not been pressed.
* **The talent readback.** With zero specs loaded there was nothing to read back;
  `character_talent` held no rows for the bot.
* **The panel and the client in one frame.** Captured separately, on two machines; a reader takes it
  on the timestamps that the bot in `client-2` is the row in the panel at 13:01.
* **The whole app.** These are the shipped `PartyPanel` in a window built by `panel86t5.py` with
  `_build_my_party_group`'s own two lines; the tab around it, and reaching it from a cold start, are
  not photographed here.
* **One class per press, not ten.** warlock, mage, priest and hunter were pressed; six of
  `party.BOT_CLASSES` were not.
* **`8.6-panel-live-…/client-agent.log` carries a password** (`slow PANELGATE86`, line 32). This
  folder's agent redacts it (`secret <13 chars>`, `client-agent.log:33`) — the earlier folder is
  left as it was written, and this sentence is the correction.
