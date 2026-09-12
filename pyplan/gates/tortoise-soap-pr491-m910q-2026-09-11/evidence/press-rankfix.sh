#!/bin/bash
# PR 491 rank-fallback press on m910q. Usage: press-fix.sh <image> <label>
# Fresh databases from the image's own sql/, the owner's extracted data bound
# READ-ONLY, one mangosd from <image>, the admin account inserted by SQL AFTER
# the world is up, then SOAP pressed with curl. Throwaway names prefixed p491f-.
set -u
IMG=$1; LABEL=$2
NET=p491f-net; DB=p491f-db; MG=p491f-mangosd; VOL=p491f-dbdata
ROOTPW=p491froot; APPPW=p491fapp; ADMINPW=P491FSECRET; EARLYPW=P491FEARLY
OUT=$HOME/pr491fix/press-$LABEL; mkdir -p "$OUT"; cd "$OUT"
log(){ printf "[%s] %s\n" "$(date -u +%H:%M:%SZ)" "$*" | tee -a press.log; }
cleanup(){
  log "cleanup: removing p491f-* containers, volume, network"
  docker rm -f $MG $DB p491f-sqlsrc >/dev/null 2>&1
  docker volume rm $VOL >/dev/null 2>&1; docker network rm $NET >/dev/null 2>&1
  rm -rf "$OUT/sql"
  log "p491f-* containers left: $(docker ps -a --format '{{.Names}}' | grep -c '^p491f-')"
}
trap cleanup EXIT
log "press $LABEL start; image $IMG ($(docker inspect -f '{{.Id}}' $IMG | cut -c8-19))"
docker network create $NET >/dev/null 2>&1
docker run -d --name $DB --network $NET -v $VOL:/var/lib/mysql -e MARIADB_ROOT_PASSWORD=$ROOTPW mariadb:10.6 >/dev/null || { log "db start FAILED"; exit 1; }
for i in $(seq 1 90); do docker exec $DB healthcheck.sh --connect --innodb_initialized >/dev/null 2>&1 && break; sleep 2; done
log "db healthy after ~$((i*2))s"
docker create --name p491f-sqlsrc $IMG >/dev/null
docker cp p491f-sqlsrc:/opt/tortoise/sql ./sql && docker cp p491f-sqlsrc:/opt/tortoise/etc/mangosd.conf.dist ./mangosd.conf
docker rm p491f-sqlsrc >/dev/null
M(){ docker exec -i $DB mariadb -uroot -p$ROOTPW "$@"; }
t0=$(date +%s)
M < sql/create_databases.sql && log "schemas ok" || { log "schemas FAILED"; exit 1; }
M <<SQL || { log "grants FAILED"; exit 1; }
CREATE USER IF NOT EXISTS 'mangos'@'%' IDENTIFIED BY '$APPPW';
GRANT ALL ON tw_world.* TO 'mangos'@'%'; GRANT ALL ON tw_char.* TO 'mangos'@'%';
GRANT ALL ON tw_logon.* TO 'mangos'@'%'; GRANT ALL ON tw_logs.* TO 'mangos'@'%'; FLUSH PRIVILEGES;
SQL
n=0; for f in $(ls sql/base/*.sql | sort); do M tw_world < "$f" || { log "world base FAILED at $f"; exit 1; }; n=$((n+1)); done
log "world base: $n files in $(( $(date +%s)-t0 ))s"
M tw_logon <<SQL || { log "realm FAILED"; exit 1; }
REPLACE INTO realmlist (id,name,address,port,icon,realmflags,timezone,allowedSecurityLevel,population,realmbuilds) VALUES (1,'P491F','127.0.0.1',8085,0,0,0,0,0,'7272');
INSERT INTO account (username, sha_pass_hash, rank) VALUES ('P491FEARLY', UPPER(SHA1('P491FEARLY:$EARLYPW')), 4);
SQL
log "accounts before start: P491FEARLY rank 4 only (control). P491FADMIN is inserted AFTER the world is up."
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
t1=$(date +%s)
docker run -d --name $MG --network $NET -p 127.0.0.1:17878:7878 \
  -v "$HOME/tortoise-server/data:/opt/tortoise/data:ro" \
  -v "$OUT/mangosd.conf:/opt/tortoise/etc/mangosd.conf:ro" \
  -w /opt/tortoise/bin $IMG ./mangosd -c /opt/tortoise/etc/mangosd.conf >/dev/null || { log "mangosd start FAILED"; exit 1; }
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
grep -n 'SOAP' world.log | tee -a press.log
[ "$state" = ready ] || { log "not ready; last lines:"; tail -15 world.log | tee -a press.log; exit 1; }
sleep 3
ENV='<?xml version="1.0" encoding="utf-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ns1="urn:MaNGOS"><SOAP-ENV:Body><ns1:executeCommand><command>server info</command></ns1:executeCommand></SOAP-ENV:Body></SOAP-ENV:Envelope>'
press(){ # name user:pass expected
  code=$(curl -s -o "curl-$1.xml" -w '%{http_code}' -u "$2" -H 'Content-Type: text/xml; charset=utf-8' -H 'SOAPAction: ""' --data "$ENV" http://127.0.0.1:17878/)
  verdict=PASS; [ "$code" = "$3" ] || verdict=FAIL
  log "$verdict curl $1 -> HTTP $code (expected $3) :: $(tr -d '\r\n' < "curl-$1.xml" | grep -oE '<(result|faultstring)>.{0,80}' | head -1)"
}
EXP_LATE=${EXP_LATE:-200}   # what a SQL-inserted admin should get on this build
press early-admin "P491FEARLY:$EARLYPW" 200
M tw_logon -e "INSERT INTO account (username, sha_pass_hash, rank) VALUES ('P491FADMIN', UPPER(SHA1('P491FADMIN:$ADMINPW')), 4); SELECT id, username, rank FROM account;" | tee -a press.log
log "P491FADMIN rank 4 inserted by SQL with the world running"
press late-admin "P491FADMIN:$ADMINPW" "$EXP_LATE"
press late-admin-wrongpw "P491FADMIN:nope" 401
M tw_logon -e "UPDATE account SET rank = 1 WHERE username = 'P491FADMIN'"
log "P491FADMIN demoted to rank 1 by SQL"
press late-demoted "P491FADMIN:$ADMINPW" "${EXP_DEMOTED:-403}"
M tw_logon -e "UPDATE account SET rank = 99 WHERE username = 'P491FADMIN'"
log "P491FADMIN rank set to 99 by SQL (outside the enum)"
press late-rank99 "P491FADMIN:$ADMINPW" "${EXP_RANK99:-403}"
M tw_logon -e "UPDATE account SET rank = 1 WHERE username = 'P491FEARLY'"
log "P491FEARLY (loaded at start as 4) demoted to rank 1 by SQL"
press early-demoted "P491FEARLY:$EARLYPW" "${EXP_EARLY_DEMOTED:-403}"
t2=$(date +%s)
docker stop -t 120 $MG >/dev/null; rc=$(docker inspect -f '{{.State.ExitCode}}' $MG)
log "graceful stop: $(( $(date +%s)-t2 ))s, exit code $rc"
log "press $LABEL done: $(grep -c '^\[.*PASS curl' press.log) PASS, $(grep -c '^\[.*FAIL curl' press.log) FAIL"
