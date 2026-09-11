# 8.5c clause 4 — a listed online bot is found by the in-game who-list

The server side of 8.5c left this clause NOT RUN and handed over everything it
needed: a reachable realm row, a named asker, named subjects, and the reason the
subjects had to share the asker's race. It was asked and answered on
2026-09-07 at 22:06–22:08 UTC, from the same 1.12.1 session that finished
8.4c's client half. The driver and the client facts are in
`../8.4c-vanilla-m910q-2026-09-07/client-login-vanilla.ps1`; this file is only
the clause.

## The reading

One frame carries the whole thing — `8-client-who-two-found-two-not.png`, the
client's own chat log:

    [Ashah]: Level 3 Tauren Warrior - The Barrens
    1 player total
    0 players total
    0 players total
    [Ehpih]: Level 3 Tauren Druid - Mulgore
    1 player total

Four questions, in this order, from `Asdffrenamed` (guid 901, account `VANGATE`,
gmlevel 0 — 8.5c's `Asdff`, renamed mid-session by 8.4c's rename clause and
renamed back at the end):

| asked | answered | why that is the right answer |
| --- | --- | --- |
| `/who Ashah` | **1 player total**, Level 3 Tauren Warrior | the app had already claimed `Ashah` as one of its 900 bots and online, photographed in `4-page-two.png` **before** the client was touched |
| `/who Acharon` | **0 players total** | `Acharon` is race 1, Human, ALLIANCE, and was online at 22:06:41Z. `AllowTwoSide.WhoList = 0` and the asker is gmlevel 0, so `MiscHandler.cpp`'s faction filter applies to it — 8.5c predicted this from the source and here it is measured |
| `/who Nosuchbothere` | **0 players total** | nobody has that name |
| `/who Ehpih` | **1 player total**, Level 3 Tauren Druid | a second bot off the same list, same race as the asker |

`7-client-who-ashah.png` is the first answer on its own, before the others were
asked, so the positive is not read out of a screen that already had four lines
on it.

**The two zeros are what make the two ones worth having.** A `/who` that
answered "1 player total" to everything would prove nothing about the bot list,
and one that answered 0 to everything would look exactly like a broken feature.
Here the same command, in the same session, seconds apart, separates a bot the
app listed from a bot it listed that the server is entitled to hide, and both
from a name nobody has.

The DB was read at 22:06:41Z, between entering the world and asking: `Acharon|4|1|1`,
`Asdffrenamed|60|6|1`, `Ashah|3|6|1` — so all three were online at the moment of
asking, and the two that came back matched the client's own answer for level and
(via race 6) for faction.

## What did not have to be worried about after all

8.5c listed as blocked: *"the level range the 1.12.1 client puts in a /who packet
is unmeasured … if the client sends a narrow range the positive find could come
back empty for a reason that is not the bot list."* The asker was level 60 by the
time it asked (8.4c had levelled it) and both subjects it found were level 3, so
whatever range this client puts in the packet for a bare `/who <name>` **includes
level 3** — the hazard is closed for this shape of question. It is not closed in
general: nothing here measures the top of that range, and this server's bots are
not all low-level (`Amelias`, guid 640, is a level 60 Human and was online). A
lane that wants to ask about a high-level bot should measure it rather than
assume this reading covers it.

The other blocker, `VANGATE`'s password, was in 8.4c's README all along:
`gate84c-p@ss`, set through 8.3a's own button. It worked on the first attempt.

## The realm row

8.5c fixed it through Yu'lon's Networking feature and left it advertising
`100.78.24.50`. This lane checked before launching anything, from the client box
rather than from the server: at about 21:41Z `Test-NetConnection 100.78.24.50 -Port 3724`
and `-Port 8085` were both True, and the row still read
`1|MaNGOS|100.78.24.50|8085`. Nothing needed re-pressing, and the client
authenticated and reached the world on its first try.

## Files

* `7-client-who-ashah.png` — the first answer, alone
* `8-client-who-two-found-two-not.png` — all four, in one chat log
