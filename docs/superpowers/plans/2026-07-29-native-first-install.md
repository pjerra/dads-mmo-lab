# Native-first install — Route A, plus migration from dml-arch

**Written 2026-07-29** from an explicit user decision: *"I just want everything to run on
docker desktop or cli on windows, and not run the arch linux... route a, but we may add route b
later. also make a option to copy a working server from the old launcher from linux arch."*

- **Route A** (this plan): fresh Windows install — clone AzerothCore Playerbot branch +
  mod-playerbots, build the worldserver *inside Docker Desktop*. No Arch distro.
- **Migration**: import a server that already works in dml-arch.
- **Route B** (deferred, not designed out): publish a prebuilt playerbots worldserver image so
  users pull instead of building. Kept as a compose-variable swap, not a rewrite.

## Summary

Executable 14-task plan for the native-first path on branch rust-main: Route A (fresh Windows install — clone AzerothCore Playerbot branch + mod-playerbots, build the worldserver inside Docker Desktop, no Arch distro) first, the WSL→native migration second, Route B (published prebuilt image) explicitly deferred but protected by design seams. Core architecture, verified against the working server at C:/Users/perzi/dml-native/wow-server-playerbots: a three-file compose layout (generated docker-compose.yml with the ${IMAGE_TAG:-master} Route-B seam and ac-* container names; a config-system-owned docker-compose.override.yml carrying only runtime env + the ./modules mount; a docker-compose.build.yml used exclusively by the install/build stage so post-install 'up' never rebuilds and the config writer never touches build config); a resumable staged install engine in crates/dml-wow (state file + BuildKit cache = the close-the-launcher-mid-build story), surfaced as dml-wow install-native (NDJSON) and a launcher flow reusing the existing terminal streaming; an honest hardware preflight built from the project's own 2 GB-per-compiler-job evidence; a new Install-DML-Native.ps1 that prepares the machine (Docker Desktop, Git for Windows, yq, the launcher, Defender exclusions — no WSL, no C# tray); and the proven poc migration scripts fixed (their committed container-name/default-dir bugs contradict their own recorded lessons) then productized as dml-wow migrate-import with a refuse-on-nonempty-target DB guard. Two live human gates close it: a real fresh build on real hardware including a kill-mid-build resume test, and a real migration re-run with the only-one-server port guard proven. All ground-truth claims in the task list were re-verified in-repo this session, including the one new finding that override_env_write preserves sibling keys but drops comments — which is itself the argument for the three-file split.

## Relationship to the SHIP-LIST

This is SHIP-LIST 4.5's own named deferral ('Native title install then becomes v0.2.0') pulled forward by explicit user decision on 2026-07-29 — for the native path only; the no-new-features freeze otherwise stands. It does NOT touch the 'Deliberately NOT on this list' exclusion: the six guides/*/install-*.sh Linux installers stay bash and untouched; Route A is a new native-only Rust surface, not a port of them, and the bash↔Rust mirror doctrine is satisfied by recording install-native/migrate-import as native-only-by-design (bash on Windows deliberately refuses install; the only mirrored change is the refusal/catalog copy in Task 5). Smallest slice that delivers a Windows-only install: Tasks 1-6 + 8 (upstream pin, compose gen, engine, preflight, CLI, launcher wiring, machine installer) — migration (7, 9, 10), the account wizard (11), and the live gates (12, 13) trail it. Shared plumbing, not conflict: SHIP-LIST 4.1/4.2 (bundle.resources) is a prerequisite only for shipping the migration EXPORT script inside the exe, not for Route A (the engine is compiled in); 4.0c's honest-copy problem is solved natively by Task 4 emitting its own host-true copy; dropping the C# tray from the native installer resolves 4.0b for this path by construction. Recommendation to ratify in Task 14: v0.1.0 beta still ships as scoped in 4.5, with Route A landing behind the existing 'Enable untested features' toggle until Tasks 12-13 pass — 'do route a first, then we keep smoke testing' is compatible with both orderings, so the release-vs-Route-A ordering is an explicit user call, not something this plan decides silently.

## Research findings (verified in-repo, 2026-07-29)

### 1. What exactly does guides/wow-wotlk/install-wow-wotlk.sh do, split into (a) made-unnecessary-by-Docker-Desktop, (b) portable docker/compose/git, (c) genuinely Linux-host-bound?

**Confidence: verified**

The flow (v1.2.1) is: check_system -> sudo cache + keepalive -> summary/confirm -> install_server (install_docker, install_git, skip-if-images-already-built, clone AzerothCore Playerbot branch, clone mod-playerbots into modules/, write docker-compose.override.yml, check_docker_hub, `docker compose up -d --build` teed to ~/playerbots-build.log) -> install_dml_start_hook -> wait_for_server (poll `docker logs` for 'ready...', 30 min cap) -> create_accounts (manual `docker attach` instructions) -> setup_gaming_mode (Steam launcher script + MY_SERVER.txt + ~/games symlink) -> completion.

(a) DOCKER DESKTOP MAKES UNNECESSARY: the OSTYPE!=linux abort (lines 131-135); the whole pacman-keyring health/reset block (154-221); steamos-readonly/devmode toggles; `pacman -Sy docker docker-compose` (254); `usermod -aG docker` (261); `systemctl daemon-reload/enable/start docker` (264-272); /etc/sudoers.d/docker-nopasswd + `chmod 666 /var/run/docker.sock` (279-284); the sudo -v keepalive (897-903); install_git via pacman/apt (336-351) — native mode already requires Git for Windows (DmlRunner::native runs bash `dml` under Git Bash), so git exists.

(b) PORTABLE (this is the Route A core): disk-space floor check (15 GB, lines 137-142); internet + Docker Hub reachability probe with mirror advice (304-334); skip-recompile check `docker compose images | grep -qi worldserver` (426-435); `git clone https://github.com/mod-playerbots/azerothcore-wotlk.git --branch=Playerbot $SERVER_DIR` (456-459); `git clone --depth 1 https://github.com/mod-playerbots/mod-playerbots.git --branch=master $SERVER_DIR/modules/mod-playerbots` (469-472); writing docker-compose.override.yml with build targets + bot env + ./modules mount (479-504); `docker compose up -d --build 2>&1 | tee ~/playerbots-build.log` with PIPESTATUS check (512-518); the readiness poll on 'ready...' in worldserver logs (526-563). Account creation is portable in SPIRIT but should become `dml wow account create` over SOAP natively — note the installer's override does NOT enable SOAP (no AC_SOAP_* in lines 479-504); the working server's override does (dml-native docker-compose.override.yml has AC_SOAP_ENABLED/IP/PORT), so a native install needs an explicit SOAP-enable step or the launcher's SOAP features are dead post-install.

(c) GENUINELY LINUX-HOST-BOUND (drop or replace): the Steam Gaming-Mode launcher heredoc with konsole/pgrep 'Wow.exe|wine' (600-786); the `ln -s` games-visibility symlink (116-123); `sudo rm -rf` cleanups (441-447); MY_SERVER.txt's Steam/konsole instructions; the dml-start.sh hook (386-407) — WSL-only by verified design: crates/dml-wow/src/lifecycle.rs:314-317 records 'KEY FACT (verified live): the native title dir has no dml-start.sh ... pure docker compose orchestration, never bash ./dml-start.sh'. Consequence a Route A design must accept or solve: native start re-runs ac-db-import on every `up` after a `down` (that hook existed precisely to skip it).

