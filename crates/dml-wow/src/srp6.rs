//! AzerothCore's SRP6 registration data: the salt and verifier that make a
//! login work.
//!
//! ## Why this exists
//!
//! Creating the launcher's SOAP account was the one manual step left in an
//! otherwise one-click install: the user had to attach to the worldserver
//! console and type two lines. That was not a design preference, it was a
//! limitation — `docker attach` REFUSES piped stdin against a TTY container
//! ("stdin is not a terminal", measured 2026-08-01), and dropping the tty makes
//! it accept the pipe and never return.
//!
//! The user sanctioned the alternative on 2026-08-01: write the account row
//! directly. That makes this **the third sanctioned write into a DML-managed
//! MySQL database**, after `wow backup restore` and the LAN toggle's realmlist
//! `UPDATE`. It is narrow by construction — see [`AccountWrite`] for what it
//! refuses to do — and it removes two other problems with the manual route: the
//! password no longer passes through the worldserver console (and therefore no
//! longer lands in the log snapshots DML writes and users share), and there is
//! no Ctrl-C-stops-your-server footgun.
//!
//! ## The algorithm, and why every detail here is load-bearing
//!
//! ```text
//! x = SHA1( salt || SHA1( UPPER(user) || ":" || UPPER(pass) ) )
//! v = g^x mod N
//! ```
//!
//! Four things silently produce a verifier that is well-formed and simply never
//! authenticates, which is the worst possible failure mode — the account exists,
//! the login fails, and nothing says why:
//!
//! 1. **Both strings are UPPERCASED.** AzerothCore's `AccountMgr::CreateAccount`
//!    does this before hashing, so a lowercase password hashes to something the
//!    server will never compute.
//! 2. **`N` and `g` are the constants the client and server agree on.** They are
//!    protocol, not a choice.
//! 3. **Byte order is LITTLE-ENDIAN throughout** — the SHA1 digest read as an
//!    integer, `N` read as an integer, and the verifier written back out.
//!    TrinityCore/AzerothCore's `BigNumber::SetBinary` defaults to little-endian
//!    and `ToByteArray` matches it. Get this wrong and everything still "works"
//!    right up to the login attempt.
//! 4. **The verifier is padded to exactly 32 bytes.** `g^x mod N` can be
//!    shorter; the column is fixed width and a short value fails to match.
//!
//! Because all four fail the same silent way, [`verifier_for`] is proven against
//! a row AzerothCore itself wrote — see the live test in this module.

use sha1::{Digest, Sha1};

/// The SRP6 group modulus, little-endian, exactly as AzerothCore stores it.
/// Protocol constant: the client computes against the same number.
pub const N_LE: [u8; 32] = [
    0xB7, 0x9B, 0x3E, 0x2A, 0x87, 0x82, 0x3C, 0xAB, 0x8F, 0x5E, 0xBF, 0xBF, 0x8E, 0xB1, 0x01, 0x08,
    0x53, 0x50, 0x06, 0x29, 0x8B, 0x5B, 0xAD, 0xBD, 0x5B, 0x53, 0xE1, 0x89, 0x5E, 0x64, 0x4B, 0x89,
];

/// The SRP6 generator. Also protocol.
pub const G: u8 = 7;

/// Salt and verifier are both fixed 32-byte columns.
pub const SALT_LEN: usize = 32;
pub const VERIFIER_LEN: usize = 32;

/// `SHA1(UPPER(user) || ":" || UPPER(pass))` — the inner hash.
///
/// Uppercasing is ASCII-only on purpose: AzerothCore uses
/// `Utf8ToUpperOnlyLatin`, and account names are already restricted to
/// `[A-Za-z0-9_]` by the validator every caller runs first. A Unicode-aware
/// uppercase could disagree with the server on a non-ASCII byte we will never
/// be handed, and disagreeing is exactly the silent failure this module is
/// built to avoid.
fn identity_hash(user: &str, pass: &str) -> [u8; 20] {
    let mut h = Sha1::new();
    h.update(user.to_ascii_uppercase().as_bytes());
    h.update(b":");
    h.update(pass.to_ascii_uppercase().as_bytes());
    h.finalize().into()
}

/// `v = g^x mod N` where `x = SHA1(salt || identity_hash)`, little-endian
/// throughout, left-padded to 32 bytes.
pub fn verifier_for(user: &str, pass: &str, salt: &[u8; SALT_LEN]) -> [u8; VERIFIER_LEN] {
    use num_bigint::BigUint;

    let mut h = Sha1::new();
    h.update(salt);
    h.update(identity_hash(user, pass));
    let x_bytes: [u8; 20] = h.finalize().into();

    // Little-endian, matching BigNumber::SetBinary's default.
    let x = BigUint::from_bytes_le(&x_bytes);
    let n = BigUint::from_bytes_le(&N_LE);
    let v = BigUint::from(G).modpow(&x, &n);

    // Right-pad the little-endian bytes to the column width. `to_bytes_le`
    // drops leading zeros (which are TRAILING here), and a short verifier
    // simply never matches.
    let mut out = [0u8; VERIFIER_LEN];
    let le = v.to_bytes_le();
    let n_copy = le.len().min(VERIFIER_LEN);
    out[..n_copy].copy_from_slice(&le[..n_copy]);
    out
}

/// A fresh random salt.
///
/// `getrandom` rather than a seeded PRNG: this is a credential, and a salt that
/// is predictable across installs would let one leaked verifier be attacked for
/// every DML user at once.
pub fn random_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).expect("the OS random source is unavailable");
    salt
}

