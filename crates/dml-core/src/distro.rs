//! Creating and preparing the `dml-arch` distro.
//!
//! Pure argv builders and one ordered step list, deliberately with no spawning
//! of its own: the ORDER is the part worth testing, and it is worth testing
//! without owning a machine that happens to be in each state. The execution
//! seam lives in the launcher's `provision.rs`.
//!
//! Flags verified against WSL 2.7.10 on 2026-08-04: `--install <distro>
//! --name --no-launch --location --vhd-size --web-download`, and
//! `--manage <distro> --set-default-user`.

/// The official catalog name (`wsl --list --online`). Not a third-party rootfs:
/// the spec's decision 4 is a catalog install, so there is no artifact to host,
/// verify or keep patched.
pub const CATALOG_NAME: &str = "archlinux";

/// What the backend needs, pinned known-good on 2026-08-04:
/// docker `1:29.6.1-1`, docker-compose `5.3.1-1`, docker-buildx `0.35.0-1`.
///
/// `docker-buildx` is REQUIRED. `install_native.rs`'s `pct` progress parser
/// reads BuildKit vertex headers out of the streamed build output, and install
/// resume rests on BuildKit's cache. Without it the build falls back to the
/// legacy builder, the progress bar goes silent and resume degrades — a
/// failure that presents as a hang rather than as a missing package.
///
/// `sudo` is REQUIRED too, and not for granting anyone new privilege — the
/// `sudoers` first-boot step writes a NOPASSWD drop-in FOR it. `sudo` is not
/// part of Arch's `base` group (verified live against a real `dml-arch`:
/// `pacman -Qi sudo` reports `Required By: base-devel`, not `base`), so a
/// fresh `wsl --install archlinux` image plausibly does not have it, and the
/// drop-in at `/etc/sudoers.d/99-dml` would be inert — the binary that reads
/// it is not there. The failure would surface later, downstream, as a bare
/// "sudo: command not found", and nothing in this module's tests can catch
/// that, because none of them execute anything. `--needed` on the
/// `pacman-sync` step makes this a no-op on the (checked-for) case where the
/// image already ships it.
pub const PACKAGES: [&str; 5] = ["docker", "docker-compose", "docker-buildx", "git", "sudo"];

/// Linux user-name charset this module accepts: lowercase alphanumeric,
/// underscore and hyphen, never starting with a hyphen (a name beginning `-`
/// reads as a flag to some programs). This is deliberately narrower than
/// `useradd`'s own grammar — it is the set that is also safe to splice,
/// UNESCAPED, inside a `sh -c` string. The `wsl-conf` and `sudoers`
/// first-boot steps do exactly that (the `--exec` crossing means `wsl.exe`
/// itself does no shell parsing, but `sh -c` is invoked explicitly as the
/// program, so that string genuinely is shell script): a name containing a
/// single quote would close the quote early and run whatever follows it as
/// root. `useradd`/`usermod` pass `user` as a discrete argv element to a real
/// program and are already safe on their own, but the guard belongs on the
/// input, not on one use of it.
pub fn valid_distro_user(user: &str) -> bool {
    !user.is_empty()
        && !user.starts_with('-')
        && user.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
}

/// `wsl --install archlinux --name <name> --no-launch`.
///
/// `--no-launch` is load-bearing: without it `wsl --install` starts the
/// distro's interactive first-run account setup, which waits on a console
/// nobody is attached to. The launcher would hang with nothing on screen.
pub fn install_distro_argv(name: &str) -> Vec<String> {
    vec![
        "--install".to_string(),
        CATALOG_NAME.to_string(),
        "--name".to_string(),
        name.to_string(),
        "--no-launch".to_string(),
    ]
}

/// `/etc/wsl.conf`. LF only — bash inside WSL chokes on CRLF, which is why
/// `.gitattributes` forces LF on every shell file in this repo.
pub const WSL_CONF: &str = "[boot]\nsystemd=true\n";