Evidence:
- `C:\Users\perzi\dads-mmo-lab\guides\wow-wotlk\install-wow-wotlk.sh (whole file read; line refs above)`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow\src\lifecycle.rs:278-317 (native never runs dml-start.sh)`
- `C:\Users\perzi\dml-native\wow-server-playerbots\docker-compose.override.yml (AC_SOAP_ENABLED present there, absent from the installer's override)`

### 2. Where does the playerbots worldserver image come from — what is cloned, what is built, with which compose file/Dockerfile?

**Confidence: verified**

Cloned: (1) https://github.com/mod-playerbots/azerothcore-wotlk.git branch `Playerbot` into $SERVER_DIR (install-wow-wotlk.sh:456-459); (2) https://github.com/mod-playerbots/mod-playerbots.git branch master into $SERVER_DIR/modules/mod-playerbots (469-472). Built: the installer writes $SERVER_DIR/docker-compose.override.yml (479-504) adding `build: {context: ., target: worldserver|authserver|db-import|client-data}` to the four ac-* services plus the `./modules:/azerothcore/modules` mount and playerbots env (UPDATES_ENABLE_DATABASES=1, AUTOLOGIN=1, MIN/MAX_RANDOM_BOTS 1600/2000); then `docker compose up -d --build` (513). The BASE docker-compose.yml and the multi-stage Dockerfile live in the cloned azerothcore-wotlk checkout, not in this repo (so their exact contents are 'likely', everything else verified). The build TAGS the local playerbots build with the OFFICIAL image names: export-from-wsl.sh:72-74 saves exactly `acore/ac-wotlk-{worldserver,authserver,db-import,client-data}:master`, and the migrated dml-native compose runs those tags with NO build: key. Evidence it is not the official image: the migrated env/dist/etc/modules tree contains playerbots.conf(+.dist) — the official acore image has no playerbots module. That single worldserver image is the only artifact a fresh native install cannot pull, which is exactly why Route A must build and why Route B is 'publish this image'. Route B seam already exists: poc/native-docker/wow-playerbots/docker-compose.yml parameterizes `image: acore/ac-wotlk-*:${IMAGE_TAG:-master}` — swapping to a published registry image is a compose-variable change, not a rewrite, if Route A generates its compose from a committed template with the same seam.

Evidence:
- `C:\Users\perzi\dads-mmo-lab\guides\wow-wotlk\install-wow-wotlk.sh:450-521`
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\migrate\export-from-wsl.sh:71-74`
- `C:\Users\perzi\dml-native\wow-server-playerbots\docker-compose.yml (image: keys only, no build:)`
- `C:\Users\perzi\dml-native\wow-server-playerbots\env\dist\etc\modules (playerbots.conf present — listed via PowerShell)`
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\wow-playerbots\docker-compose.yml:28-80 (${IMAGE_TAG:-master} seam)`

### 3. What must the finished title directory contain for the EXISTING native mode to drive it?

**Confidence: verified**

The contract, from what the code actually reads: (1) LOCATION+NAME: <games_dir>/wow-server-playerbots — games_dir resolves env DML_GAMES_DIR -> ~/.dml/launcher.json -> %USERPROFILE%\dml-native (launcher/src-tauri/src/startup.rs:46-48,70-80); the folder name IS the title id (dml-core compose.rs:24-26; config.rs:180-186 TITLE). (2) COMPOSE FILE: docker-compose.yml (or the 3 alternates) in the title dir or one subdir (dml-core compose.rs:31-48 resolve_compose_dir); container_name must stay ac-database/ac-worldserver/etc — backup.rs builds `docker exec ac-database mysqldump -uroot ...` (backup.rs:641-648), world-restart does `docker restart -t 300 ac-worldserver`, db.rs hints name ac-database. (3) DB PUBLISHED ON LOCALHOST: native reads MySQL DIRECTLY at 127.0.0.1 as root (db.rs:19-43); port/password resolve env DOCKER_DB_EXTERNAL_PORT/DB_EXTERNAL_PORT -> the title's .env -> 3306/'password' (db.rs:215-244) — so the compose must publish 3306 and keep compose-default creds or record overrides in .env. (4) CONFIG TREE: docker-compose.override.yml at the title dir — the config system reads AND writes .services.ac-worldserver.environment there (config.rs:107-195, override_env_write:369; the RUST path needs NO yq — config.rs:669 — but the bash `dml wow config` fallback under Git Bash DOES need yq.exe, default <games_dir>\tools\yq.exe, lib.rs:5472-5479); env/dist/etc/{worldserver,authserver}.conf + env/dist/etc/modules/*.conf (conf_path_in config.rs:157-163); optional .env (cfg_file_path config.rs:560-567). (5) MODULE SOURCES: modules/<key>/.git dirs — the installed-check is literally `.git` is_dir (modules.rs:154-155) — AND mounted at /azerothcore/modules via the override, or the playerbots DB updater shuts the worldserver down at boot (poc README migration lesson, found live). (6) SOAP: AC_SOAP_ENABLED/IP/PORT in the override env + port 7878 published + creds at the WINDOWS home ~/.dml/soap.env (soap.rs:143-149, env DML_SOAP_URL/USER/PASS win). (7) NOT needed: dml-start.sh (lifecycle.rs:314-317) and yq for Rust-path config. The migrated C:/Users/perzi/dml-native/wow-server-playerbots matches every point (verified listing: compose+override, env/dist/etc tree incl. modules/*.conf + lua_scripts bridges, modules/ with the five mod-* clones, logs/). So Route A 'done' = produce exactly this shape; the acore checkout layout the WSL installer produces ALSO satisfies it (compose in dir, env/dist/etc in checkout, modules/) — meaning a fresh native build in <games_dir>/wow-server-playerbots would be driveable by native mode as-is once images are built and SOAP is enabled.

Evidence:
- `C:\Users\perzi\dads-mmo-lab\launcher\src-tauri\src\startup.rs:44-141`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-core\src\compose.rs:16-65`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow\src\config.rs:107-195,157-163,461-490,560-567,664-698`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow\src\db.rs:1-43,215-244,397`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow\src\backup.rs:641-648`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow\src\modules.rs:151-155,205-246`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow\src\soap.rs:49-61,143-149`
- `C:\Users\perzi\dml-native\wow-server-playerbots (directory listing + both compose files read)`

### 4. Which CLI surface does a native install path need that is NOT in dml-wow-cli today?

**Confidence: verified**

Verified against the full Cmd enum (crates/dml-wow-cli/src/cli.rs:21-358): dml-wow has NO `games list`, NO `games catalog`, NO generic `games status <id>` (only the fixed-title `status`), and NO `doctor`. It DOES have `install` — but that arm is a stdio passthrough that spawns Git Bash running bash `dml games install <id>` (run.rs:896-978), and bash's `games install` HARD-REFUSES on a Windows host: `_installers_supported()` is `! _host_bash_is_windows` (cli/src/80-titles.sh:26-28), producing 'installing titles needs the WSL backend: the DML installers are Linux scripts (sudo, pacman/apt, systemd)...' (80-titles.sh:31-33, enforced at 90-main.sh:1499-1502 before the script-exists check). So there is NO working native install path today — by recorded design, since all six guides installers are Linux system scripts (SHIP-LIST 'Deliberately NOT on this list'). Also relevant: even in native mode the LAUNCHER gets its Library data by shelling bash `dml` — games_list (lib.rs:521-528) and games_catalog (lib.rs:5869-5871) go through the runner, and games catalog carries install_supported:false on Windows so the UI can explain. Therefore Route A must ADD: (1) a native title-install orchestration (preflight checks, the two clones, override generation, `compose up -d --build` streamed, readiness wait, SOAP/account/bridge post-setup) on some surface — none exists in bash (blocked), Rust, or the launcher; (2) a decision about whether `games list`/`catalog` stay bash-backed or gain Rust ports (they are among the last bash dependencies of native mode); (3) an installer-script story consistent with the fact that install-wow-wotlk.sh's PORTABLE core (Q1b) is only ~90 lines of git+docker logic once the Linux system setup is gone.

Evidence:
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow-cli\src\cli.rs:21-358 (full Cmd enum)`
- `C:\Users\perzi\dads-mmo-lab\crates\dml-wow-cli\src\run.rs:896-978 (install passthrough requires Git Bash + DML_SCRIPT bash dml)`
- `C:\Users\perzi\dads-mmo-lab\cli\src\80-titles.sh:8-53`
- `C:\Users\perzi\dads-mmo-lab\cli\src\90-main.sh:1480-1516 (install arm), 1450-1478 (catalog arm)`
- `C:\Users\perzi\dads-mmo-lab\launcher\src-tauri\src\lib.rs:520-528,5869-5871,5874-5926 (games_list/catalog/install all shell bash dml)`

### 5. What is the honest FAILURE surface of building AzerothCore under Docker Desktop on a normal Windows machine?

**Confidence: likely**

In-repo evidence first. TIME: the installer's own claim is 2-4 hours on a Steam Deck (install-wow-wotlk.sh:17,363,370-373), and SHIP-LIST 4.0c records that the estimate is wrong off-Deck ('a 4-vCPU VM is slower') and that the copy itself gaslights Windows users; the user cancelled such a compile tonight. RAM — the project has ALREADY paid for this lesson: Install-DML.ps1 Step 6 (lines 726-776) encodes it — a worldserver compile runs one C++ compiler per core at ~2 GB each at peak; too many jobs on low RAM and the compiler is SIGKILLed mid-compile, which presents as 'dies at the same low % every retry'; their mitigation is cores = min(4, hostCores, wslRamGB/2GB) plus a swap floor of 8 GB. CRITICAL GAP: that mitigation protects the dml-arch VM only. Docker Desktop's own WSL2 VM is sized by the user's .wslconfig/Docker settings (default ~50% of host RAM, historically capped ~8 GB — general knowledge, likely), and NOTHING in a Route A flow today caps build parallelism (the acore Dockerfile builds with all VM cores — likely, Dockerfile not in repo). A 16 GB machine gets an ~8 GB VM: 4+ jobs x 2 GB is borderline OOM by the project's own numbers. DISK: the installer checks only 15 GB free; verified artifact sizes: client-data volume ~6 GB when downloaded (cli.rs:282 and 90-main.sh both say '~6 GB'), images ~3-5 GB (same sources), and a source build additionally holds the full acore checkout plus Docker build cache inside Docker Desktop's ext4.vhdx, which does not shrink automatically (the launcher's vhdx-shrink tool was explicitly dropped for native mode — poc README). Build-cache peak is recorded nowhere in-repo (unknown; 10-20 GB from general experience — likely). WINDOWS-SPECIFIC HAZARDS: (1) CRLF — a Windows-side checkout with autocrlf=true corrupts the shell scripts the build/runtime context uses; this repo's own .gitattributes exists because of exactly this class (verified as a project lesson; the acore-specific requirement is general knowledge — likely). (2) Defender — the project already ships Defender exclusions 'for source builders, scoped to directories' (commit 3297db9, tested by guides/DML-Windows/tests/Test-InstallerDefender.ps1, 128 checks): real-time scanning of a compile is a known drag here. (3) Docker Hub reachability/rate limits (check_docker_hub, lines 304-334, with mirror advice). WHAT THE USER SEES WHEN IT FAILS: in the WSL flow, raw BuildKit output teed to ~/playerbots-build.log then one line 'Compilation failed. Check ~/playerbots-build.log' — no percentage, no ETA, no OOM diagnosis; in the launcher, games_install streams raw chunks to a terminal (lib.rs:5874-5926) and cancel is `taskkill /F /T` (lib.rs:5945-5968), leaving BuildKit state ambiguous. An OOM kill looks like a random compiler 'Killed signal' line buried 2 hours into a log (likely). Design consequence: Route A needs a preflight that measures VM RAM/disk and either caps jobs or refuses honestly, staged progress (clone / build / db-import / ready), a resumable 'images already built' check (the WSL installer has one — portable), and truthful copy (SHIP-LIST 4.0c). This failure surface is the strongest argument for designing Route A so Route B (pull a published image) can replace the build step without touching anything else.

Evidence:
- `C:\Users\perzi\dads-mmo-lab\guides\wow-wotlk\install-wow-wotlk.sh:14-21,137-142,304-334,363-374,508-520`
- `C:\Users\perzi\dads-mmo-lab\guides\DML-Windows\Install-DML.ps1:726-776 (2 GB/core, SIGKILL symptom, core/swap mitigation)`
- `C:\Users\perzi\dads-mmo-lab\docs\SHIP-LIST.md:223-232 (4.0c), 282-284 (~6 GB client-data / 3-5 GB images also in cli.rs:282-287 and 90-main.sh:1572-1577)`
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\README.md:76-79 (vhdx tools dropped)`
- `C:\Users\perzi\dads-mmo-lab\launcher\src-tauri\src\lib.rs:5874-5968`
- `git log aec1cd6..: commit 3297db9 'feat(installer): Defender exclusions for source builders, scoped to directories'`

### 6. What would productizing the two poc migration scripts into the launcher require, and what in them is Windows-hostile or stale?

**Confidence: verified**

What exists and is PROVEN: export-from-wsl.sh (runs INSIDE dml-arch; consistent mysqldump of all acore_* DBs, client-data volume tar, env/dist/etc tree, env.ac, the override, modules WITH .git, docker save of the four exact images) + import-to-desktop.sh (Git Bash on Windows; docker.exe discovery, docker load, volume restore via a temp container, DB restore, verification counts, boot) — live-verified 2026-07-24 (2505 characters, user logged in and played; poc README Increment 4). Productizing requires: (1) A launcher flow that runs export via `wsl.exe -d dml-arch -u dml -- bash <script>` — the WSL runner seam exists, but the scripts must ship with the exe (exactly the SHIP-LIST 4.1 bundle.resources plumbing) — and streams progress: multi-GB docker-save/tar through /mnt/c is slow with zero progress today. (2) COMPOSE GENERATION the scripts deliberately skip: import REQUIRES a docker-compose.yml the export does NOT produce (import-to-desktop.sh:4-9 tells the human to copy the poc template and hand-add binds + AC_PLAYERBOTS_DATABASE_INFO); productization means a committed template rendered with the exported override's environment merged — the README's 'biggest lesson' is that skipping the override boots a silently-wrong 500-bot/1x/SOAP-off server. (3) FIXING VERIFIED STALE BUGS in the committed scripts — they contradict their own recorded lessons: (a) import polls/execs containers named dml-native-database/dml-native-worldserver (lines 57,64,67,74) and the poc template names all containers dml-native-* (poc/native-docker/wow-playerbots/docker-compose.yml:28-80), but the lesson in the SAME file's comments (lines 20-31), the README, and the WORKING dml-native compose require container_name ac-*; run as-committed against the correct compose, the import hangs at 'database never became healthy'. (b) Default-dir mismatch: export defaults OUT to .../dml-native/wow-playerbots (line 16, README header too) while the folder name MUST be wow-server-playerbots (it IS the title id) and import defaults to $HOME/dml-native/wow-server-playerbots (line 32) — the two scripts' defaults do not even point at each other. (c) import does not copy ~/.dml/soap.env to the Windows home (README says required, stripping CRs that wsl.exe piping adds) nor party presets/backups. (4) WINDOWS-HOSTILE/FRAGILE bits to absorb into Rust: bash-side docker.exe discovery duplicating dml-core::engine; cygpath + MSYS_NO_PATHCONV for the volume-restore bind; `gunzip | docker load` and tar-into-volume with no progress; sleep-loop health polls; export's Windows-username sniff via `cmd.exe /c echo %USERNAME%` from inside WSL (fragile in non-interactive contexts); GNU `stat -c %s` (works in Git Bash, not portable beyond it). (5) MISSING GUARDS the README itself lists as remaining: a 'which server is active' port-collision guard (distro and native servers share ports — only one can run) and explicit snapshot semantics in the UI (progress does NOT sync back to the distro). Roughly: the data plumbing is proven; the product work is compose generation, the ac-* rename, path/name unification, soap.env carry-over, progress streaming, and the active-server guard.

Evidence:
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\migrate\export-from-wsl.sh (whole file; lines 15-16,49-74)`
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\migrate\import-to-desktop.sh (whole file; lines 4-9,20-32,57-74)`
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\README.md:190-259 (Increment 4 + Remaining)`
- `C:\Users\perzi\dads-mmo-lab\poc\native-docker\wow-playerbots\docker-compose.yml:28-80 (stale dml-native-* container names)`
- `C:\Users\perzi\dml-native\wow-server-playerbots\docker-compose.yml:13-77 (the WORKING compose: container_name ac-*, name: dml-wow-native)`

### 7. (Context the plan must state) How does this work relate to docs/SHIP-LIST.md?

**Confidence: verified**

SHIP-LIST's one rule is 'no new features until Phase 4 is done', and it deliberately excludes rewriting the six Linux installers. Route A does not violate the installer-rewrite exclusion (the Linux scripts stay untouched; Route A is a NEW native path, not a port of them), but it IS a new feature pulled ahead of the release gate — specifically it is SHIP-LIST 4.5's own named deferral ('Native title install then becomes v0.2.0 rather than a release blocker') being promoted by explicit user decision on 2026-07-29. The plan should say exactly that: the user is overriding the freeze FOR THE NATIVE PATH ONLY, and Route A implements the item 4.5 already scoped as v0.2.0. Two SHIP-LIST items are shared plumbing rather than conflicts: 4.1/4.2 (bundle resources + provision from the bundle) are prerequisites for shipping any install/migration scripts inside the exe, and 4.0c (host-aware installer copy) is the same honesty problem Route A's build progress/estimates must solve. The 'install dml script as the original but for Docker Desktop' request maps to replacing Install-DML.ps1's Steps 5-10 (WSL2 features, .wslconfig, Arch import, pacman/systemd/docker-in-distro, embedded CLI bootstrap) with: install Docker Desktop + Git for Windows + yq.exe + the new launcher; and dropping Step 11's C# tray entirely resolves SHIP-LIST 4.0b (two indistinguishable launchers) instead of re-creating it.

Evidence:
- `C:\Users\perzi\dads-mmo-lab\docs\SHIP-LIST.md:7-9,153-268,324-331`
- `C:\Users\perzi\dads-mmo-lab\guides\DML-Windows\Install-DML.ps1:685-1101,1990-2010 (steps a native installer replaces/drops)`

## Tasks

### Task 1 — Pin the upstream build contract (design spec + committed SHA pins)


**Produces:** The design spec with VERIFIED upstream facts, not assumptions: shallow-clone mod-playerbots/azerothcore-wotlk branch Playerbot and mod-playerbots/mod-playerbots to scratch; record with quoted lines + commit SHAs: (a) what the base docker-compose.yml actually contains (build: keys? env/dist/etc binds? profiles?), (b) the Dockerfile stages (worldserver/authserver/db-import/client-data — the WSL override's targets prove they exist, confirm names), (c) whether the build honors any parallelism arg/env (grep Dockerfile + apps/ scripts for nproc/JOBS/MTHREADS — this decides whether Task 4 can cap jobs or only instruct Docker Desktop resource limits), (d) client-data download size/mechanism, (e) any upstream .gitattributes/eol pinning (CRLF exposure of a Windows-side checkout). Then the spec RESOLVES: title-dir layout (recommended: title dir IS the checkout, exactly like the proven WSL flow, so env/dist/etc lands where crates/dml-wow/src/config.rs:157-163 reads it) and the three-file compose split. Pin the two upstream SHAs as the default clone refs (overridable via flag) so installs are reproducible.

**Files:** `C:/Users/perzi/dads-mmo-lab/docs/superpowers/specs/2026-07-29-native-install-design.md`

**Tests:** Proof: the spec quotes actual upstream compose/Dockerfile lines with SHAs — a reviewer can re-fetch and diff. What breaks it: upstream force-moves the branch; the pinned SHA makes that a visible clone-time error instead of a silent behavior change. No production code; tree stays green by construction.

**USER GATE:** Sign off the layout decision (checkout-at-root vs subdir) and the pinned SHAs — pinning trades freshness for reproducibility and only the user can rank those for his community.

### Task 2 — Compose/override generation module in dml-wow (the shared Route A / migration / Route B seam)

*Depends on: 1*

**Produces:** render(title_dir, opts{image_tag, ports, soap, bot_min/max, rates}) writing three files modeled on the WORKING C:/Users/perzi/dml-native/wow-server-playerbots pair: docker-compose.yml (container_name ac-database/ac-worldserver/etc — backup.rs:641-648 and world-restart address containers by exactly these names; image acore/ac-wotlk-*:${IMAGE_TAG:-master} — the Route B seam; DB on 127.0.0.1:${DOCKER_DB_EXTERNAL_PORT:-3306} and SOAP pinned 127.0.0.1:7878 matching the CLI security posture), docker-compose.override.yml (ONLY runtime env — AC_SOAP_ENABLED/IP/PORT, playerbots min/max, rates, AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES — plus the ./modules:/azerothcore/modules mount; comment-light because override_env_write drops comments), docker-compose.build.yml (ONLY build: contexts/targets, passed explicitly via -f during install, never auto-loaded). Plus merge_exported_override(env_map) for the migration path. Embedded via include_str! per the existing data/ convention. dml-core stays untouched and game-agnostic.

**Files:** `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/src/composegen.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/data/native-compose.yml.tmpl`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/data/native-override.yml.tmpl`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/data/native-build.yml.tmpl`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/tests/native_compose_gen.rs`

**Tests:** Proof: golden renders in native_compose_gen.rs; a ROUND-TRIP test that runs the real crate::config::override_env_write against a freshly generated override and asserts the ./modules mount and all sibling keys survive (fails if the config writer ever clobbers the mount — the 'silently wrong server' class the migration hit live); a separation test asserting docker-compose.yml+override contain NO build: key and build.yml contains ONLY build config (fails on the exact 'simplification' that would design Route B out); a seam test asserting image_tag=X yields acore/ac-wotlk-*:X (fails if tags get hardcoded); an #[ignore] live test running docker compose config -q on the rendered trio (runs in the live parity-gate environment). Run cargo suites sequentially with bats per the standing gotcha.


### Task 3 — Native install engine: staged, resumable state machine with NDJSON events

*Depends on: 1, 2*

**Produces:** The Route A engine in dml-wow (WoW-specific, so NOT dml-core), stages: preflight → clone-core (git clone --config core.autocrlf=input, pinned SHA from Task 1) → clone-module → generate-compose (Task 2) → build (docker compose -f base -f override -f build.yml build, streamed via dml-core::proc, teed to <title>/logs/build-<UTC ts>.log) → up → readiness (poll the compose-PROJECT-scoped worldserver logs for the ready marker — never a bare container name, per the logsnap incident lesson — with BootLoopWatch armed, closing the recorded native readiness-wait gap in lifecycle.rs) → post (state file pruned, done event). State file .dml-install.json in the title dir; every stage checks completion evidence before redoing (checkout .git + expected remote/SHA; the portable 'docker compose images | worldserver' skip check from install-wow-wotlk.sh:426-435; ready marker). Resume = rerun continues from the first unmet stage; a killed build resumes cache-warm from BuildKit. Accepted + documented: native up re-runs ac-db-import (idempotent) — the dml-start.sh skip hook is deliberately WSL-only. Event vocabulary added to docs/cli-contract.md.

**Files:** `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/src/install_native.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/tests/native_install_engine.rs`, `C:/Users/perzi/dads-mmo-lab/docs/cli-contract.md`

**Tests:** Proof: engine driven with fake docker/git on PATH (per-platform #[cfg] helper per the portability rules) whose calls are recorded and read back IN ORDER — the same oracle pattern as LifecycleEnv. Asserts: (a) fresh-run call sequence; (b) resume from state=build re-invokes build but NOT clone (fails if resume re-clones or skips the build); (c) a failing build exits nonzero leaving state at build, and the next run resumes there (fails if the state file is written before the stage actually completes); (d) the never-ready readiness fake asserts elapsed >= deadline — the anti-vacuous rule, a failed spawn cannot satisfy it; (e) the tee log exists and contains the fake build output (fails if streaming drops the tee). No test pins a pure list production never reads — order is asserted against recorded calls only.


### Task 4 — Honest hardware preflight (the anti-3-hour-failure gate)

*Depends on: 1*

**Produces:** Preflight stage checks: docker engine reachable (tri-state — docker not answering is evidence of NOTHING except that install cannot proceed; reuse the Restart-Docker hint); Docker VM resources via docker info --format {{.MemTotal}}/{{.NCPU}}; free disk on BOTH the games-dir drive and docker info's DockerRootDir drive (the vhdx lives there and never shrinks); Docker Hub reachability (port of check_docker_hub); git present. Policy from the project's own paid-for lesson (Install-DML.ps1:726-776, ~2 GB per compiler job): refuse below floor with machine-readable INSTALL_UNDERSPEC + an --allow-underspec override; warn in the middle band; when NCPU > MemTotal/2GB, instruct capping Docker Desktop CPUs (or pass the parallelism knob if Task 1 found one). All copy host-true: no Steam Deck text, time estimate stated as hardware-dependent with the measured core/RAM numbers in the sentence (this is SHIP-LIST 4.0c solved natively rather than patched).

**Files:** `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/src/install_native.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/tests/native_preflight.rs`

**Tests:** Proof: pure policy functions tested as a table (mem/cpu/disk → verdict) — fails if a floor is changed silently; docker-info parse against captured real output; a test asserting the refuse path emits INSTALL_UNDESPEC-coded failure and that --allow-underspec flips it to a warn that still names the numbers (fails if someone downgrades refuse→warn without touching policy, or strips the numbers from the copy).

**USER GATE:** Ratify the floors and default posture. Proposed: REFUSE below 4 GB VM RAM or 40 GB free on the Docker data-root drive; WARN below 8 GB / 60 GB; always overridable. Only the user can rank 'protect people from a doomed 3-hour build' against 'let them try'.

### Task 5 — CLI surface: dml-wow install-native + the one mirrored copy change in bash

*Depends on: 3, 4*

**Produces:** New subcommand install-native (NDJSON streamed, resumable; distinct from the existing interactive Install passthrough at cli.rs:348 which stays untouched as the WSL route), wrapping the Task 3 engine; id validated with the existing valid_game_id rule. Doctrine recorded in cli-contract.md: install-native is NATIVE-ONLY BY DESIGN — no bash mirror, because bash's _installers_supported() (cli/src/80-titles.sh:26-28) deliberately refuses on Windows and the Linux installers remain the WSL/Linux path; this is consistent with Phase 6's direction. The ONE mirrored change so both surfaces tell the same story: _installers_unsupported_msg and the games catalog copy stop claiming install is impossible on Windows and instead point at the native install route (edit cli/src/*.sh, rebuild cli/dml via build.sh — never edit cli/dml directly).

**Files:** `C:/Users/perzi/dads-mmo-lab/crates/dml-wow-cli/src/cli.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow-cli/src/run.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow-cli/tests/cli_integration.rs`, `C:/Users/perzi/dads-mmo-lab/cli/src/80-titles.sh`, `C:/Users/perzi/dads-mmo-lab/cli/tests/`, `C:/Users/perzi/dads-mmo-lab/docs/cli-contract.md`

**Tests:** Proof: cli_integration spawns the built dml-wow.exe with fake docker/git on PATH and asserts the NDJSON vocabulary + exit codes match cli-contract.md (fails if the event names drift from the contract doc's table); a bats test pins the NEW refusal/catalog wording (fails if the copy is later changed on one surface only — the half-ship class); the existing Install passthrough tests stay green proving the WSL route is untouched. Run bats and cargo sequentially.


### Task 6 — Launcher wiring: native install flow with streamed terminal + resume affordance

*Depends on: 5*

**Produces:** A games_install_native Tauri command streaming the engine's events over the existing Channel/terminal plumbing, sharing the existing InstallSlot busy-guard; backend-based routing: native backend → engine, WSL backend → existing bash games_install (unchanged). Library shows Install for native mode (deciding by backend, not by bash catalog's install_supported flag), a Resume button when a title dir carries .dml-install.json, and cancel copy that tells the truth ('stops the build; progress is kept, Resume continues from the Docker build cache') since cancel remains taskkill /F /T. TermEvent union in api.ts gains the new events; unknown events stay ignored per the standing contract.

**Files:** `C:/Users/perzi/dads-mmo-lab/launcher/src-tauri/src/lib.rs`, `C:/Users/perzi/dads-mmo-lab/launcher/src/lib/api.ts`, `C:/Users/perzi/dads-mmo-lab/launcher/src/lib/pages/Library.svelte`, `C:/Users/perzi/dads-mmo-lab/launcher/src/lib/api.test.ts`

**Tests:** Proof: vitest on the event mapping — every new engine event renders and an UNKNOWN event is ignored not thrown (fails if a future event crashes the terminal — the reserved-pct rule); a Rust unit test on the routing seam asserting native backend dispatches the engine and WSL dispatches the bash passthrough (fails if routing regresses to bash-on-Windows refusal); svelte-check and the vitest suite stay at 0 regressions.


### Task 7 — Only-one-server guard: port-collision precheck on native start (mirrored)

*Depends on: 3*

**Produces:** The guard the poc README lists as missing: before native games start/up, probe 3724/8085/3306/7878 (from the title's .env where overridden); if bound by something that is not this compose project, refuse with a message naming the ports and the likely owner ('the dml-arch WSL server may be running — only one server can own these ports'). Mirrored into bash games start per doctrine (the same collision exists in the other direction), using the port-scan tool seam the bats harness already stubs. Advisory-only variant considered and rejected: a start that will lose the port race and crash-loop is exactly what the boot-loop watch would then diagnose — refusing earlier is the honest surface.

**Files:** `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/src/lifecycle.rs`, `C:/Users/perzi/dads-mmo-lab/cli/src/90-main.sh`, `C:/Users/perzi/dads-mmo-lab/cli/tests/`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/tests/`

**Tests:** Proof: Rust unit test with an injectable prober fake asserting refusal + that the message names every colliding port (fails if the guard is removed or the port list is dropped from the copy); bats test via the stubbed port tool asserting the bash mirror refuses identically (fails if one surface half-ships). Tri-state discipline: a prober that cannot answer is evidence of nothing and must NOT block start — tested explicitly.


### Task 8 — Install-DML-Native.ps1: the Windows machine installer 'as the original'

*Depends on: 1*

**Produces:** A NEW script (Install-DML.ps1 stays untouched — it remains the WSL/Arch route): detect-or-install Docker Desktop (default detect-and-instruct with the personal-use license sentence; -InstallDocker opt-in via winget), install Git for Windows if absent (native mode hard-requires it — even list/catalog shell bash today), download pinned yq.exe + SHA256 into <games_dir>/tools, install/point-at the DML Launcher (no C# tray, no WSL features, no Arch import — resolving SHIP-LIST 4.0b for this path by construction), write ~/.dml/launcher.json with backend=native + games_dir (env stays highest precedence per startup.rs), apply the existing directory-scoped Defender exclusions (the 3297d3b/3297db9 machinery) to games_dir BEFORE any build runs, -DryRun support, UTF-8 BOM if any non-ASCII. Deliberately does NOT write .wslconfig by default (sizing Docker Desktop's VM is instructed by Task 4's preflight copy; mutating every WSL user's machine is opt-in via -WriteWslConfig).

**Files:** `C:/Users/perzi/dads-mmo-lab/guides/DML-Windows/Install-DML-Native.ps1`, `C:/Users/perzi/dads-mmo-lab/guides/DML-Windows/tests/Test-InstallerNative.ps1`

**Tests:** Proof: Test-InstallerNative.ps1 in the existing no-Pester 128-check style: asserts the script contains NO wsl --install/pacman/systemd/C:/DML-tray references (fails if WSL-era steps leak back in), pins yq version+hash (fails on unpinned download), -DryRun performs zero side effects (fails if any step loses its dry-run guard), Defender exclusions are directory-scoped exactly like the tested pattern, and the launcher.json write preserves existing keys. Windows-smoke style run on this box as the live check.

**USER GATE:** Pick the Docker Desktop install mode (detect-and-instruct vs winget-automated) and confirm this script is the canonical native entry point named distinctly enough from Install-DML.ps1 that a stranger cannot confuse the two.

### Task 9 — Fix the committed migration scripts to match their own recorded lessons


**Produces:** The verified bugs fixed (all re-confirmed this session): import-to-desktop.sh polls/execs dml-native-database/dml-native-worldserver at lines 57/64/67/74 while its OWN comment block (lines 20-31), the README, and the working server require ac-* container names — run as committed it hangs at 'database never became healthy'; export default OUT is .../dml-native/wow-playerbots (line 16) while import defaults to $HOME/dml-native/wow-server-playerbots (line 32) and the folder name IS the title id — unify both on wow-server-playerbots; import gains the soap.env copy to the Windows home with CR strip (README calls it required; the script never does it). Scripts stay bash and stay LF (.gitattributes).

**Files:** `C:/Users/perzi/dads-mmo-lab/poc/native-docker/migrate/import-to-desktop.sh`, `C:/Users/perzi/dads-mmo-lab/poc/native-docker/migrate/export-from-wsl.sh`, `C:/Users/perzi/dads-mmo-lab/poc/native-docker/migrate/check-migrate-scripts.sh`, `C:/Users/perzi/dads-mmo-lab/poc/native-docker/README.md`

**Tests:** Proof: a committed check-migrate-scripts.sh asserting (a) zero 'dml-native-' container references remain in either script, (b) the two defaults resolve to the same folder name, (c) shellcheck-clean — cheap and it fails on exactly the regressions found; the REAL proof is the Task 13 live re-run. What breaks it: reintroducing the stale names or re-diverging the defaults.


### Task 10 — Productize migration import: dml-wow migrate-import + launcher migration flow

*Depends on: 2, 3, 7, 9*

**Produces:** The 'copy a working server from the old launcher' option. Import engine in Rust (Windows side, replacing import-to-desktop.sh's fragile bits natively — no cygpath/MSYS_NO_PATHCONV needed): staged + resumable like Task 3 (docker load streamed with per-image progress, client-data volume restore via temp container, compose TRIO generated by Task 2 with the EXPORTED override's env merged in — the 'biggest lesson': skipping it boots a silently wrong 500-bot SOAP-off server — DB restore, soap.env copy with CR strip, party-presets/backups copy, verification counts echoed). DOCTRINE: the DB restore is a MySQL write — sanctioned as the same class as wow backup restore and recorded in cli-contract.md + CLAUDE.md: it may ONLY run against a stack this import just created, and REFUSES a non-empty target (no --replace in v1; simpler and safer than restore semantics). Export side stays the bash script running inside the distro (a Linux script, consistent with the installer exclusion), invoked by the launcher via wsl.exe streamed into the terminal — from the repo path for now, from bundle.resources once SHIP-LIST 4.1 lands (dependency noted, not blocking the CLI path). Native-only surface, same no-bash-mirror rationale as Task 5.

**Files:** `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/src/migrate.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow/tests/native_migrate_import.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow-cli/src/cli.rs`, `C:/Users/perzi/dads-mmo-lab/crates/dml-wow-cli/src/run.rs`, `C:/Users/perzi/dads-mmo-lab/launcher/src-tauri/src/lib.rs`, `C:/Users/perzi/dads-mmo-lab/launcher/src/lib/pages/Library.svelte`, `C:/Users/perzi/dads-mmo-lab/docs/cli-contract.md`

**Tests:** Proof: fake-docker call-order suite for the full import sequence; THE guard test: the stub must DEMAND emptiness evidence before allowing the mysql restore call, and the engine must refuse a target reporting existing acore_characters rows (fails if the guard is dropped — the exact class of the Backups round's shipped Critical where a permissive stub hid dead creds); CR-strip byte test on soap.env (fails if the copy stops stripping wsl.exe's CRs); merge test asserting exported env lands in the generated override and NO build: key appears anywhere (fails if migration output stops being config-system-safe).


### Task 11 — Post-install account + SOAP bootstrap: guided console step with verified outcome

*Depends on: 3, 6*

**Produces:** After readiness, a guided step: an interactive worldserver-console session (docker attach against the compose-PROJECT-resolved worldserver — never a bare name, per the logsnap lesson) reusing the games_install stdin/stdout passthrough plumbing, with the exact 'account create' + 'account set gmlevel dmlsoap 3 -1' commands displayed to copy; then write ~/.dml/soap.env and VERIFY with a real SOAP round-trip (server-info) before the flow may declare the install done — a skipped step leaves every SOAP feature dead with no cause shown, so 'done' must be earned, not assumed. Explicitly NOT automated via MySQL: an SRP6 INSERT into acore_auth would create a new sanctioned-write class, recorded in the spec as a deferred option only the user can sanction.

**Files:** `C:/Users/perzi/dads-mmo-lab/launcher/src-tauri/src/lib.rs`, `C:/Users/perzi/dads-mmo-lab/launcher/src/lib/pages/Library.svelte`, `C:/Users/perzi/dads-mmo-lab/launcher/src/lib/`, `C:/Users/perzi/dads-mmo-lab/docs/superpowers/specs/2026-07-29-native-install-design.md`

**Tests:** Proof: Rust test asserting the attach target is resolved through the title's compose project (fails if it regresses to a bare container name that could answer for another engine's container); vitest on the step's state machine asserting the flow cannot reach 'done' while SOAP verify has not succeeded (fails if someone makes the verify optional); SOAP verify asserted against the existing SOAP test stub with a real XML round-trip, not a stubbed true.

**USER GATE:** The user types the console commands (interactive worldserver console cannot be unit-tested), and ratifies keeping account creation manual vs sanctioning an SRP6 MySQL write later.

### Task 12 — LIVE Route A gate: real fresh build, kill-mid-build resume, first login

*Depends on: 4, 6, 8, 11*

**Produces:** Evidence, not code: on this machine with a SCRATCH DML_GAMES_DIR (never C:/Users/perzi/dml-native — that is the disposable-but-working snapshot), run Install-DML-Native.ps1 in detect mode, then the full launcher install: preflight verdict honest for this box, clone, build (record wall-clock + peak Docker VM RAM for Task 4's floors and the estimate copy), CLOSE THE LAUNCHER mid-build then reopen and Resume — must continue cache-warm, not restart from zero; readiness; account step; login with the real WoW client. Then the SHIP-LIST 4.6-flavor repeat on the fresh VM (no Docker, no Git, no repo) to prove the PS1 carries a bare machine to the same result. New SMOKE-TESTS rows added and checked.

**Files:** `C:/Users/perzi/dads-mmo-lab/docs/SMOKE-TESTS.md`, `C:/Users/perzi/dads-mmo-lab/.superpowers/sdd/`

**Tests:** Proof: this task IS the test — the checklist rows with timestamps and the measured numbers folded back into Task 4's policy table (a floor contradicted by the measurement gets corrected, which is what makes the numbers falsifiable). A build that only ever ran on the dev box does not count; the VM leg is the release-gate standard.

**USER GATE:** Entire task is human: hours-long real build, Task-Manager kill, real client login, fresh-VM repeat. Cannot be unit-tested by construction.

### Task 13 — LIVE migration gate: real export/import from dml-arch + the port guard proven

*Depends on: 7, 9, 10*

**Produces:** Re-run the migration end-to-end with the FIXED scripts + the Rust import against the real distro server into a scratch games dir: identity check (2505 characters, the known guid/level spot-checks, 255 accounts), boot to ready with bots online, SOAP verified, THEN the negative test: with the distro server running, native start must refuse with the Task 7 port message (and vice versa). Confirm progress-does-not-sync-back is stated in the flow's copy (snapshot semantics).

**Files:** `C:/Users/perzi/dads-mmo-lab/docs/SMOKE-TESTS.md`, `C:/Users/perzi/dads-mmo-lab/poc/native-docker/README.md`

**Tests:** Proof: checklist rows with the verification counts pasted verbatim; the guard leg fails exactly when Task 7's precheck regresses. This live pass is the ONLY test that can catch a wrong-but-well-formed compose merge (a server that boots with default bots is green to every unit test and wrong to the user).

**USER GATE:** Entire task is human: it touches the real dml-arch server (export is read-only by design — verify the WAS_RUNNING branch behaves) and requires a real client login on the migrated stack.

### Task 14 — Docs + doctrine reconciliation: SHIP-LIST, CLAUDE.md, contract, Route B seam record

*Depends on: 5, 6, 10, 12, 13*

**Produces:** SHIP-LIST gains the explicit record: native install = item 4.5's v0.2.0 deferral pulled forward by user decision 2026-07-29, native path only, freeze otherwise intact, Route A behind 'Enable untested features' until Tasks 12-13 pass (or the user rules otherwise); CLAUDE.md updated per the standing self-maintenance rule (new modules, the native-only-no-bash-mirror doctrine note, migrate-import as the third sanctioned character-data write, the three-file compose invariant); cli-contract.md command table complete for install-native/migrate-import; the spec records Route B's open parameters verbatim so they are decided, not forgotten: registry + image name for the published playerbots worldserver, who builds/updates it, and the invariant that Route B must remain 'set IMAGE_TAG + skip the build stage' — plus the deferred items (Rust games list/catalog port, SRP6 account write, WSL-side export via bundle.resources).

**Files:** `C:/Users/perzi/dads-mmo-lab/docs/SHIP-LIST.md`, `C:/Users/perzi/dads-mmo-lab/CLAUDE.md`, `C:/Users/perzi/dads-mmo-lab/docs/cli-contract.md`, `C:/Users/perzi/dads-mmo-lab/docs/superpowers/specs/2026-07-29-native-install-design.md`, `C:/Users/perzi/dads-mmo-lab/cli/README.md`

**Tests:** Proof: the review convention — cli-contract's table is diffed against the clap surface in final review (a missing row is a finding); the CLAUDE.md update is checked against the actual tree. What breaks it: shipping Tasks 5/10 without their contract rows — reviewers catch doc drift only if this task is not skipped.


## Risks

- Upstream drift: the Playerbot branch of mod-playerbots/azerothcore-wotlk is a moving target, and its base docker-compose.yml + Dockerfile are NOT in this repo — every 'the build works like X' claim is unverified until Task 1 pins a commit SHA and records the actual compose/Dockerfile contents. If the base compose already carries build: keys or env/dist/etc binds that conflict with the generated files, Task 2's template design changes; that is why Task 1 blocks Task 2.
- Build failure surface on real hardware: the project's own numbers (Install-DML.ps1:726-776, ~2 GB per compiler job, SIGKILL presents as 'dies at the same % every retry') apply to Docker Desktop's VM, which nothing currently sizes. If the acore Dockerfile exposes no parallelism knob (Task 1 must answer), the only mitigation is instructing Docker Desktop resource settings — a preflight that refuses honestly is then load-bearing, and the refuse-vs-warn floors are guesses until the Task 12 live build measures a real machine.
- Config-writer clobber class: the generated docker-compose.override.yml becomes property of override_env_write, which drops comments and restyles YAML (config.rs:352). Any drift that lets build: or the ./modules mount live in the override risks a config save silently disabling the module mount — the exact 'boots a silently wrong server' failure the migration already hit. Task 2's round-trip test exists to make that class fail loudly; do not weaken it.
- MySQL-write doctrine expansion: migrate-import restores a DB dump via mysql, making it the third sanctioned character-data write (after restore and the LAN realmlist UPDATE). The refuse-on-nonempty-target guard is what keeps it from becoming a data-loss surface; the Backups round already shipped one Critical because a test stub was too permissive — Task 10's stub must DEMAND emptiness evidence, not default to it.
- Cancel/resume honesty: launcher cancel is taskkill /F /T; the resumability story is the state file + BuildKit cache, not process suspension. Docker's build cache and ext4.vhdx grow and never shrink (vhdx tools were deliberately dropped for native mode) — a user who retries a failing build three times pays ~3x disk. The preflight disk floor must account for the Docker data-root drive, not just the games dir.
- Two-installers confusion (SHIP-LIST 4.0b recurrence): shipping Install-DML-Native.ps1 alongside Install-DML.ps1 re-creates the 'which of these is the app' problem one layer up. Task 8 keeps names/copy visibly distinct and installs no C# tray, but the README must say plainly which installer a Windows-native user runs.
- Account/SOAP bootstrap stays manual (worldserver console via docker attach): automating it would need an SRP6 INSERT into acore_auth — a doctrine change only the user can sanction. Until then the first-run flow has a mandatory human step, and a user who skips it has a launcher whose SOAP features are all dead with no obvious cause; the guided step must verify SOAP round-trip before declaring the install done.
- Native mode still shells bash (Git Bash) for games list/catalog even after Route A — a machine without Git for Windows fails before install starts. The PS1 must install Git for Windows before first launcher run; porting list/catalog to Rust stays a recorded deferral, not part of this slice.
- Test-suite concurrency gotcha applies to all new suites: never run bats and cargo parity/engine tests simultaneously (bats setup() rewrites cli/dml in place).
- Route B seam erosion: Route B stays a compose-variable swap ONLY while (a) image tags stay ${IMAGE_TAG:-master}, (b) build: lives solely in docker-compose.build.yml, (c) the engine's build stage is skippable when images already exist. A later 'simplification' merging the build file into the override would silently design Route B out — Task 2's file-separation test is the tripwire.

## DECISIONS TAKEN (user ratified 2026-07-29, "do your recommendations and go")

1. **Orchestration lives in Rust.** The install engine is a `dml-wow` surface
   (`install-native`, NDJSON streamed, resumable); `Install-DML-Native.ps1` is a
   THIN machine-preparer only (Docker Desktop, Git for Windows, the launcher).
   One implementation, two entry points — and nothing new to delete when Phase 6
   removes the bash CLI.
2. **The checkout lives on the Windows filesystem.** Not negotiable in fact: the
   existing native config code REQUIRES `env/dist/etc` bind-mounted back out, so
   part of the tree must be Windows-side regardless. Clone with
   `core.autocrlf=input` (LF preserved for a Linux build), and reuse the
   Defender build-tool exclusion already shipped to keep the scan cost off it.
3. **Preflight REFUSES a doomed build.** Below ~6 GB Docker VM RAM or ~40 GB free
   on the Docker data-root drive it refuses; between there and 8 GB / 60 GB it
   warns. Always overridable with an explicit flag. Rationale is the project's
   own evidence (~2 GB per compiler job; a SIGKILL "dies at the same % every
   retry") plus a real cancelled build on 2026-07-29 — an unguarded build on a
   small machine costs hours and then fails.
4. **v0.1.0 still ships as scoped in SHIP-LIST 4.5.** Route A lands behind the
   existing "Enable untested features" toggle until its live gates pass, so the
   beta is not held hostage to a multi-hour build path. Feature key
   `native-install`, registered untested.

The remaining questions below stay open; none blocks Tasks 2, 4 or 9.

## Open questions — THE USER MUST ANSWER THESE

1. Where should the Route A install orchestration live: a new PowerShell installer 'as the original' (Install-DML-Native.ps1), a new Rust surface (dml-wow-cli + launcher command), or a relaxed bash `dml games install` arm? The user's words suggest a PS1, but the launcher needs a programmatic path too, and the choice collides with the bash-Rust mirror policy and Phase 6's plan to delete the bash CLI — only the user can rank those.

2. Build location trade-off: clone AzerothCore onto the Windows filesystem (visible to the user, but CRLF/Defender/build-context-upload risks) or inside a Docker named volume / helper container (safe and fast, but the checkout and env/dist/etc tree become invisible to the config system unless bind-mounted back out — which native mode's conf editing REQUIRES). The env/dist/etc bind is non-negotiable for the existing config code, so how much of the checkout lives on the Windows side is a real design decision.

3. Minimum-hardware policy: should the native installer REFUSE to start a source build below a floor (e.g. Docker VM RAM < 6-8 GB or free disk < ~40 GB), or warn and proceed? The project's own 2 GB/core + SIGKILL evidence says an unguarded build on a small machine fails after hours — the user must pick refuse vs warn, and the advertised time estimate.

4. Release sequencing: does Route A land behind the 'Enable untested features' toggle while the v0.1.0 beta ships as scoped in SHIP-LIST 4.5, or does the beta now wait for Route A? (The user said 'do route a first, then we keep smoke testing' — but the SHIP-LIST relationship needs an explicit call.)

5. Migration semantics: snapshot-copy (both servers exist; only one can run — needs the active-server guard) vs move-and-retire the dml-arch server; and should migration also be offered from an original-launcher C:\DML install, or only from a dml-arch DML install?

6. Route B parameters that shape Route A now: which registry/image name will the prebuilt playerbots worldserver be published under (affects the IMAGE_TAG/image-name seam Route A's generated compose should carry), and who builds/updates that image?

7. Does the new Windows installer install Docker Desktop itself (winget/direct download, license nuance: Docker Desktop is free for personal use but licensed for larger businesses) or just detect-and-instruct? And should it keep writing a .wslconfig to size Docker Desktop's VM for the build, which also affects every other WSL user on the machine?

