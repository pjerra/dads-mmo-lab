//! LIVE SOAP-CREDENTIAL SYNC, opt-in and DELIBERATE. **This test rewrites
//! `~/.dml/soap.env` inside the user's `dml-arch` distro, twice, and puts the
//! original back byte for byte.**
//!
//! # What it proves, and why nothing else can
//!
//! [`dml_core::soap_env`] closes R6: `wow_soap_autosetup` runs in the LAUNCHER,
//! on Windows, and writes the Windows `~/.dml/soap.env`, while on
//! `Backend::Arch` and `Backend::Wsl` every SOAP-backed verb — GM Tools, My
//! Party, console send, announcements, motd — is answered by a CLI running
//! INSIDE the distro against `/home/dml/.dml/soap.env`. The launcher could
//! prove a round-trip and report success while all of them returned `SOAP_AUTH`.
//!
//! The unit tests behind `sync_with`'s seams prove the DECISIONS. What they
//! cannot reach is the boundary itself, and that is where every interesting way
//! to be wrong lives:
//!
//!   * does `wsl.exe --exec sh -c <script>` with the file on STDIN actually land
//!     the bytes, unmangled, with no CR, at mode 0600?
//!   * does a wrong credential in that file really make the in-distro CLI answer
//!     `SOAP_AUTH` — the literal `classify_probe` keys on?
//!   * and does putting the right one back really make the same verb work again,
//!     through BOTH in-distro CLIs?
//!
//! So this is a real before/after, produced on demand rather than waited for:
//! **working → publish a credential that cannot authenticate → both runners
//! return `SOAP_AUTH` → publish the original back → both runners work again.**
//!
//! # Running it
//!
//!   cargo test -p dml-core --test live_arch_soap_env -- --ignored --nocapture
//!
//! # Safety rules this test enforces on ITSELF
//!
//!   * It REFUSES to run unless the distro's own credentials are working to
//!     start with. Without that there is no known-good state to restore, and a
//!     test that can leave a user's SOAP worse than it found it is not worth
//!     running. (In the R6 state proper — distro refused — the fix is the
//!     launcher's job, not a test's.)
//!   * It REFUSES to run unless the world server is answering SOAP. Otherwise
//!     every probe is `Unknown` and every assertion below is vacuous.
//!   * The restore is a `Drop` guard, so a panic anywhere after the first write
//!     still puts the original back. A test that leaves a user unable to use GM
//!     Tools because an assertion failed is a worse bug than the one it found.
//!   * It creates no account, writes no database, runs no mutating SOAP verb,
//!     and starts/stops nothing.

use dml_core::envelope::Envelope;
use dml_core::runner::{DmlRunner, DISTRO, USER};
use dml_core::soap_env::{classify_probe, publish_to_distro, DistroSoap, PROBE_VERB};

/// A credential that cannot possibly authenticate: the account does not exist.
/// Chosen over a wrong password for a real account so nothing here can be
/// mistaken for an attempt on one.
const BAD_USER: &str = "dml_no_such_acct";
const BAD_PASS: &str = "definitelyNotIt";

/// Read the distro's `~/.dml/soap.env`. `None` when it does not exist.
///
/// Raw bytes as a string: the restore has to be byte-identical, so nothing here
/// trims, normalises or re-serialises.
fn read_distro_soap_env() -> Option<String> {
    let out = std::process::Command::new("wsl.exe")
        .args([
            "-d", DISTRO, "-u", USER, "--exec", "sh", "-c",
            "cat \"$HOME/.dml/soap.env\" 2>/dev/null",
        ])
        .output()
        .expect("wsl.exe must run");
    let text = dml_core::envelope::decode_wsl_output(&out.stdout);
    (out.status.success() && !text.is_empty()).then_some(text)
}

fn distro_soap_env_mode() -> String {
    let out = std::process::Command::new("wsl.exe")
        .args([
            "-d", DISTRO, "-u", USER, "--exec", "stat", "-c", "%a",
            "/home/dml/.dml/soap.env",
        ])
        .output()
        .expect("wsl.exe must run");
    dml_core::envelope::decode_wsl_output(&out.stdout).trim().to_string()
}

/// Puts the original file back when it goes out of scope, however it goes out
/// of scope.
struct Restore(String);

impl Drop for Restore {
    fn drop(&mut self) {
        match publish_to_distro(&self.0) {
            Ok(()) => eprintln!("    [restore] the original soap.env is back in the distro"),
            Err(e) => eprintln!(
                "    [restore] !!! COULD NOT RESTORE the distro's soap.env: {e}\n    \
                 Put it back by hand before using GM Tools or My Party."
            ),
        }
    }
}

/// Both in-distro CLIs' answer to the probe verb: `dml-wow` via the Arch
/// runner, and the bash `dml` via the default runner. They read the SAME file,
/// so they must always agree — and that agreement is what makes one probe a
/// statement about all 74 translated verbs.
fn both_runners() -> (Envelope, Envelope) {
    let argv: Vec<&str> = PROBE_VERB.to_vec();
    let arch = DmlRunner::arch()
        .run_json(&argv)
        .unwrap_or_else(|e| panic!("ARCH runner failed for {argv:?}: {e:?}"));
    let bash = DmlRunner::default()
        .run_json(&argv)
        .unwrap_or_else(|e| panic!("BASH runner failed for {argv:?}: {e:?}"));
    (arch, bash)
}

