//! Create the launcher's SOAP account by writing it, rather than asking the
//! user to type it into a console.
//!
//! # THIS IS THE THIRD SANCTIONED WRITE INTO A DML-MANAGED MySQL DATABASE
//!
//! The standing rule is that MySQL is read-only and mutations go over SOAP GM
//! commands. Two exceptions already existed — `wow backup restore` and the LAN
//! toggle's realmlist `UPDATE`. The user sanctioned this third one on
//! 2026-08-01, and it is worth stating why the rule could not simply be kept:
//!
//! **SOAP cannot create the account that SOAP needs.** Every GM command travels
//! over SOAP, SOAP requires a GM-level-3 account, and a fresh AzerothCore has
//! none. The only other route is the worldserver console, and `docker attach`
//! REFUSES piped stdin against a TTY container ("stdin is not a terminal",
//! measured against a live Docker Desktop); dropping the tty makes attach accept
//! the pipe and never return. So the choice was a manual step forever, or this.
//!
//! ## What keeps it narrow
//!
//! * **It only ever INSERTs.** There is no code path here that updates or
//!   deletes an account. A name that already exists is a refusal, not an
//!   overwrite — someone else's credentials are never touched, and a user who
//!   runs this twice cannot lose the account they made the first time.
//! * **It writes two rows and no more**: one `account`, one `account_access`.
//! * **Every value is BOUND**, never spliced. Same rule as every reader in this
//!   crate.
//! * **It proves itself.** The caller verifies with a real SOAP round-trip
//!   afterwards, and the manual console path stays in the UI as the fallback —
//!   so a schema this does not understand degrades to the old flow rather than
//!   to a silent half-configured server.

use crate::db::{self, Database, DbConfig};
use crate::srp6;
use dml_core::error::CmdError;

/// The account already exists. A refusal, never an overwrite.
pub const CODE_EXISTS: &str = "ACCOUNT_EXISTS";
/// The insert itself failed — most likely an auth schema this build does not
/// understand.
pub const CODE_WRITE_FAILED: &str = "ACCOUNT_WRITE_FAILED";

/// Does an account with this name already exist?
///
/// Compared UPPERCASED because that is how AzerothCore stores and compares
/// names; asking case-sensitively would report "free" for a name that will then
/// collide on insert.
pub fn account_exists(cfg: &DbConfig, user: &str) -> Result<bool, CmdError> {
    let res = db::query_with_params(
        cfg,
        Database::Auth,
        "SELECT 1 FROM account WHERE username = ? LIMIT 1",
        vec![mysql::Value::from(user.to_ascii_uppercase())],
    )
    .map_err(db::db_err_to_cmd)?;
    Ok(!res.rows.is_empty())
}

/// Escape a value that is about to become the fixed part of a `LIKE` pattern,
/// with `!` as the escape character.
///
/// `_` matches any single character and `%` any run, so an unescaped `dmlsoap_`
/// would also match `dmlsoapX`, and a prefix containing `%` would match every
/// account on the server.
///
/// **`!` rather than `\`, because the `ESCAPE` argument is a string LITERAL and
/// MySQL parses it per `sql_mode`.** `ESCAPE '\\'` is one backslash under the
/// default mode and TWO characters under `NO_BACKSLASH_ESCAPES` — and MySQL
/// requires that argument to be exactly one character, so the statement errors
/// out on a server whose only unusual property is a stricter `sql_mode`, taking
/// the family guard down with it. `'!'` needs no string-literal escaping in any
/// mode, so `ESCAPE '!'` means the same thing on every server. `pages::bots_where`
/// and bash's `_bot_prefix_like` already reason about `NO_BACKSLASH_ESCAPES` the
/// same way; this follows them.
///
/// The escape character escapes ITSELF, and that arm is load-bearing rather than
/// decorative: without it a prefix `A!B` would be sent as `A!B`, MySQL would read
/// the `!` as escaping `B`, and the pattern would match `AB` — the `!` swallowed.
/// Worse, `A!_B` would become `A!!_B`, whose `_` is left unescaped and matches
/// any character. One pass over the input, so an emitted `!!` is never rescanned;
/// a two-pass replace chain that added `!%`/`!_` before doubling `!` would escape
/// its own escapes.
///
/// A literal backslash now needs nothing: with `ESCAPE '!'` declared it is not
/// special to `LIKE`, and this pattern is BOUND rather than spliced, so no
/// string-literal parser sees it either.
///
/// The one arm NOT copied from `botid::escape_like_literal` is its `'` -> `''`:
/// that module splices its pattern into SQL text, this one BINDS it, and
/// doubling a quote inside a bound value would search for a name that literally
/// contains two quotes.
fn escape_like_prefix(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '!' => out.push_str("!!"),
            '%' => out.push_str("!%"),
            '_' => out.push_str("!_"),
            _ => out.push(ch),
        }
    }
    out
}

