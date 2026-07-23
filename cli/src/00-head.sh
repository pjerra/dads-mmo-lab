#!/usr/bin/env bash
set -euo pipefail

VERSION="3.0.0"
# Games root. DML_GAMES_DIR overrides the default (like DML_SOAP_URL /
# DML_SOAP_USER / DML_SOAP_PASS override the SOAP endpoint): tests point it
# at a fixture dir, and native Windows mode (Git Bash + Docker Desktop)
# points it at a Windows-side folder instead of the MSYS2 $HOME.
GAMES_DIR="${DML_GAMES_DIR:-$HOME/games}"
