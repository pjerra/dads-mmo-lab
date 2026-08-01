//! The launcher sets up its own SOAP access, without asking anyone to invent a
//! password.
//!
//! `soap_bootstrap` removed the worldserver console; `account_write` and `srp6`
//! made the account row writable directly. What was left was still three manual
//! acts at the end of a multi-hour install: find the card (it lived on ONE
//! page), invent a password, click a button. This module removes all three.
//!
//! Everything here is pure or seam-injected, so the whole decision tree is
//! testable with no server and no database. The impure bindings live in the
//! launcher's `wow_soap_autosetup` command.

use crate::soap_bootstrap::DEFAULT_SOAP_USER;

/// AzerothCore's ceiling, not a preference. `soap_cmds::valid_account_pass`
/// enforces `{4,16}` and `account_write::create_gm_account` runs it before it
/// writes anything, so a "stronger" 32-character password would be refused on
/// every fresh install -- at the one moment the user has nothing to retype.
pub const PASSWORD_LEN: usize = 16;

/// Exactly `valid_account_pass`'s charset: 26 + 26 + 10 + 8 = 70 symbols.
/// ~98 bits over 16 characters, which is far past anything that matters here.
pub const PASSWORD_ALPHABET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_@#%+=!-";

/// Generate a password from a caller-supplied byte source.
///
/// **Rejection sampling, not `byte % 70`.** 256 is not a multiple of 70
/// (`256 = 3x70 + 46`), so plain modulo hands the first 46 symbols a fourth
/// chance the remaining 24 never get. The bias is invisible in any output a
/// human would look at, which is exactly why it has to be handled here rather
/// than noticed later.
///
/// `fill` is a seam so the discard rule can be proven with a scripted byte
/// stream; production passes `getrandom`.
pub fn generate_password_from(mut fill: impl FnMut(&mut [u8])) -> String {
    let n = PASSWORD_ALPHABET.len();
    // The largest multiple of n that fits in a byte: 3 * 70 = 210. Anything at
    // or above it is thrown away rather than folded.
    let limit = (256 / n) * n;
    let mut out = String::with_capacity(PASSWORD_LEN);
    let mut buf = [0u8; 64];
    let mut i = buf.len();
    while out.len() < PASSWORD_LEN {
        if i == buf.len() {
            fill(&mut buf);
            i = 0;
        }
        let b = buf[i] as usize;
        i += 1;
        if b < limit {
            out.push(PASSWORD_ALPHABET[b % n] as char);
        }
    }
    out
}

/// A fresh random password.
///
/// `getrandom` for the same reason `srp6::random_salt` uses it: this is a
/// credential for a GM-level-3 account on a server whose auth port is
/// published, and a predictable one would be predictable on every DML install
/// at once.
pub fn generate_password() -> String {
    generate_password_from(|buf| {
        getrandom::fill(buf).expect("the OS random source is unavailable")
    })
}

/// Six lowercase hex digits, for the collision-fallback account name.
pub fn random_hex6() -> String {
    let mut b = [0u8; 3];
    getrandom::fill(&mut b).expect("the OS random source is unavailable");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The name to try when `dmlsoap` is taken: `dmlsoap_<6 hex>`.
///
/// 14 characters, inside `valid_account_user`'s 20. Chosen over resetting the
/// existing account's password because this code has no business touching a row
/// it did not write -- `create_gm_account` refuses to, and this is how that
/// refusal stays survivable instead of becoming a dead end.
pub fn fallback_user(hex6: &str) -> String {
    format!("{DEFAULT_SOAP_USER}_{hex6}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::soap_cmds::{valid_account_pass, valid_account_user};

    #[test]
    fn the_alphabet_is_exactly_what_the_validator_accepts() {
        // Byte-for-byte, because a symbol the validator refuses would produce a
        // BAD_ARG on a fresh install -- the one place a user cannot retype it.
        assert_eq!(PASSWORD_ALPHABET.len(), 70, "26 + 26 + 10 + 8");
        for b in PASSWORD_ALPHABET {
            let s = (*b as char).to_string().repeat(4);
            assert!(valid_account_pass(&s), "alphabet leaks {:?}", *b as char);
        }
        // Non-vacuity: a character the validator rejects must actually fail the
        // check above, or this test proves nothing.
        assert!(!valid_account_pass("$$$$"));
    }

    #[test]
    fn every_generated_password_is_one_the_server_will_take() {
        for _ in 0..1000 {
            let p = generate_password();
            assert_eq!(p.len(), PASSWORD_LEN);
            assert!(valid_account_pass(&p), "generated an unusable password: {p:?}");
        }
    }

    #[test]
    fn generated_passwords_are_not_repeated() {
        // A constant password across installs is the failure mode we rejected
        // when we chose not to ship a fixed default.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(seen.insert(generate_password()), "a password repeated in 1000 draws");
        }
    }

    #[test]
    fn bytes_at_or_above_the_rejection_limit_are_discarded() {
        // 256 = 3*70 + 46. Plain `byte % 70` would fold 210..=255 back onto the
        // first 46 symbols and give them a fourth chance the other 24 never get.
        // Feed exactly those rejects followed by 0..16 and assert NONE of the
        // rejects reached the output: the password must be the first 16 symbols
        // of the alphabet, in order.
        let feed: Vec<u8> = (210u8..=255).chain(0..16).collect();
        let mut k = 0usize;
        let pw = generate_password_from(|buf| {
            for slot in buf.iter_mut() {
                *slot = feed[k % feed.len()];
                k += 1;
            }
        });
        let want: String = PASSWORD_ALPHABET[..PASSWORD_LEN].iter().map(|b| *b as char).collect();
        assert_eq!(pw, want, "a rejected byte reached the password");
    }

    #[test]
    fn the_fallback_name_is_one_the_server_will_take() {
        let u = fallback_user("ab12ef");
        assert_eq!(u, "dmlsoap_ab12ef");
        assert!(u.len() <= 20, "valid_account_user caps at 20: {u}");
        assert!(valid_account_user(&u), "{u}");
    }

    #[test]
    fn the_hex_is_six_lowercase_hex_digits_and_varies() {
        let a = random_hex6();
        assert_eq!(a.len(), 6);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{a}");
        // 24 bits: a repeat in 50 draws would mean the RNG is not running.
        let mut seen = std::collections::HashSet::new();
        for _ in 0..50 {
            seen.insert(random_hex6());
        }
        assert!(seen.len() > 40, "hex barely varies: {} distinct in 50", seen.len());
    }
}
