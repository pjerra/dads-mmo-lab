# 8.4a — Play, WoW WotLK — server side gated live on yulon-ubuntu, 2026-09-07

**Not ticked.** Every clause that can be proved without a person in the game is
green; the one that needs a character somebody is actually playing is not, and
the reason is recorded below rather than worked around.

Server: the AzerothCore install on `yulon-ubuntu` (`~/wowserver`, rev
`413bea61a85e`, playerbots). Code: `c3d5011b`.

## What is proved

| clause | reading |
| --- | --- |
| every action on an offline character | teleport, set level, rename, revive and mailed gold, each read back in the row |
| a name typed in the wrong case | `aevret`, `AEVRET` and `aEVRET` all found `Aevret`; a name nobody has was refused before the server was asked |
| a gear set larger than one mail | 19 pieces, the button promised 2 mails, 2 arrived |
| the tab draws what the tree has | every button named its character: `Teleport Aevret`, `Send Aevret's 12 worn items (1 mail)` |

Screenshots `1-characters.png` (1001 characters, live), `2-chosen.png` (the
buttons naming the chosen one) and `3-teleported.png` (the tab reading the
server's own sentence back).

## What is not, and why

**The client half.** The definition of done asks for the effect in the game
client — the character standing at the destination, the mail in the mailbox, the
level on the character frame. Doing that needs a character somebody is playing,
and **this server has no people on it**: 1001 characters, all of them bots.

The way in was to take over a bot's account with 8.3a's own password button and
log in as it. That worked as far as the realm list — `5-client-realm-list.png`
is the real 3.3.5a client authenticated as `RNDBOT56` with the password this app
set, which is a second live press of 8.3a — and then stopped: after the realm
list the client returns to a login screen reading "Connected"
(`6-client-login-hangs.png`), three attempts, and no character ever reached the
world. The likeliest reason is the one the box cannot work around: the
playerbots module owns those characters' sessions, and a bot account is not an
empty account with a spare seat in it.

So the honest state is: the server half is done and the client half needs a
character a person owns — the same shape as 8.1d, which the owner logged into by
hand. Two minutes of somebody's time closes it.

**The online clause** is in the same place for the same reason. It ran against
an online BOT and the readings were not usable: `character level` answered
"You changed level of Airaani to 60." and the row still read 72 half a minute
later, because the module maintains its bots' levels. A character a person is
playing has no such second author.

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
