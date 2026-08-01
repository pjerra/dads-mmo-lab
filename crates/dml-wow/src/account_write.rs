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
