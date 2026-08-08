# dml-pack-tools:local — the tools the host doesn't have.
#
# qemu-img is not installed on SteamOS; it only exists inside the per-game
# qemu images (dekaron-qemu:local et al) and guestfs:local. Rather than reach
# into a game's own image (which reclaim may later remove), dml-pack builds
# this one generic helper.
#
#   docker build -f tools.Dockerfile -t dml-pack-tools:local .
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends qemu-utils zstd tar \
 && rm -rf /var/lib/apt/lists/*
