# m910q: the redundant `idx_owner_bot_event` dropped — 2026-09-10

The owner's answer to open question 5 (through the question tool, 2026-09-10 afternoon): "Drop it now".
The lead did it by hand; the record is `index-drop.txt`, every line written by the script with the
box's clock.

What it shows: the three Tortoise containers exited (28 hours); the database started alone with
`docker compose up -d --no-deps tortoise-db` and healthy after 8 s; `tw_char.ai_playerbot_random_bots`
carried both `idx_owner_bot_event` (non-unique, owner,bot,event) and `uq_owner_bot_event` (unique, the
same three columns); one `ALTER TABLE ... DROP INDEX idx_owner_bot_event`, rc 0, 8938 rows; the index
gone and the unique one kept; the database stopped again; the world never started. The client was
given its password through `MYSQL_PWD` in the container's environment, never an argument, and the
record was grepped for it before this commit (0 matches).

A press of the T11 route on this install would re-create the index from the fork's `character_updates`
file until T19's first half lands (the probe learning completeness on an unmarked install); the owner
knows.