fn brief(e: &Envelope) -> String {
    match &e.error {
        Some(err) => format!("ok=false {}: {}", err.code, err.message),
        None => format!("ok=true {}", e.data),
    }
}

fn report(stage: &str, arch: &Envelope, bash: &Envelope) {
    eprintln!(
        "--- {stage}\n    arch (dml-wow) : {}\n    bash (dml)     : {}\n    verdict        : {:?}",
        brief(arch),
        brief(bash),
        classify_probe(arch)
    );
}

#[test]
#[ignore = "REWRITES the distro's soap.env (and restores it); run with --ignored"]
fn live_a_published_credential_makes_the_in_distro_cli_authenticate() {
    // -- preconditions ------------------------------------------------------

    let original = read_distro_soap_env()
        .expect("this test needs an existing /home/dml/.dml/soap.env to restore afterwards");

    let (arch0, bash0) = both_runners();
    report("BEFORE (the distro's own credentials)", &arch0, &bash0);
    assert_eq!(
        classify_probe(&arch0),
        DistroSoap::Working,
        "REFUSING TO RUN: the distro's own SOAP credentials are not working, so there is no \
         known-good state to restore. Start the server (and let the world finish booting); if it \
         is up and this still says otherwise, the machine is in the R6 state and the LAUNCHER is \
         what should repair it, not this test. Got: {}",
        brief(&arch0)
    );
    assert_eq!(
        classify_probe(&bash0),
        DistroSoap::Working,
        "the bash CLI disagrees with dml-wow about the same file: {}",
        brief(&bash0)
    );

    // From here on, every exit path restores.
    let _restore = Restore(original.clone());

    // -- the "before" of R6, produced on demand -----------------------------

    let bad = format!(
        "DML_SOAP_URL={}\nDML_SOAP_USER={BAD_USER}\nDML_SOAP_PASS={BAD_PASS}\n",
        url_of(&original)
    );
    publish_to_distro(&bad).expect("publishing into the distro must succeed");

    let (arch1, bash1) = both_runners();
    report("AFTER publishing a credential that cannot authenticate", &arch1, &bash1);
    // THIS is what every SOAP feature looks like in the R6 state.
    assert_eq!(
        classify_probe(&arch1),
        DistroSoap::Refused,
        "a credential for a non-existent account must make the in-distro CLI answer SOAP_AUTH — \
         that literal is the only thing that licenses `sync_with` to write: {}",
        brief(&arch1)
    );
    assert_eq!(
        arch1.error.as_ref().map(|e| e.code.as_str()),
        Some("SOAP_AUTH"),
        "{}",
        brief(&arch1)
    );
    assert_eq!(
        bash1.error.as_ref().map(|e| e.code.as_str()),
        Some("SOAP_AUTH"),
        "the bash CLI must see the same refusal — it reads the same file: {}",
        brief(&bash1)
    );

    // ...and the write itself must be private and unmangled, checked HERE
    // rather than after the restore, so it is the file this test wrote that is
    // being measured.
    assert_eq!(distro_soap_env_mode(), "600", "a GM3 password must not be world-readable");
    let landed = read_distro_soap_env().expect("the file we just wrote must be readable");
    assert_eq!(
        landed, bad,
        "the bytes were mangled in transit. A trailing \\r on the password authenticates as a \
         different string and the only symptom is SOAP_AUTH"
    );
    assert!(!landed.contains('\r'), "a CR reached the distro: {landed:?}");

    // -- the "after": publish the working credential back -------------------

    publish_to_distro(&original).expect("republishing the original must succeed");
    let (arch2, bash2) = both_runners();
    report("AFTER publishing the working credential", &arch2, &bash2);
    assert_eq!(
        classify_probe(&arch2),
        DistroSoap::Working,
        "publishing a working credential must make the in-distro CLI authenticate again — this \
         is the whole fix: {}",
        brief(&arch2)
    );
    assert_eq!(classify_probe(&bash2), DistroSoap::Working, "{}", brief(&bash2));

    assert_eq!(
        read_distro_soap_env().as_deref(),
        Some(original.as_str()),
        "the restore must be byte-identical"
    );
    assert_eq!(distro_soap_env_mode(), "600");
}

/// The `DML_SOAP_URL` line of a `soap.env`, or the built-in default. Kept so the
/// deliberately-bad credential still dials the SAME endpoint the good one does —
/// otherwise "refused" could mean "knocked on the wrong door", and the test
/// would prove nothing about credentials at all.
fn url_of(contents: &str) -> String {
    contents
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("DML_SOAP_URL="))
        .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
        .unwrap_or_else(|| "http://127.0.0.1:7878/".to_string())
}