/// `wsl --terminate <name>` — required after writing `wsl.conf`, because
/// systemd only comes up on the next boot of the distro.
pub fn terminate_argv(name: &str) -> Vec<String> {
    vec!["--terminate".to_string(), name.to_string()]
}

/// `wsl --manage <name> --set-default-user <user>`. Preferred over editing the
/// `[user]` section of `wsl.conf` by hand: it is the documented API, and it
/// cannot corrupt a file the rest of this module also writes.
pub fn set_default_user_argv(name: &str, user: &str) -> Vec<String> {
    vec![
        "--manage".to_string(),
        name.to_string(),
        "--set-default-user".to_string(),
        user.to_string(),
    ]
}

/// What a first-boot step actually does.
///
/// AN ENUM, NOT A `restart_after: bool`, and the difference is the whole
/// point. A bool is a field a consumer can simply not read: the next plan's
/// executor is told to iterate this Vec faithfully, and "faithfully" has to
/// mean something the compiler enforces. To get at an `InDistro` step's argv
/// you must match on this type, and a match that does not handle
/// [`Self::RestartDistro`] does not compile. A restart that is skipped is not
/// a cosmetic loss — see [`first_boot_steps`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirstBootAction {
    /// Run a command INSIDE the distro.
    InDistro {
        /// Whether this step needs `-u root`. Every in-distro step here does:
        /// they all run before the unprivileged user has sudo rights, or they
        /// configure sudo itself.
        as_root: bool,
        /// argv AFTER `wsl.exe -d <name> -u <who> --exec`.
        argv: Vec<String>,
    },
    /// Shut the distro down on the HOST and let the next step boot it afresh.
    /// Carries the complete `wsl.exe` argv ([`terminate_argv`]) rather than
    /// making the executor rebuild it, so this Vec really is an executable
    /// description of the sequence and not a description with a footnote.
    RestartDistro {
        /// argv for `wsl.exe` itself — NOT prefixed with `-d <name> --exec`.
        argv: Vec<String>,
    },
}

/// One first-boot step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstBootStep {
    /// Stable id — the wire name in the streamed NDJSON and the key the
    /// ordering test asserts on.
    pub id: &'static str,
    pub action: FirstBootAction,
}