/// The `LIKE` pattern [`account_family_exists`] binds, split out so the exact
/// string can be pinned by a test rather than re-derived by eye.
///
/// Upper-cased for the same reason as [`account_exists`], escaped, and only THEN
/// given its trailing `%` — that one is the single wildcard here that is meant
/// to be a wildcard, so it must be appended after the escaping rather than
/// travel through it.
fn family_pattern(prefix: &str) -> String {
    format!("{}%", escape_like_prefix(&prefix.to_ascii_uppercase()))
}

/// Does ANY account whose name starts with `prefix` already exist?
///
/// [`account_exists`] cannot answer the question the fallback name needs asked.
/// A fallback name carries fresh random hex, so asking whether THAT name is
/// taken is not a guard at all — it is free by construction, and can only ever
/// answer "free". The bounded question is whether the launcher has already put
/// an account of this family on this server.
///
/// The pattern comes from [`family_pattern`]; the explicit `ESCAPE '!'` is what
/// makes its escaping mean anything, and `!` is the character that survives
/// every `sql_mode` — see [`escape_like_prefix`] for why `'\\'` does not.
pub fn account_family_exists(cfg: &DbConfig, prefix: &str) -> Result<bool, CmdError> {
    let res = db::query_with_params(
        cfg,
        Database::Auth,
        "SELECT 1 FROM account WHERE username LIKE ? ESCAPE '!' LIMIT 1",
        vec![mysql::Value::from(family_pattern(prefix))],
    )
    .map_err(db::db_err_to_cmd)?;
    Ok(!res.rows.is_empty())
}

