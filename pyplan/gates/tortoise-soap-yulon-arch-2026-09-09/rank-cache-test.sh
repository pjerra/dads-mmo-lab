#!/usr/bin/env bash
# Third attempt, and the first with an uncontaminated "before".
#
# Attempt two (20:21Z) read HTTP 200 on both sides of its restart and called the refusals a
# password problem. Its "before" was not before anything: attempt one had already restarted the
# world at 20:20:13Z, four minutes after YULONCHK3 was created, so by the time attempt two asked
# its first question the server had reloaded from the database anyway. The restart it was trying
# to measure had already happened.
#
# So: a brand-new account, made right now with rank 4 while the world runs, asked once before any
# restart and once after. Nothing else changes between the two questions. The password is written
# here in the scheme the fork's AccountMgr::CheckPassword uses, so a refusal cannot be the password.
set -u
DIR=/home/pk/tortoise-vm
ACC=YULONCACHE1
PASS=TESTPASS1
say() { echo "[$(date -u +%H:%M:%SZ)] $*"; }
PW=$(grep -E '^DB_ROOT_PASSWORD=' "$DIR/.env" | cut -d= -f2-)
[ -n "$PW" ] || { say "FATAL: no DB_ROOT_PASSWORD in $DIR/.env"; exit 2; }
q() { docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb -u root -N -e "$1"; }

say "GROUND: uptime of the world right now (it must not have restarted since $ACC exists)"
docker inspect -f '{{.State.StartedAt}}' tortoise-mangosd
say "GROUND: does $ACC exist yet? $(q "SELECT COUNT(*) FROM tw_logon.account WHERE username='$ACC'")"

HASH=$(printf '%s' "$ACC:$PASS" | tr '[:lower:]' '[:upper:]' | sha1sum | cut -d' ' -f1 | tr '[:lower:]' '[:upper:]')
q "INSERT INTO tw_logon.account(username,sha_pass_hash,joindate,\`rank\`) VALUES('$ACC','$HASH',NOW(),4)"
say "made $ACC with rank 4 while the world runs: $(q "SELECT id,username,\`rank\` FROM tw_logon.account WHERE username='$ACC'")"

soap() {
  curl -s -o /tmp/soap3.out -w '%{http_code}' --max-time 25 -u "$ACC:$PASS" \
    -H 'Content-Type: application/xml' -X POST "http://127.0.0.1:7878/" \
    -d '<?xml version="1.0" encoding="utf-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ns1="urn:MaNGOS"><SOAP-ENV:Body><ns1:executeCommand><command>server info</command></ns1:executeCommand></SOAP-ENV:Body></SOAP-ENV:Envelope>'
}
body() { tr -d '\n' < /tmp/soap3.out | grep -oE '<faultstring>[^<]*</faultstring>|<result>[^<]{0,90}' | head -2; }
wait_for_soap() {
  for i in $(seq 1 60); do
    c=$(soap); [ "$c" != "000" ] && { say "SOAP answering after $((i*10))s"; return 0; }
    sleep 10
  done
  say "SOAP never answered in 600s"; return 1
}

CODE=$(soap); say "BEFORE any restart: HTTP $CODE :: $(body)"
say "restarting the world, nothing else changed"
docker restart tortoise-mangosd >/dev/null
sleep 20
wait_for_soap || exit 2
CODE2=$(soap); say "AFTER the restart: HTTP $CODE2 :: $(body)"
say "GROUND: the row never changed: $(q "SELECT \`rank\`, sha_pass_hash='$HASH' FROM tw_logon.account WHERE username='$ACC'")"

if [ "$CODE" != "200" ] && [ "$CODE2" = "200" ]; then
  say "VERDICT: THE RANK IS READ AT STARTUP. An account granted its rank while the world runs is refused until the world restarts, with the row correct the whole time."
elif [ "$CODE" = "200" ]; then
  say "VERDICT: no restart needed -- a rank written while the world runs is honoured at once."
else
  say "VERDICT: refused on both sides; a restart is not the missing piece. Read the bodies."
fi
say DONE
