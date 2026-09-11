#!/bin/bash
# PR 491 live press on m910q: fresh databases from the image's own sql/, the
# owner's extracted data bound READ-ONLY, one mangosd from the pr491-soap-on
# image, SOAP pressed with curl and with Yu'lon's own soap.py, graceful stop.
# Throwaway names, all prefixed pr491-; removed at the end. Nothing of the
# owner's install is written.
set -u
IMG=yulon.local/cmangos-tortoise-server:pr491-soap-on
NET=pr491-net; DB=pr491-db; MG=pr491-mangosd; VOL=pr491-dbdata
ROOTPW=pr491root; APPPW=pr491app; ADMINPW=PR491SECRET; LOWPW=PR491LOW
OUT=$HOME/pr491/press; mkdir -p "$OUT"; cd "$OUT"
log(){ printf "[%s] %s\n" "$(date -u +%H:%M:%SZ)" "$*" | tee -a press.log; }
cleanup(){
  log "cleanup: stopping and removing pr491-* containers, volume, network"
  docker rm -f $MG $DB pr491-sqlsrc >/dev/null 2>&1
  docker volume rm $VOL >/dev/null 2>&1; docker network rm $NET >/dev/null 2>&1
  rm -rf "$OUT/sql"
  docker ps -a --format '{{.Names}}' | grep -c '^pr491-' | xargs -I{} log "pr491-* containers left: {}"
}
trap cleanup EXIT
log "press start; image $IMG"
docker network create $NET >/dev/null 2>&1
docker run -d --name $DB --network $NET -v $VOL:/var/lib/mysql -e MARIADB_ROOT_PASSWORD=$ROOTPW mariadb:10.6 >/dev/null || { log "db start FAILED"; exit 1; }
for i in $(seq 1 90); do docker exec $DB healthcheck.sh --connect --innodb_initialized >/dev/null 2>&1 && break; sleep 2; done
log "db healthy after ~$((i*2))s"
docker create --name pr491-sqlsrc $IMG >/dev/null
docker cp pr491-sqlsrc:/opt/tortoise/sql ./sql && docker cp pr491-sqlsrc:/opt/tortoise/etc/mangosd.conf.dist ./mangosd.conf
docker rm pr491-sqlsrc >/dev/null
M(){ docker exec -i $DB mariadb -uroot -p$ROOTPW "$@"; }
t0=$(date +%s)
M < sql/create_databases.sql && log "schemas: create_databases.sql ok ($(grep -c 'CREATE TABLE' sql/create_databases.sql) tables)" || { log "schemas FAILED"; exit 1; }
M <<SQL || { log "grants FAILED"; exit 1; }
CREATE USER IF NOT EXISTS 'mangos'@'%' IDENTIFIED BY '$APPPW';
GRANT ALL ON tw_world.* TO 'mangos'@'%'; GRANT ALL ON tw_char.* TO 'mangos'@'%';
GRANT ALL ON tw_logon.* TO 'mangos'@'%'; GRANT ALL ON tw_logs.* TO 'mangos'@'%'; FLUSH PRIVILEGES;
SQL
n=0; for f in $(ls sql/base/*.sql | sort); do M tw_world < "$f" || { log "world base FAILED at $f"; exit 1; }; n=$((n+1)); done
log "world base: $n files into tw_world in $(( $(date +%s)-t0 ))s"
M tw_logon <<SQL || { log "realm/accounts FAILED"; exit 1; }
REPLACE INTO realmlist (id,name,address,port,icon,realmflags,timezone,allowedSecurityLevel,population,realmbuilds) VALUES (1,'PR491','127.0.0.1',8085,0,0,0,0,0,'7272');
INSERT INTO account (username, sha_pass_hash, rank) VALUES ('PR491ADMIN', UPPER(SHA1('PR491ADMIN:$ADMINPW')), 4);
INSERT INTO account (username, sha_pass_hash, rank) VALUES ('PR491LOW',   UPPER(SHA1('PR491LOW:$LOWPW')), 1);
SELECT id, username, rank FROM account;
SQL
log "accounts: PR491ADMIN rank 4, PR491LOW rank 1 (created BEFORE start: this core reads rank at startup)"
setkey(){ sed -i -E "s|^$1 = .*|$1 = $2|" mangosd.conf; grep -q -E "^$1 = " mangosd.conf || echo "$1 = $2" >> mangosd.conf; }
setkey LoginDatabase.Info "\"$DB;3306;mangos;$APPPW;tw_logon\""
setkey WorldDatabase.Info "\"$DB;3306;mangos;$APPPW;tw_world\""
setkey CharacterDatabase.Info "\"$DB;3306;mangos;$APPPW;tw_char\""
setkey LogsDatabase.Info "\"$DB;3306;mangos;$APPPW;tw_logs\""
setkey Database.AutoUpdate.Enabled 1
setkey Database.AutoUpdate.Path "\"/opt/tortoise/sql/database_updates/\""
setkey DataDir "\"/opt/tortoise/data\""
setkey LogsDir "\"/tmp\""
setkey RealmID 1
setkey Console.Enable 0
setkey GameType 0
setkey AutoHonorRestart 0
setkey AutoRestart.MaxServerUptime 0
setkey SOAP.Enabled 1
setkey SOAP.IP 0.0.0.0
setkey SOAP.Port 7878
grep -n -E '^(SOAP\.|Console.Enable|RealmID|DataDir|LogsDir|Database.AutoUpdate.(Enabled|Path))' mangosd.conf | tee -a press.log
t1=$(date +%s)
docker run -d --name $MG --network $NET -p 127.0.0.1:17878:7878 \
  -v "$HOME/tortoise-server/data:/opt/tortoise/data:ro" \
  -v "$OUT/mangosd.conf:/opt/tortoise/etc/mangosd.conf:ro" \
  $IMG ./mangosd -c /opt/tortoise/etc/mangosd.conf >/dev/null || { log "mangosd start FAILED"; exit 1; }
READY='World server is up and running|World initialized|MaNGOS.*started up successfully|Ready to login'
FATAL='Correct \*.map files not found|Could not open (?!.*Logging to it is off for this run)|Database .* not found|\[1146\] Table .* doesn.t exist|Your database structure is not up to date'
state=none
for i in $(seq 1 180); do
  docker logs $MG 2>&1 > world.log
  if grep -q -E "$READY" world.log; then state=ready; break; fi
  if grep -q -P "$FATAL" world.log; then state=fatal; break; fi
  if [ "$(docker inspect -f '{{.State.Running}}' $MG)" != true ]; then state=exited; break; fi
  sleep 5
done
log "world: $state after $(( $(date +%s)-t1 ))s; ready line: $(grep -m1 -E "$READY" world.log)"
grep -n -i 'soap' world.log | tee -a press.log
[ "$state" = ready ] || { log "not ready; last lines:"; tail -15 world.log | tee -a press.log; exit 1; }
sleep 3
ENV='<?xml version="1.0" encoding="utf-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ns1="urn:MaNGOS"><SOAP-ENV:Body><ns1:executeCommand><command>server info</command></ns1:executeCommand></SOAP-ENV:Body></SOAP-ENV:Envelope>'
press(){ # name user:pass
  code=$(curl -s -o "curl-$1.xml" -w '%{http_code}' -u "$2" -H 'Content-Type: text/xml; charset=utf-8' -H 'SOAPAction: ""' --data "$ENV" http://127.0.0.1:17878/)
  log "curl $1 -> HTTP $code :: $(tr -d '\r\n' < "curl-$1.xml" | grep -oE '<(result|faultstring)>.{0,160}' | head -1)"
}
press admin "PR491ADMIN:$ADMINPW"
press wrongpw "PR491ADMIN:nope"
press lowrank "PR491LOW:$LOWPW"
press nouser "PR491NOBODY:x"
log "--- Yu'lon soap.py (the app's own client, namespace urn:MaNGOS)"
python3 - <<PY 2>&1 | tee -a press.log
import sys; sys.path.insert(0, "$OUT")
import soap_press as soap
def go(acct, pw, cmd):
    r = soap.execute(soap.Endpoint("127.0.0.1", 17878, acct, pw, namespace="urn:MaNGOS"), cmd, timeout=30)
    print(f"soap.py {acct} {cmd!r} -> outcome={r.outcome} http={r.http_status} text={r.text[:200]!r}")
go("PR491ADMIN", "$ADMINPW", "server info")
go("PR491ADMIN", "$ADMINPW", "account create PR491VIASOAP Passw0rd")
go("PR491ADMIN", "$ADMINPW", "thiscommanddoesnotexist")
go("PR491ADMIN", "nope", "server info")
go("PR491LOW", "$LOWPW", "server info")
PY
M tw_logon -e "SELECT id, username, rank FROM account" | tee -a press.log
t2=$(date +%s)
docker stop -t 120 $MG >/dev/null; rc=$(docker inspect -f '{{.State.ExitCode}}' $MG)
log "graceful stop: $(( $(date +%s)-t2 ))s, exit code $rc"
docker logs $MG 2>&1 > world.log; tail -6 world.log | tee -a press.log
log "press done"