/// The ordered first-boot sequence, root-only by construction.
///
/// Order is the contract, and every edge in it was learned from a real
/// failure on a genuinely fresh `wsl --install archlinux` image (Task 10's
/// live gate, 2026-08-04) — none of these constraints are hypothetical:
///
/// - `wsl-conf` then a RESTART, before anything that needs systemd:
///   `/etc/wsl.conf`'s `[boot] systemd=true` only takes effect on the NEXT
///   boot of the distro, so `docker-enable`'s `systemctl enable --now docker`
///   cannot work in the boot that wrote it. The live gate (2026-08-04) hit
///   exactly this and the operator had to insert `wsl --terminate
///   dml-arch-test` by hand between the steps — see the gate log's Step 3,
///   row (b), "Applies the systemd setting." That restart is now a step of
///   this sequence rather than knowledge someone has to already have.
/// - `useradd` before `sudoers`: the drop-in names a user that must already
///   exist.
/// - `pacman-key` before `pacman-sync` (before ANY package install): a fresh
///   Arch WSL rootfs ships with NO initialized pacman keyring. The very
///   first `pacman -S`/`-Syu` of any kind fails outright —
///   `error: required key missing from keyring` — until `pacman-key --init`
///   and `pacman-key --populate archlinux` have both run.
/// - `pacman-sync` before `sudoers`: `sudo` is not part of Arch's `base`
///   group (`Required By: base-devel`, verified live), so `/etc/sudoers.d`
///   itself does not exist on a fresh image until the `sudo` package has
///   been installed. Writing the drop-in earlier — the ORIGINAL order this
///   module shipped with — hit exactly that on the live gate: `sh: line 1:
///   /etc/sudoers.d/99-dml: No such file or directory`. The `sudoers` step
///   below also `mkdir -p`s the directory itself as a second, independent
///   safeguard — belt AND braces, not belt INSTEAD of braces, because a
///   guard that only prevents this class of failure if some other step
///   stays correctly ordered is not much of a guard.
/// - `pacman-sync` before `docker-group`: `usermod -aG docker` cannot add a
///   group member before the `docker` package has created the group.
///
/// Refuses rather than emitting a poisoned step: `user` is spliced unescaped
/// into a `sh -c` string for the `wsl-conf`/`sudoers` steps, so a `user`
/// containing a shell metacharacter would otherwise run as root. Silently
/// sanitizing the name instead of refusing it is not an option — a caller
/// that typed `d'ml` would end up with an account named `dml` that nobody
/// asked for.
pub fn first_boot_steps(distro: &str, user: &str) -> Result<Vec<FirstBootStep>, String> {
    if !valid_distro_user(user) {
        return Err(format!(
            "{user:?} is not a valid distro user name (lowercase letters, digits, '_' and '-' only, must not start with '-')"
        ));
    }
    let root = |id: &'static str, argv: Vec<String>| FirstBootStep {
        id,
        action: FirstBootAction::InDistro { as_root: true, argv },
    };
    let s = |v: &str| v.to_string();
    Ok(vec![
        // `printf %s` rather than a heredoc: this argv crosses `--exec`, so
        // there is no shell to interpret one, and `printf` writes the exact
        // bytes with no trailing surprise.
        root(
            "wsl-conf",
            vec![
                s("sh"),
                s("-c"),
                format!("printf %s '{WSL_CONF}' > /etc/wsl.conf"),
            ],
        ),
        // THE BOUNDARY. Everything after this line runs in a distro where
        // systemd is PID 1; everything before it does not. Placed immediately
        // after the write, exactly where the live gate operator had to put it
        // by hand.
        FirstBootStep {
            id: "restart",
            action: FirstBootAction::RestartDistro { argv: terminate_argv(distro) },
        },
        root("useradd", vec![s("useradd"), s("-m"), s("-G"), s("wheel"), user.to_string()]),
        // One step, not two: `--init` and `--populate` are inherently
        // sequential (the second is meaningless without the first) and a
        // keyring that is initialized but not populated is exactly as
        // useless to `pacman-sync` as one that is neither — there is no
        // real recoverability gained by giving them separate ids, and it
        // keeps the NDJSON stream to one section for what is really one
        // operation: preparing the keyring.
        root(
            "pacman-key",
            vec![s("sh"), s("-c"), s("pacman-key --init && pacman-key --populate archlinux")],
        ),
        root(
            "pacman-sync",
            {
                let mut v = vec![s("pacman"), s("-Syu"), s("--noconfirm"), s("--needed")];
                v.extend(PACKAGES.iter().map(|p| p.to_string()));
                v
            },
        ),
        root(
            "sudoers",
            vec![
                s("sh"),
                s("-c"),
                format!(
                    "mkdir -p /etc/sudoers.d && printf %s '{user} ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/99-dml && chmod 0440 /etc/sudoers.d/99-dml"
                ),
            ],
        ),
        root("docker-group", vec![s("usermod"), s("-aG"), s("docker"), user.to_string()]),
        root("docker-enable", vec![s("systemctl"), s("enable"), s("--now"), s("docker")]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISTRO: &str = "dml-arch";

    /// The steps, with the ids in order. Most assertions below are about
    /// ORDER, and order is the contract.
    fn ids(user: &str) -> Vec<&'static str> {
        first_boot_steps(DISTRO, user).unwrap().iter().map(|s| s.id).collect()
    }

    /// The argv of an in-distro step, or `None` for the restart boundary.
    fn in_distro_argv(step: &FirstBootStep) -> Option<&[String]> {
        match &step.action {
            FirstBootAction::InDistro { argv, .. } => Some(argv),
            FirstBootAction::RestartDistro { .. } => None,
        }
    }

    fn step(user: &str, id: &str) -> FirstBootStep {
        first_boot_steps(DISTRO, user).unwrap().into_iter().find(|s| s.id == id).unwrap()
    }

    #[test]
    fn the_install_comes_from_the_official_catalog_and_does_not_launch_a_shell() {
        let argv = install_distro_argv("dml-arch");
        assert_eq!(
            argv,
            vec![
                "--install".to_string(),
                "archlinux".to_string(),
                "--name".to_string(),
                "dml-arch".to_string(),
                "--no-launch".to_string(),
            ]
        );
    }

    /// `--no-launch` is not a nicety. Without it `wsl --install` starts the
    /// distro's interactive first-run account setup, which waits on a console
    /// nobody is attached to — the launcher would hang with nothing on screen.
    #[test]
    fn the_install_never_opens_an_interactive_first_run() {
        assert!(install_distro_argv("x").iter().any(|a| a == "--no-launch"));
    }

    #[test]
    fn systemd_is_switched_on_in_wsl_conf() {
        assert!(WSL_CONF.contains("[boot]"));
        assert!(WSL_CONF.contains("systemd=true"));
        // LF only: bash inside WSL chokes on CRLF (.gitattributes enforces the
        // same rule for every shell file in this repo).
        assert!(!WSL_CONF.contains('\r'), "wsl.conf must be LF-only");
    }

    #[test]
    fn buildx_is_installed_because_progress_and_resume_depend_on_it() {
        // install_native.rs's pct parser reads BuildKit vertex headers and
        // resume rests on BuildKit's cache. Without buildx the build silently
        // falls back to the legacy builder: the progress bar goes dead and the
        // failure reads as a hang.
        assert!(PACKAGES.contains(&"docker-buildx"));
        assert!(PACKAGES.contains(&"docker"));
        assert!(PACKAGES.contains(&"docker-compose"));
        assert!(PACKAGES.contains(&"git"));
    }

    /// `sudo` is not part of Arch's `base` group (verified live: `pacman -Qi
    /// sudo` on a real `dml-arch` reports `Required By: base-devel`, not
    /// `base`), so without this package the `sudoers` first-boot step's
    /// NOPASSWD drop-in is inert — the binary that reads it is not installed.
    #[test]
    fn sudo_itself_is_installed_or_the_sudoers_dropin_is_inert() {
        assert!(PACKAGES.contains(&"sudo"), "PACKAGES: {PACKAGES:?}");
    }

    #[test]
    fn first_boot_order_creates_the_user_before_it_needs_one() {
        assert_eq!(
            ids("dml"),
            vec![
                "wsl-conf",
                "restart",
                "useradd",
                "pacman-key",
                "pacman-sync",
                "sudoers",
                "docker-group",
                "docker-enable"
            ]
        );
    }

    /// THE LIVE-GATE FINDING (2026-08-04). `/etc/wsl.conf`'s
    /// `[boot] systemd=true` only takes effect on the NEXT boot of the distro,
    /// so `docker-enable`'s `systemctl enable --now docker` cannot work in the
    /// boot that wrote the file. The gate operator had to insert
    /// `wsl --terminate dml-arch-test` by hand between the steps -- gate log
    /// Step 3, row (b): "Applies the systemd setting."
    ///
    /// The next plan's executor is instructed to iterate this Vec faithfully,
    /// so a Vec with no restart in it GUARANTEES that failure. This asserts the
    /// boundary as a RELATION, not a second copy of the whole list: it holds
    /// however many steps are inserted or renamed around it.
    #[test]
    fn a_restart_boundary_sits_between_wsl_conf_and_docker_enable() {
        let ids = ids("dml");
        let conf = ids.iter().position(|i| *i == "wsl-conf").expect("wsl-conf");
        let restart = ids.iter().position(|i| *i == "restart").expect(
            "no restart boundary: systemd only becomes PID 1 on the NEXT boot after wsl.conf is \
             written, so docker-enable fails without one (live gate, 2026-08-04)",
        );
        let enable = ids.iter().position(|i| *i == "docker-enable").expect("docker-enable");
        assert!(conf < restart, "the restart must FOLLOW the write it applies: {ids:?}");
        assert!(restart < enable, "systemd must be PID 1 before `systemctl enable --now`: {ids:?}");
    }

    /// ...and it is a real, runnable command, not a marker the executor has to
    /// know how to interpret. `terminate_argv` existed with no caller; this is
    /// the caller, so the Vec is a complete executable description.
    #[test]
    fn the_restart_step_carries_the_host_command_that_performs_it() {
        let restart = step("dml", "restart");
        match &restart.action {
            FirstBootAction::RestartDistro { argv } => {
                assert_eq!(argv, &terminate_argv(DISTRO));
                assert!(argv.iter().any(|a| a == "--terminate"), "{argv:?}");
                assert!(argv.iter().any(|a| a == DISTRO), "must name the distro: {argv:?}");
            }
            other => panic!("the restart step must be a RestartDistro action, got {other:?}"),
        }
    }

    /// The restart runs on the HOST, so it is not an in-distro step and must
    /// never be handed to `wsl.exe -d <name> -u <who> --exec`. This is the
    /// property a `restart_after: bool` could not express at all.
    #[test]
    fn the_restart_is_not_an_in_distro_command() {
        assert!(in_distro_argv(&step("dml", "restart")).is_none());
        for id in ["wsl-conf", "useradd", "pacman-sync", "docker-enable"] {
            assert!(in_distro_argv(&step("dml", id)).is_some(), "{id} runs inside the distro");
        }
    }

    /// A fresh Arch WSL rootfs has no initialized pacman keyring — the FIRST
    /// package install of any kind fails with `required key missing from
    /// keyring` until `pacman-key --init`/`--populate` have both run. This is
    /// a real ordering RELATION, not a second copy of the whole-list
    /// assertion above: it holds even if steps are inserted or renamed around
    /// these two.
    #[test]
    fn pacman_key_step_precedes_pacman_sync() {
        let steps = first_boot_steps(DISTRO, "dml").unwrap();
        let key_ix = steps.iter().position(|s| s.id == "pacman-key").unwrap();
        let sync_ix = steps.iter().position(|s| s.id == "pacman-sync").unwrap();
        assert!(key_ix < sync_ix, "pacman-key ({key_ix}) must precede pacman-sync ({sync_ix})");
    }

    /// `/etc/sudoers.d` does not exist on a fresh image until the `sudo`
    /// package (installed by `pacman-sync`) creates it — writing the drop-in
    /// earlier failed on the live gate with "No such file or directory".
    /// A real ordering relation, not a restatement of the whole list.
    #[test]
    fn sudoers_follows_pacman_sync() {
        let steps = first_boot_steps(DISTRO, "dml").unwrap();
        let sync_ix = steps.iter().position(|s| s.id == "pacman-sync").unwrap();
        let sudoers_ix = steps.iter().position(|s| s.id == "sudoers").unwrap();
        assert!(sync_ix < sudoers_ix, "pacman-sync ({sync_ix}) must precede sudoers ({sudoers_ix})");
    }

    /// First boot runs as root and CANNOT use sudo: the sudoers drop-in is
    /// itself one of these steps, so anything invoking `sudo` before it lands
    /// would prompt for a password on a console nobody is attached to. That is
    /// the invariant — not the tautology that a hardcoded `root(...)` helper
    /// returns `as_root: true`. With `sudoers` now moved AFTER `pacman-sync`,
    /// the window where `sudo` is absent is longer than it used to be, which
    /// makes this check more important, not less.
    ///
    /// Checked as a whitespace-delimited WORD anywhere in argv, not just
    /// `argv[0]`: three steps (`wsl-conf`, `pacman-key`, `sudoers`) run their
    /// real command as the BODY of an `sh -c` script rather than as argv[0],
    /// so a program-position-only check would miss `sudo` invoked inside one
    /// of those bodies. `pacman-sync` is skipped by id because its argv
    /// legitimately contains the literal "sudo" as a PACKAGE NAME, which is
    /// not an invocation.
    #[test]
    fn no_first_boot_step_reaches_for_a_sudo_that_does_not_exist_yet() {
        for step in first_boot_steps(DISTRO, "dml").unwrap() {
            // The restart is a HOST command; there is no user to run it as.
            let FirstBootAction::InDistro { as_root, argv } = &step.action else {
                continue;
            };
            assert!(as_root, "{} must run as root", step.id);
            if step.id == "pacman-sync" {
                continue;
            }
            for arg in argv {
                assert!(
                    !arg.split_whitespace().any(|w| w == "sudo"),
                    "{} invokes sudo (found in {arg:?}), but the sudoers drop-in requires it to \
                     already exist: {argv:?}",
                    step.id,
                );
            }
        }
    }

    #[test]
    fn the_sudoers_rule_is_nopasswd_and_scoped_to_the_user() {
        let sudoers = step("dml", "sudoers");
        let joined = in_distro_argv(&sudoers).expect("sudoers runs in the distro").join(" ");
        assert!(joined.contains("dml ALL=(ALL) NOPASSWD: ALL"), "got {joined}");
        assert!(
            joined.contains("/etc/sudoers.d/"),
            "must be a drop-in, never an edit of /etc/sudoers: {joined}"
        );
    }

    #[test]
    fn pacman_never_waits_for_a_confirmation_nobody_can_give() {
        let sync = step("dml", "pacman-sync");
        let argv = in_distro_argv(&sync).expect("pacman-sync runs in the distro");
        assert!(argv.iter().any(|a| a == "--noconfirm"), "got {argv:?}");
    }

    #[test]
    fn ordinary_distro_user_names_are_accepted() {
        assert!(first_boot_steps(DISTRO, "dml").is_ok());
        assert!(first_boot_steps(DISTRO, "dml-arch-test").is_ok());
        assert!(valid_distro_user("dml"));
        assert!(valid_distro_user("dml_2"));
    }

    /// `user` is spliced UNESCAPED into a `sh -c` string for the `wsl-conf`
    /// and `sudoers` steps. The `--exec` crossing means `wsl.exe` itself does
    /// no shell parsing, but `sh -c` is invoked explicitly as the program, so
    /// that string genuinely is shell script: a name containing a single
    /// quote would close the quote early and run whatever follows as root.
    #[test]
    fn a_hostile_user_name_is_refused_not_smuggled_into_a_root_shell_string() {
        for bad in ["d'ml", "d$ml", "d\\ml", "d ml"] {
            assert!(!valid_distro_user(bad), "{bad:?} should be rejected by valid_distro_user");
            assert!(
                first_boot_steps(DISTRO, bad).is_err(),
                "{bad:?} should be refused by first_boot_steps"
            );
        }
    }

    #[test]
    fn set_default_user_uses_manage_not_a_config_edit() {
        assert_eq!(
            set_default_user_argv("dml-arch", "dml"),
            vec![
                "--manage".to_string(),
                "dml-arch".to_string(),
                "--set-default-user".to_string(),
                "dml".to_string(),
            ]
        );
    }

    #[test]
    fn terminate_argv_names_the_distro() {
        assert_eq!(terminate_argv("dml-arch"), vec!["--terminate".to_string(), "dml-arch".to_string()]);
    }
}
