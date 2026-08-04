// Directory resources cargo would otherwise never notice a NEW file in.
//
// `tauri_build::build()` copies `bundle.resources` into the cargo target dir
// and prints one `cargo:rerun-if-changed` per resource it WALKED — i.e. per
// file (tauri-build 2.6.3 `copy_resources`; tauri-utils' `ResourcePathsIter`
// skips directories). A bridge script that does not exist yet is on no such
// list, so adding `cli/lua/party/dml_newthing.lua` left cargo believing the
// build script was still fresh: build.rs never re-ran, `target/<profile>` kept
// the old script set, and `provision::plan` — which enumerates the bridge
// scripts from disk at runtime exactly so a new one needs no code change —
// provisioned the stale set with no error anywhere. Verified in the real build
// output before this fix: `..\..\cli\lua\gm\dml_gm.lua` was listed, `..\..\cli\lua\gm`
// was not.
//
// Cargo rescans a DIRECTORY named on a rerun-if-changed line, so naming the two
// bridge directories closes the hole for ADDED files. Paths are relative to this
// package root, the same base `bundle.resources` uses. `payload.rs`'s
// `the_build_script_makes_cargo_watch_every_directory_resource` reads cargo's
// captured stdout and fails if a directory-form resource is missing from this
// list.
//
// KNOWN, DEV-LOOP-ONLY GAP — DELETIONS ARE NOT PRUNED. The watch above only
// gets build.rs re-run; `tauri_build::build()` then COPIES the current sources
// and never removes anything, so deleting `cli/lua/party/dml_oldthing.lua`
// leaves the copy under `target/<profile>/cli/lua/party` exactly where it was.
// That matters because `provision::plan` enumerates the bridge scripts from the
// resource dir at RUNTIME, so a `tauri dev` / `cargo run` build keeps
// provisioning a script that no longer exists in the repo. It does NOT reach a
// shipped installer (the bundler copies from the configured sources into a
// fresh staging dir), and it is not fixed here on purpose: a build script that
// deletes files under `target/` races every parallel cargo invocation for a
// developer-loop wart. The remedy is one line — delete `target/<profile>/cli`
// (or `cargo clean -p launcher`) — and `payload.rs`'s
// `the_bundled_resource_dirs_match_the_repo` fails with exactly that message
// rather than letting the stale script ship into a distro in silence.
const WATCH_DIRS: [&str; 2] = ["../../cli/lua/party", "../../cli/lua/gm"];

/// Resource-relative path of the Linux `dml-wow` binary `bundle.resources`
/// carries. Duplicated from `payload::DML_WOW_BIN` (build.rs cannot depend on
/// the crate it is building) and pinned equal to it by
/// `payload::tests::tauri_conf_bundles_every_payload_target`, which reads the
/// same string out of `tauri.conf.json`.
const DML_WOW_STUB: &str = "backend/dml-wow";

fn main() {
    for dir in WATCH_DIRS {
        println!("cargo:rerun-if-changed={dir}");
    }
    // `tauri_build::build()` (below) hard-fails if a `bundle.resources`
    // source is missing from disk, and the real Linux `dml-wow` ELF (CI's
    // ubuntu job, ../../.github/workflows/rust.yml) can never be produced on
    // this Windows checkout. Ensure SOME file exists at that path before the
    // resource walk runs, so an ordinary `cargo build`/`cargo test` stays
    // green with no manual staging step.
    //
    // Deliberately NOT a tracked file: `payload::is_elf` reads this stub as
    // MISSING (right -- it is not a real binary), but a git-tracked
    // placeholder would let `git checkout .` / `git clean -fd` / a fresh
    // clone silently revert a correctly-staged real binary back to the stub
    // with no error anywhere, the same silent-failure shape one layer
    // earlier. Instead this only ever writes when the path is ABSENT, so a
    // release process that has staged the real ELF here is never touched,
    // and `.gitignore` keeps the path out of the index entirely.
    ensure_dml_wow_stub();
    tauri_build::build()
}

fn ensure_dml_wow_stub() {
    let path = std::path::Path::new(DML_WOW_STUB);
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create backend/ for the dml-wow stub");
    }
    // A breadcrumb, not a silent `exit 1`: `payload::is_elf`/`resolve()` are
    // meant to catch this before it is ever reached, but if something did
    // exec it, a bare `exit 1` gives zero signal that this was the inert
    // placeholder rather than a crashing real binary.
    std::fs::write(
        path,
        "#!/bin/sh\necho \"dml-wow: placeholder, no real Arch backend was bundled\" >&2\nexit 1\n",
    )
    .expect("write the dml-wow placeholder stub");
}
