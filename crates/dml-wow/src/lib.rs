//! WoW (AzerothCore + playerbots) game library for DML.
//!
//! Moved verbatim out of the launcher's `src/dml/` module tree
//! (cargo-workspace refactor, Task 7). The `pub use dml_core::…` lines below
//! are the re-export shims Tasks 2-6 left behind when the game-agnostic half
//! moved to `dml-core`; they keep every existing call site's module path
//! (`config`, `db`, `soap`, `status`, …) working unchanged.

pub mod account_write;
pub mod accountwide;
pub mod ahbot;
pub use dml_core::backend;
pub mod backup;
pub mod botid;
pub mod bridge;
pub mod cachestatus;
pub mod clientpath;
pub mod commands;
pub mod composegen;
pub mod config;
pub mod db;
pub mod destructive;
pub use dml_core::envelope;
pub mod install_native;
pub mod iteminfo;
pub mod lan;
pub mod lanip;
pub mod lifecycle;
pub mod logsnap;
pub mod maint;
pub mod modmgr;
pub mod moduletail;
pub mod modules;
pub mod native;
pub mod pages;
pub mod paperdoll;
pub mod party;
pub mod party_specs;
pub mod preflight;
pub mod registry;
pub mod restore;
pub use dml_core::runner;
pub mod soap;
pub mod srp6;
pub mod soap_bootstrap;
pub mod soap_cmds;
pub mod stats;
pub mod status;
pub mod tuning;

pub use dml_core::util::{dml_home_dir, home_dir};