/// Everything needed to insert one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub username_upper: String,
    pub salt: [u8; SALT_LEN],
    pub verifier: [u8; VERIFIER_LEN],
}

/// Build the row data for a new account.
///
/// The username is stored UPPERCASED because that is what AzerothCore stores
/// and compares; storing it as typed makes a case-different login miss.
pub fn registration_for(user: &str, pass: &str) -> Registration {
    let salt = random_salt();
    Registration {
        username_upper: user.to_ascii_uppercase(),
        verifier: verifier_for(user, pass, &salt),
        salt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_protocol_constants_are_the_ones_azerothcore_uses() {
        // Not ours to choose: the client computes against the same number, so a
        // "tidier" constant here means nobody can ever log in.
        assert_eq!(N_LE.len(), 32);
        assert_eq!(N_LE[0], 0xB7, "little-endian: the LOW byte comes first");
        assert_eq!(N_LE[31], 0x89);
        assert_eq!(G, 7);
    }

    #[test]
    fn case_does_not_matter_because_both_inputs_are_uppercased() {
        // AzerothCore uppercases before hashing. If we did not, a user who typed
        // their password in lowercase would create an account that can never
        // authenticate -- and nothing would say why.
        let salt = [7u8; SALT_LEN];
        let a = verifier_for("dmlsoap", "hunter2", &salt);
        let b = verifier_for("DMLSOAP", "HUNTER2", &salt);
        let c = verifier_for("DmlSoap", "Hunter2", &salt);
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn the_verifier_is_always_exactly_the_column_width() {
        // g^x mod N is frequently shorter than 32 bytes, and to_bytes_le drops
        // the (trailing, little-endian) zeros. A short verifier is stored
        // happily and never matches.
        let mut short_count = 0;
        for i in 0..64u8 {
            let salt = [i; SALT_LEN];
            let v = verifier_for("dmlsoap", "hunter2", &salt);
            assert_eq!(v.len(), VERIFIER_LEN);
            if v[VERIFIER_LEN - 1] == 0 {
                short_count += 1;
            }
        }
        // Sanity: the padding branch is actually exercised by this sample, so
        // the assertion above is not vacuous.
        assert!(short_count > 0, "no short verifier in 64 samples -- padding untested");
    }

    #[test]
    fn a_different_salt_gives_a_different_verifier() {
        let a = verifier_for("dmlsoap", "hunter2", &[1u8; SALT_LEN]);
        let b = verifier_for("dmlsoap", "hunter2", &[2u8; SALT_LEN]);
        assert_ne!(a, b);
    }

    #[test]
    fn a_different_password_gives_a_different_verifier() {
        let salt = [9u8; SALT_LEN];
        assert_ne!(verifier_for("dmlsoap", "hunter2", &salt), verifier_for("dmlsoap", "hunter3", &salt));
    }

    #[test]
    fn salts_are_not_predictable_across_calls() {
        // A salt that repeated would let one cracked verifier attack every DML
        // install at once.
        let a = random_salt();
        let b = random_salt();
        assert_ne!(a, b);
        assert_ne!(a, [0u8; SALT_LEN], "an all-zero salt means the RNG did nothing");
    }

    #[test]
    fn the_stored_username_is_uppercased() {
        let r = registration_for("DmlSoap", "hunter2");
        assert_eq!(r.username_upper, "DMLSOAP");
    }

    /// THE ONLY TEST THAT CAN PROVE THIS IS RIGHT.
    ///
    /// Every internal check above passes just as happily on a verifier the
    /// server will reject: wrong endianness, a missed uppercase or an unpadded
    /// value all produce 32 self-consistent bytes. The only real oracle is a row
    /// AzerothCore itself wrote.
    ///
    /// Run it against a live server after creating an account by hand:
    ///
    /// ```text
    /// # in the worldserver console
    /// account create srp6probe hunter2
    ///
    /// # then
    /// DML_SRP6_USER=srp6probe DML_SRP6_PASS=hunter2 \
    ///   cargo test -p dml-wow --lib srp6::tests::live_ -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a live acore_auth with a hand-made account (see the doc comment)"]
    fn live_our_verifier_matches_the_one_azerothcore_wrote() {
        let user = std::env::var("DML_SRP6_USER").expect("set DML_SRP6_USER");
        let pass = std::env::var("DML_SRP6_PASS").expect("set DML_SRP6_PASS");
        let cfg = crate::db::DbConfig::from_env();
        // BOUND parameter, never a spliced literal -- the same rule every other
        // reader in this crate follows, and the reason none of them can be
        // reached by a quote in a name.
        let res = crate::db::query_with_params(
            &cfg,
            crate::db::Database::Auth,
            "SELECT LOWER(HEX(salt)), LOWER(HEX(verifier)) FROM account WHERE username = ?",
            vec![mysql::Value::from(user.to_ascii_uppercase())],
        )
        .expect("query");
        let row = res.rows.first().expect("no such account -- create it in the console first");
        let salt_hex = crate::db::cell_string(row.first()).expect("salt");
        let want_hex = crate::db::cell_string(row.get(1)).expect("verifier");

        let mut salt = [0u8; SALT_LEN];
        for (i, b) in salt.iter_mut().enumerate() {
            *b = u8::from_str_radix(&salt_hex[i * 2..i * 2 + 2], 16).unwrap();
        }
        let got = verifier_for(&user, &pass, &salt);
        let got_hex: String = got.iter().map(|b| format!("{b:02x}")).collect();
        eprintln!("azerothcore: {want_hex}");
        eprintln!("ours       : {got_hex}");
        assert_eq!(got_hex, want_hex, "our SRP6 does not match the server's");
    }
}
