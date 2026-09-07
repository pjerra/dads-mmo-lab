# 8.5a — Browse Bots, WoW WotLK — live gate

**Where and when.** `yulon-ubuntu`, 2026-09-07 01:19Z – 01:26Z, against the finished 7.2 WotLK
install at `~/wowserver`, code at `4fb4a4c5`. The who-list half ran on the Hyper-V host with
`C:\clients\WoW-WotLK-3.3.5a-min`, in the interactive session, using the same recipe 8.3a's gate
established.

## What was proved

| Definition of done | Result |
|---|---|
| The total equals the same clause run by hand | **1000 = 1000**, out of 1001 characters on the server |
| The split by marker type is shown | `1000 by the playerbots registry, 0 by the account prefix`, and each half checked separately against the database |
| An unreadable marker refuses to answer | `total=None`, "this install's bot marker could not be read, and an empty marker would match every account on the server" |
| A readable marker matching nothing while characters exist warns, neither reporting zero | **the box's wording does not describe this tree** — see below |
| A listed online bot is found by the in-game who-list | **`[Paelat] Level 4 Draenei Shaman - Azuremyst Isle`, "1 player total"**, `4-who-list.png` |
| (paging, which the list needs to be usable at all) | two pages of 50 with no overlap; a filter on three letters returned exactly its one match |

## What the machine refuted

**A marker matching nothing does not make the total zero on this tree, and must not warn.**
The clause has two arms — the playerbots registry and the account-name prefix — and the registry
answers whether or not the prefix does. Asked with `NOSUCHBOT`, this server still returned 1000
bots, all of them found by the registry:

```
a marker matching nothing: total=1000 by_registry=1000 by_prefix=0 warning=''
```

That is the split doing exactly the job it was built for. The box's clause was written from 8.1a,
where the same question is asked of a *count* with no second signal in view; here the honest
behaviour is to answer, and to say which signal answered. The warn path is real and is exercised by
`test_a_marker_matching_nothing_while_characters_exist_warns`; it will be **gated live on 8.5c and
8.5d**, where the CMaNGOS trees have only the prefix and the state is reachable.

**On this install the prefix is carrying nothing.** `by_prefix=0` means every one of the 1000 bot
characters is in the registry, so the name-prefix arm currently identifies none that the registry
would miss. Worth knowing before anyone decides the prefix is the marker.

**The in-game who-list is faction-filtered.** The first attempt asked about `Aberenz` — listed
online by the app, level 59 — and the client answered `0 players total`. `Aberenz` is race 5,
Undead: Horde, and the asking character is a Gnome. The second attempt used `Paelat`, race 11,
Draenei, and found it. Nothing was wrong with the list; a who-list gate has to pick a bot the asking
character can see.

## The screenshots

| File | What it shows |
|---|---|
| `1-bots.png` | the Bots tab: 1000 bots, the split by signal, and the first fifty |
| `2-page-two.png` | the next page, 51–100 |
| `3-filtered.png` | a three-letter filter, and the one row it matched |
| `4-who-list.png` | the client in the world, and `/who` finding a bot the tab listed |

## After the gate

The adversarial review that followed replaced `OFFSET` paging with a cursor —
`(name, guid)` — because the list is read live and `OFFSET` counts rows: one bot
logging out before the boundary shifts every later page by one. The screenshots
here were retaken against that version, which is why the summary reads
"Page 2, 50 shown" rather than a row range: the rows are read by cursor and a
range would be a count the tab does not have.

## How to re-run it

```
python gate85a.py  total | split | refuse | page | shots
```

and, on the Hyper-V host, `client-who.ps1` through the `schtasks /it` runner 8.3a's gate carries.
The who-list account's password was set **through 8.3a's own Set-password button**, which is how a
gate for one box came to use the feature from another.
