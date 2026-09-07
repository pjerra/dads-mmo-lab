# 8.4b — Play, WoW TBC — server side gated live on m910q, 2026-09-07

**Not ticked.** Every clause that does not need the game client is green; the
client half is one screen from done and is written up at the end so tomorrow's
attempt starts where this one stopped.

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

## The client half, and where it stopped

The account `TBCGATE` owns `Ddsasd`, the one character on this server that is
not a bot. Its password was set through 8.3a's own button and the real 2.4.3
client logged in with it — it is at the realm-choosing screen in
`tbcplay-w02.png`, which is as far as 8.3b ever needed to go.

Three things had to be learnt to get that far, and the third is the one that
cost the evening:

1. This client publishes **no `MainWindowHandle` and no `GxWindowClass`** while
   it is starting, so a driver that clicks by window coordinates clicks 0,0 —
   the desktop — and types the login at whatever is there.
2. The reason there was no window: **"Hardware changed. Reload default
   settings?"**, a Windows message box this client puts up when `Config.wtf` has
   been edited, BEFORE it creates its own window. The driver's own windowed-mode
   rewrite caused it. An ENTER after launch answers it, and the window handle
   appears immediately afterwards.
3. Its realm dialog is not where the 3.3.5a client's is, so the click that
   dismisses one does not dismiss the other. That is the screen this box stops
   on.
