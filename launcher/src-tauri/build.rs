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

fn main() {
    for dir in WATCH_DIRS {
        println!("cargo:rerun-if-changed={dir}");
    }
    tauri_build::build()
}
