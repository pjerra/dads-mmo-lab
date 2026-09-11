"""Apply this lane's three named changes to the copies of the 2026-09-08 scripts.

Every one of them is a defect that folder's own README records, fixed here rather than
re-measured; none of them touches a clause.
"""
import pathlib

p = pathlib.Path("/home/pk/lane710b/run-710-rerun.sh")
s = p.read_text()

# --- 1. the wait comes from the COMMITTED fix, not from a copy in this folder -------------
old = '"$PY" "$LANE/wait_ready.py" >> "$OUT/ready-after-restore.txt" 2>&1'
new = ('"$PY" "$LANE/checkout/pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/wait_ready.py" '
       '>> "$OUT/ready-after-restore.txt" 2>&1')
assert old in s, "wait anchor"
s = s.replace(old, new)

# --- 2. 2026-09-08 finding 3a -------------------------------------------------------------
old = '''  sudo cp -a "$OUT/ufw-user.rules.before" /etc/ufw/user.rules
  sudo cp -a "$OUT/ufw-user6.rules.before" /etc/ufw/user6.rules'''
new = '''  sudo cp -a "$OUT/ufw-user.rules.before" /etc/ufw/user.rules
  sudo cp -a "$OUT/ufw-user6.rules.before" /etc/ufw/user6.rules
  # 2026-09-08 finding 3a, fixed here rather than re-measured: `cp -a` preserves the COPY's
  # ownership, and the copy was chowned to pk so the driver could read it. Every sha256
  # check said "restored" and ufw itself was the only thing that noticed --
  # "WARN: uid is 0 but '/etc/ufw/user.rules' is owned by 1000". The mode is restated too,
  # because -a carried the copy's mode as well.
  sudo chown root:root /etc/ufw/user.rules /etc/ufw/user6.rules
  sudo chmod 640 /etc/ufw/user.rules /etc/ufw/user6.rules
  {
    echo "--- ownership after the restore (2026-09-08 finding 3a) ---"
    sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules
  } >> "$OUT/sweep4.log" 2>&1'''
assert old in s, "ufw anchor"
s = s.replace(old, new)

# --- 3. the realm row goes back AND the authserver is restarted so it re-reads it ---------
old = '''      echo "put back to: $(realm_address)"'''
new = '''      echo "put back to: $(realm_address)"
      echo "--- restarting ac-authserver so it re-reads the row it announces ---"
      docker restart ac-authserver
      sleep 10
      docker logs --tail 60 ac-authserver 2>&1 | grep -i "Added realm" \\
        || echo "(no 'Added realm' line in the last 60 -- look at the whole log)"'''
assert old in s, "realm anchor"
s = s.replace(old, new)
p.write_text(s)
print("run script patched")

c = pathlib.Path("/home/pk/lane710b/cleanup-710.sh")
t = c.read_text()
old = '''  docker exec ac-database mysql -uroot -ppassword -e \\
    "DELETE aa FROM acore_auth.account_access aa
       JOIN acore_auth.account a ON a.id = aa.id
      WHERE a.username='$ACCOUNT';
     DELETE FROM acore_auth.account WHERE username='$ACCOUNT';"'''
new = '''  # 2026-09-08 finding 3b, fixed here: the multi-table DELETE failed with
  # "ERROR 1046 (3D000): No database selected", the script's own next check printed the
  # account it was supposed to have removed, and it had to be deleted by hand. The database
  # is named on the command line now; the check below is why the defect was visible at all.
  docker exec ac-database mysql -uroot -ppassword acore_auth -e \\
    "DELETE FROM account_access WHERE id IN (SELECT id FROM account WHERE username='$ACCOUNT');
     DELETE FROM account WHERE username='$ACCOUNT';"'''
assert old in t, "cleanup anchor"
t = t.replace(old, new)
c.write_text(t)
print("cleanup patched")
