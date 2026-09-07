# 8.4b — Play, WoW TBC — server side gated live on m910q, 2026-09-07

Every clause is green, the client half included: the real 2.4.3 client stood in
the world while the commands were sent from the other side of the machine.

Server: the 7.4c TBC install on `m910q` (`~/tbc-7.4c`, CMaNGOS 0.18). Code
`5b31bd03`.

## What is proved

| clause | reading |
| --- | --- |
| every action on an offline character | `tele name` moved it (map 530 to map 0), level 53 to 60, `at_login` 0 to 1, mails 2 to 3 |
| a name typed in the wrong case | `bimokk`, `BIMOKK` and `bIMOKK` all found `Bimokk`; a name nobody has was refused before the server was asked |
| a gear set larger than one mail | 19 pieces, the button promised 2 mails, 2 arrived |
| revive, offline | answers success and changes nothing, as on WotLK — which is why the tab does not offer it there |

## What this tree calls things, asked rather than inherited

**`teleport` is not a command here.** Asked directly, this server answers *"There
is no such command"*; the verb is `tele`, whose subcommands are `del` and
`name`, and `.tele name [#playername] #location` says *"Character can be
offline"* exactly as AzerothCore's does. A constant in the app would have drawn
a working button on one tree and, on this one, a refusal that arrives as a
**closed connection** — which is precisely the shape 8.3b taught this app to
report as `silent` rather than as a dead channel:

    'teleport name Ddsasd Orgrimmar' -> unknown
        http://127.0.0.1:7878/ took the command and closed the connection without answering

**The mail cap was measured by exceeding it** (`mail-cap.py`), not read off a
constant in somebody's core:

    12 items -> yes: 'Mail sent to Ddsasd'     one mail, 12 items in it
    13 items -> refused, connection closed     no mail, no split

So twelve is the cap and thirteen is not "twelve and one over" — it is nothing
at all. A gear set therefore has to be batched by the app, which is what the
button promises before the press.

**The equipped set is one join here.** `character_inventory` carries
`item_template` beside `item`, where AzerothCore's row carries only the instance
guid — so this tree reads the id straight off the inventory row. The wrong shape
would answer a list of instance guids that look exactly like item ids.

## The client half

`TBCGATE` owns `Ddsasd`, the one character on this server that is not a bot. Its
password was set through 8.3a's own button, the real 2.4.3 client logged in with
it, and the actions were sent from `m910q` while it stood in the world.

| what was sent | what the client showed |
| --- | --- |
| `tele name Ddsasd Orgrimmar` (the gate's own stage) | the character panel reading **Orgrimmar** (`4-client-character-orgrimmar.png`) |
| `tele name Ddsasd Stormwind` | in the world, **Elwynn Forest**, the Eastern Kingdoms — a different continent from where it started (`5-client-in-world-level-25.png`) |
| `character level Ddsasd 25` | the portrait reading **25**, and the row reading 25 after the logout saved it |
| `send money Ddsasd "A gift" … 90000` | the **envelope** on the minimap (`6-client-mail-envelope.png`) |
| `character rename Ddsasd` | at the next login: **"Your name has been flagged for rename"** (`7-client-rename-prompt.png`) |

The row after that session read `Ddsasd 25 0 1 0` — level 25, offline, rename
flagged, map 0 — so what the world held while it was played is what the database
holds now.

## Four things this client needed that the 3.3.5a one did not

Each was found by looking at what came back, and each is in `client-login.ps1`:

1. **"Hardware changed. Reload default settings?"** — a plain Windows message box
   this client puts up when `WTF\Config.wtf` has been edited, BEFORE it creates
   its game window (`hardware-changed-dialog.png`). So there is no window to
   find: `MainWindowHandle` is 0, `GxWindowClass` finds nothing, and every click
   goes to 0,0 — the desktop. An ENTER eight seconds after launch answers it and
   the handle appears at once. Three runs were lost to this.
2. **The realm screen wants a language ticked first.** Its "Development"
   checkbox is what turns "Suggest Realm" from grey to red
   (`3-client-development-ticked.png`); the 3.3.5a client shows a realm list
   straight away and has neither control.
3. **Then "Suggest Realm", then Accept** — *"You have been assigned to the MaNGOS
   Realm."* — and only then the character screen.
4. **The realm dialog reappears if Okay is clicked during "Logging in to game
   server".** It has to wait for the load, and clicking it early simply reopens
   it, twice measured.

And one that was not the client's fault at all: **the realm row advertised
`192.168.10.134`**, m910q's LAN address, which the Hyper-V host cannot reach. The
client authenticated against realmd over Tailscale and was then told to connect
to an address on another network, so the realm dialog came back for ever. Fixed
through **Yu'lon's own Networking plan and apply** — `realmlist → 100.78.24.50`,
which is a live press of that feature rather than a row written by hand.

## The rename flag survives a live session

`character rename` was sent while the character was ONLINE and the flag was
still there after the logout, which is not obvious: the logout save writes the
player's own row, and it could have overwritten it. On this client the prompt
appears when **Enter World** is pressed rather than on arrival at the character
screen, which is where the 3.3.5a client shows it.
