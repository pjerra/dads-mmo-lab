// The account-name and password rules AzerothCore + the DML CLI actually
// enforce, mirrored client-side.
//
// WHY THEY LIVE HERE AND NOT ONLY IN RUST. The SOAP bootstrap card shows the
// user two console commands to paste into their own server, and only validates
// afterwards -- so a password the launcher will later reject creates a REAL
// GM-level-3 account first. Retrying then re-emits `account create`, which
// AzerothCore refuses as "already exists", so the password is never updated and
// the verification failure blames the wrong command. Nothing in the app points
// at `account set password`, which is the only way out.
//
// Catching it before the commands are shown costs one regex and saves the user
// from a half-created admin account on their own server.
//
// The rules are ported from crates/dml-wow/src/soap_cmds.rs
// (`valid_account_user` / `valid_account_pass`), which are themselves the bash
// CLI's `_valid_account_user` / `_valid_account_pass` verbatim. An account this
// accepts must be one `dml wow account create` accepts too, or the app would be
// offering something the rest of it cannot address.

/** 3-20 characters, letters, digits and underscore. */
export const ACCOUNT_NAME_RE = /^[A-Za-z0-9_]{3,20}$/;

/**
 * 4-16 characters from AzerothCore's accepted set.
 *
 * The set is narrower than a typical password field, which is worth saying out
 * loud in the UI: `$` and `.` are common in generated passwords and both are
 * rejected here.
 */
export const ACCOUNT_PASS_RE = /^[A-Za-z0-9_@#%+=!-]{4,16}$/;

export function validAccountName(s: string): boolean {
  return ACCOUNT_NAME_RE.test(s);
}

export function validAccountPass(s: string): boolean {
  return ACCOUNT_PASS_RE.test(s);
}

/** Why this pair cannot be used, or `null` when it can. */
export function accountRuleError(user: string, pass: string): string | null {
  if (!validAccountName(user)) {
    return "Account name: 3-20 characters, letters, digits and underscore only.";
  }
  if (!validAccountPass(pass)) {
    // Names the rejected characters rather than restating the rule, because the
    // rule is already on screen and the surprise is always the punctuation.
    return "Password: 4-16 characters. Letters, digits and _ @ # % + = ! - only (no $ . , : / \\ or spaces).";
  }
  return null;
}