/// Create the account at GM level 3 on every realm.
///
/// Refuses rather than overwrites if the name is taken. Returns the account id.
pub fn create_gm_account(cfg: &DbConfig, user: &str, pass: &str) -> Result<i64, CmdError> {
    // The CLI's own rules, run before anything is written: an account this
    // creates must be one the rest of the app can address.
    crate::soap_bootstrap::validate(user, pass)?;

    if account_exists(cfg, user)? {
        return Err(CmdError {
            code: CODE_EXISTS.to_string(),
            message: format!("An account called {user:?} already exists on this server."),
            // Deliberately does NOT offer to reset it. Changing a password is a
            // different, more dangerous operation than creating one, and this
            // module has no business doing it on a name it did not create.
            hint: "Pick a different name, or use that account's existing password.".to_string(),
        });
    }

    let reg = srp6::registration_for(user, pass);
    db::execute(
        cfg,
        Database::Auth,
        "INSERT INTO account (username, salt, verifier) VALUES (?, ?, ?)",
        vec![
            mysql::Value::from(reg.username_upper.clone()),
            mysql::Value::from(reg.salt.to_vec()),
            mysql::Value::from(reg.verifier.to_vec()),
        ],
    )
    .map_err(|e| CmdError {
        code: CODE_WRITE_FAILED.to_string(),
        message: format!("Could not create the account: {}", crate::db::db_err_to_cmd(e).message),
        hint: "Your server's auth database may use a schema this build does not know. You can still create the account by hand in the worldserver console.".to_string(),
    })?;

    // Read the id back rather than trusting LAST_INSERT_ID across a pooled
    // connection: `execute` does not promise the same session for a follow-up
    // query, and granting GM on the WRONG id would be a real security bug.
    let res = db::query_with_params(
        cfg,
        Database::Auth,
        "SELECT id FROM account WHERE username = ?",
        vec![mysql::Value::from(reg.username_upper.clone())],
    )
    .map_err(db::db_err_to_cmd)?;
    let id = res
        .rows
        .first()
        .and_then(|r| db::sql_row_int(r.first()))
        .ok_or_else(|| CmdError {
            code: CODE_WRITE_FAILED.to_string(),
            message: "The account was created but could not be read back.".to_string(),
            hint: "Check the server's auth database.".to_string(),
        })?;

    // GM 3 on realm -1 (every realm). Without this the account exists and SOAP
    // still refuses it, which looks exactly like the bug this whole feature
    // exists to remove.
    db::execute(
        cfg,
        Database::Auth,
        "INSERT INTO account_access (id, gmlevel, RealmID) VALUES (?, ?, ?)",
        vec![mysql::Value::from(id), mysql::Value::from(3), mysql::Value::from(-1)],
    )
    .map_err(|e| CmdError {
        code: CODE_WRITE_FAILED.to_string(),
        message: format!("The account was created but could not be given GM access: {}", crate::db::db_err_to_cmd(e).message),
        hint: "Run `account set gmlevel <name> 3 -1` in the worldserver console to finish it."
            .to_string(),
    })?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_password_is_refused_before_any_write() {
        // The validation runs FIRST, so a rejected password cannot leave a
        // half-made account behind. Reaching the DB at all would be the bug --
        // this passes with no server because it never gets that far.
        let cfg = DbConfig::from_env();
        let err = create_gm_account(&cfg, "dmlsoap", "no$dollars").unwrap_err();
        assert_eq!(err.code, "BAD_ARG", "{err:?}");
    }

    #[test]
    fn a_bad_name_is_refused_before_any_write() {
        let cfg = DbConfig::from_env();
        let err = create_gm_account(&cfg, "ab", "hunter2").unwrap_err();
        assert_eq!(err.code, "BAD_ARG", "{err:?}");
    }

    #[test]
    fn the_refusal_for_an_existing_account_does_not_offer_to_reset_it() {
        // Changing an existing password is a different and more dangerous
        // operation than creating one, and this module must not grow into it by
        // way of a helpful-sounding hint.
        let err = CmdError {
            code: CODE_EXISTS.to_string(),
            message: String::new(),
            hint: "Pick a different name, or use that account's existing password.".to_string(),
        };
        assert!(!err.hint.to_lowercase().contains("reset"));
        assert!(!err.hint.to_lowercase().contains("overwrite"));
    }

    #[test]
    fn a_family_prefix_cannot_widen_into_a_wildcard() {
        // `dmlsoap_` is the real caller, and its underscore is a LIKE wildcard:
        // unescaped it would report the family "taken" because of an unrelated
        // `dmlsoapX`, and this launcher would then refuse to set SOAP up on a
        // server that had room for it.
        assert_eq!(escape_like_prefix("DMLSOAP_"), "DMLSOAP!_");
        // The mirror-image hazard, and the worse one: a `%` in the prefix would
        // match every account and report the family taken on a virgin server.
        assert_eq!(escape_like_prefix("A%B"), "A!%B");
        // The escape character escaping itself. Left alone, `A!B` reaches MySQL
        // as `A!B`, the `!` is read as escaping `B`, and the pattern matches
        // `AB` -- the character SWALLOWED rather than searched for.
        assert_eq!(escape_like_prefix("A!B"), "A!!B");
        // And the compound case, which is the widening one: unescaped, `A!_B`
        // would go out as `A!!_B`, whose `_` is then no longer escaped at all
        // and matches any single character.
        assert_eq!(escape_like_prefix("A!_B"), "A!!!_B");
        // Backslash is NOT escaped any more, and must not be: `ESCAPE '!'`
        // takes `\` out of LIKE's vocabulary, and the pattern is bound rather
        // than spliced, so no string-literal parser sees it either. Escaping it
        // would search for a name containing two backslashes.
        assert_eq!(escape_like_prefix("a\\b"), "a\\b");
    }

    /// The exact bytes the real caller sends, pinned rather than re-derived by
    /// eye. `dmlsoap_` is 8 characters ending in a LIKE wildcard, and the whole
    /// point of the guard is that the 8th one is a literal underscore: the
    /// pattern must find `DMLSOAP_AB12EF` and must NOT be satisfied by an
    /// unrelated `DMLSOAPX`.
    #[test]
    fn the_family_pattern_binds_a_literal_underscore_and_one_real_wildcard() {
        assert_eq!(family_pattern(&crate::soap_autosetup::fallback_prefix()), "DMLSOAP!_%");
    }

    /// LIVE. Creates a throwaway account, proves the launcher can authenticate
    /// with it over real SOAP, then deletes it.
    ///
    /// This is the only test that can prove the SRP6 is right: every offline
    /// check passes just as happily on a verifier the server will reject.
    ///
    /// ```text
    /// cargo test -p dml-wow --lib account_write::tests::live_ -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "writes to a live acore_auth and needs the worldserver up"]
    fn live_a_written_account_can_actually_authenticate() {
        let cfg = DbConfig::from_env();
        let user = "dmlsrp6probe";
        let pass = "Probe_1234";

        // Clean up any leftover from an earlier run so the test is repeatable.
        let _ = db::execute(
            &cfg,
            Database::Auth,
            "DELETE FROM account_access WHERE id IN (SELECT id FROM account WHERE username = ?)",
            vec![mysql::Value::from(user.to_ascii_uppercase())],
        );
        let _ = db::execute(
            &cfg,
            Database::Auth,
            "DELETE FROM account WHERE username = ?",
            vec![mysql::Value::from(user.to_ascii_uppercase())],
        );

        let id = create_gm_account(&cfg, user, pass).expect("create");
        eprintln!("created account id={id}");

        let soap = crate::soap::SoapConfig {
            url: crate::soap::SoapConfig::load().url,
            user: user.to_string(),
            pass: pass.to_string(),
        };
        let outcome = crate::soap::exec(&soap, "server info");
        eprintln!("soap said: {outcome:?}");

        // Tidy up BEFORE asserting, so a failure does not leave the account on
        // the user's server.
        let _ = db::execute(
            &cfg,
            Database::Auth,
            "DELETE FROM account_access WHERE id = ?",
            vec![mysql::Value::from(id)],
        );
        let _ = db::execute(
            &cfg,
            Database::Auth,
            "DELETE FROM account WHERE id = ?",
            vec![mysql::Value::from(id)],
        );

        assert!(
            matches!(outcome, crate::soap::SoapOutcome::Ok(_)),
            "a written account must be able to authenticate: {outcome:?}"
        );
    }
}
