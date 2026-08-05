//! A LIBRARY VIEW OF THE ARG SURFACE, and nothing else.
//!
//! This target exists so `tests/vocab_surface.rs` can feed
//! [`dml_core::vocab`]'s translations into the REAL clap tree — the derive
//! itself as the oracle, rather than a list of subcommand names someone typed
//! into a test and has to keep true by hand.
//!
//! Only `cli` is re-exported. `out`'s two write helpers are deliberately
//! `pub(crate)` ("an internal helper `main.rs`'s ..." — see out.rs), and
//! `main.rs` is a separate crate from this lib, so routing the binary through
//! here would mean widening that visibility to satisfy a test. The binary
//! therefore keeps its own `mod cli; mod out; mod run;` and is untouched by
//! this file.
//!
//! The cost, stated rather than discovered: `cli.rs` compiles twice, and its
//! own unit tests run once per target. That is the whole price of the lib.

pub mod cli;
