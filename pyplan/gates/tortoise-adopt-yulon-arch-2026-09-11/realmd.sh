#!/bin/bash
S=1789080436
echo "# The realmd log for the login attempt"
echo "stamp: $(date -Is)"
echo
echo "Login was pressed on the client's login screen at 2026-09-10T22:47:21+00:00 (epoch 1789080441)."
echo "Window below: docker logs --since $S tortoise-realmd (five seconds before the press)."
echo "The SRP verifier, salt and session key this account's own row gained are REDACTED here:"
echo "they are this throwaway account's credentials material and no claim rests on their value."
echo
docker logs --since $S tortoise-realmd 2>&1 \
  | sed -E "s/'[0-9A-F]{40,}'/'<redacted>'/g" \
  | sed "s/^/  > /"
