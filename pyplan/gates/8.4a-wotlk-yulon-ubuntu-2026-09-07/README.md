# 8.4a — Play, WoW WotLK — server side gated live on yulon-ubuntu, 2026-09-07

Every clause is green, including the ones that need a character standing in the
world — sent from the other side of the machine while a real 3.3.5a client had
it logged in.

Server: the AzerothCore install on `yulon-ubuntu` (`~/wowserver`, rev
`413bea61a85e`, playerbots). Code: `c3d5011b`.

## What is proved

| clause | reading |
| --- | --- |
| every action on an offline character | teleport, set level, rename, revive and mailed gold, each read back in the row |
| a name typed in the wrong case | `aevret`, `AEVRET` and `aEVRET` all found `Aevret`; a name nobody has was refused before the server was asked |
| a gear set larger than one mail | 19 pieces, the button promised 2 mails, 2 arrived |
| the tab draws what the tree has | every button named its character: `Teleport Aevret`, `Send Aevret's 12 worn items (1 mail)` |
| every action on a character being played | teleport, level, mailed gold and rename, each seen in the client — the table below |

Screenshots `1-characters.png` (1001 characters, live), `2-chosen.png` (the
buttons naming the chosen one) and `3-teleported.png` (the tab reading the
server's own sentence back).

## The client half, on a character a person was playing

The definition of done asks for the effect in the game client. This server has
1001 characters and 1000 of them are bots — but the thousand-and-first is
`Asfgg` on `YULONGATE`, a character the owner made by hand for an earlier box.
Its account's password was set through 8.3a's own button (a second live press of
that feature), a real 3.3.5a client logged in with it, and the actions were sent
from the other side of the machine while it stood in the world.

| what was sent | what the client showed |
| --- | --- |
| `teleport name Asfgg Orgrimmar` | the character in **Valley of Strength**, and *"You are being teleported by server console command."* in the log (`6-client-orgrimmar-level-20.png`) |
| `character level Asfgg 20` | the portrait reading **20**, the achievements for levels 10 and 20, and *"Server console command level up you to [20]"* |
| `send money Asfgg "A gift" … 70000` | the **envelope** on the minimap — mail waiting (`8-client-mail-envelope.png`) |
| `teleport name Asfgg Stormwind` | the **Trade District**, and *"Changed Channel: [1. General - Stormwind City]"* (`7-client-stormwind.png`) |
| `character rename Asfgg` | at the next login: **"Your name has been flagged for rename — Please enter a new name"**, over a character panel reading *Asfgg, Level 20 Warrior, Stormwind City* (`9-client-rename-prompt.png`) |

That last frame carries three of them at once: the rename prompt the flag
produces, the level the command set, and the city the teleport moved it to.

`revive` has no visible effect on a character who is not dead; the server
answered it and the row was read.

**The bot route did not work, and that is worth recording.** The first attempt
took over a bot's account the same way. It authenticated — `5-client-realm-list.png`
is that client at the realm list — and then never reached the world, three
times, returning to a login screen reading "Connected". The playerbots module
owns those characters' sessions. An online BOT is no good for the online clause
either: `character level` answered "You changed level of Airaani to 60." and the
row still read 72 half a minute later, because the module maintains its bots.

## Three things this box measured

**1. The server answers before its own write lands.**

    level 55 -> 58: the server answered in 0.18s, the row changed after 0.30s
    level 58 -> 59: the server answered in 0.15s, the row changed after 0.26s

A tab that re-read its list the moment the command answered would show the state
BEFORE the thing that was just done — "You change the level of Aevret to 60"
above a row still reading 55, which reads as the action having failed. The
sentence is shown at once and the list refresh is scheduled (`_ROW_SETTLE_MS`,
750ms, that measurement with room). `answer-beats-the-write.py` is the probe.

**2. For an ONLINE character the world holds the truth, not the row.** The
teleport moved the character and `characters.position_x` did not change until
the world was asked to save. This is the limit of 8.3b's "the row is the answer":
it holds for a character nobody is playing.

**3. `.revive` takes a name, whatever its help says.** The command documents no
argument at all — *"Revive the selected player. If no player is selected, it
will revive you."* — and `revive NOSUCHCHARACTER` answered *"Character
'Nosuchcharacter' does not exist."* A feature built from that help would have
drawn no revive button on the one tree that has one.

## And two the gate got wrong about itself

The first run asserted the character had moved and read the row immediately: the
app was right and the box was wrong. The second teleported to Stormwind a
character already standing in Stormwind and called that a failure. Both are
recorded because a gate that fails on the app's behalf is how a working feature
gets "fixed".
