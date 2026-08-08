# dmlpack — reference copy of Baerthe's tool

Third-party reference material for roadmap **Round 5.7** (build-once,
install-anywhere game packs). Not built by this repo and not wired into
anything — it is here so the v0.2 design can read the real implementation
instead of a summary, and so the only copy does not live in a Downloads
folder.

**Provenance.** Fetched 2026-08-08 from `wow.baerthe.com` and from the
`TurtleV2.dmlpack` distribution. `dmlpack.py` is not published standalone: it
rides inside `install-dmlpack.sh` as a base64 payload after the
`__DMLPACK_PAYLOAD__` marker, and was decoded from there.

**Permission.** Baerthe granted unrestricted use ("we are free to use
dmlpack.py as we wish", relayed by the project owner 2026-08-08, Discord).
The files carry no licence header of their own. If any of this code is
absorbed into DML's own crates it ships under this repo's AGPL-3.0 like
everything else; keep this note as the record of why that is allowed.

| file | what it is |
|---|---|
| `dmlpack.py` | the tool itself — 2100 lines, stdlib-only Python: `pack` / `verify` / `restore` / `shortcuts` / `repair` / `list` / `reclaim` / `deck2-ok`, including a binary-VDF writer and the Steam appid hash |
| `install-dmlpack.sh` | the self-contained menu installer that embeds it |
| `tools.Dockerfile` | the `dml-pack-tools:local` image — zstd/qemu-img live in a container so the host needs no packages (a trick worth stealing) |
| `TurtleV2-manifest.json` | a real production manifest, extracted from the 13 GB Turtle WoW V2 pack — the best format documentation that exists |

Format essentials, verified against the source: a `.dmlpack` is an
**uncompressed tar whose first member is `manifest.json`** (so the manifest
reads in milliseconds without touching the payload); member kinds `tar_zst`,
`files_tar_zst`, `docker_volume`, `docker_image`, `root_tar_zst`, `qcow2`,
each sha256-pinned with packed and restored byte counts.
