#!/bin/sh
input=$(cat)
printf '{"ok":true,"data":{"echo":"%s"}}\n' "$input"
