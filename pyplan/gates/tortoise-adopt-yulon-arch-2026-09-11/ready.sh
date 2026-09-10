#!/bin/bash
S=1789078528
PAT="World server is up and running|World initialized|MaNGOS.*started up successfully|Ready to login"
echo "# The worldserver ready line, bounded to THIS run"
echo "stamp: $(date -Is)"
echo
echo "Start was pressed on the Server tab at 2026-09-10T22:15:28+00:00 (epoch $S)."
echo "docker logs is cumulative on a restarted container, so every window below is an"
echo "epoch --since, the way the m910q gate bounded its own."
echo
echo "## the window before the containers came up: --since $S --until $((S+2))"
echo "lines: $(docker logs --since $S --until $((S+2)) tortoise-mangosd 2>&1 | wc -l)"
echo "ready-marker matches: $(docker logs --since $S --until $((S+2)) tortoise-mangosd 2>&1 | grep -cE "$PAT")"
echo
echo "## the whole run window: --since $S"
echo "lines: $(docker logs --since $S tortoise-mangosd 2>&1 | wc -l)"
echo "ready-marker matches (the catalog entry's own ready.world pattern): $(docker logs --since $S tortoise-mangosd 2>&1 | grep -cE "$PAT")"
echo "the line itself:"
docker logs --since $S tortoise-mangosd 2>&1 | grep -E "$PAT" | sed "s/^/  > /"
echo
echo "## realmd, last lines in the window"
docker logs --since $S tortoise-realmd 2>&1 | tail -6 | sed "s/^/  > /"
echo
echo "## listening ports (auth 3724, world 8090 from the catalog entry)"
ss -ltn | grep -E "3724|8090" | sed "s/^/  > /"
echo
echo "## containers"
docker ps --format "{{.Names}}|{{.Status}}" | sed "s/^/  > /"
