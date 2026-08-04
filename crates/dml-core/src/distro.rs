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
pub const PACKAGES: [&str; 4] = ["docker", "docker-compose", "docker-buildx", "git"];

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

/// One first-boot step, run inside the distro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstBootStep {
    /// Stable id — the wire name in the streamed NDJSON and the key the
    /// ordering test asserts on.
    pub id: &'static str,
    /// Whether this step needs `-u root`. Every step here does: they all run
    /// before the unprivileged user has sudo rights, or they configure sudo
    /// itself.
    pub as_root: bool,
    /// argv AFTER `wsl.exe -d <name> -u <who> --exec`.
    pub argv: Vec<String>,
}

/// The ordered first-boot sequence, root-only by construction.
///
/// Order is the contract: the sudoers drop-in cannot be written for a user that
/// does not exist, and `usermod -aG docker` cannot add a group member before
/// the `docker` package has created the group.
pub fn first_boot_steps(user: &str) -> Vec<FirstBootStep> {
    let root = |id: &'static str, argv: Vec<String>| FirstBootStep { id, as_root: true, argv };
    let s = |v: &str| v.to_string();
    vec![
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
        root("useradd", vec![s("useradd"), s("-m"), s("-G"), s("wheel"), user.to_string()]),
        root(
            "sudoers",
            vec![
                s("sh"),
                s("-c"),
                format!(
                    "printf %s '{user} ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/99-dml && chmod 0440 /etc/sudoers.d/99-dml"
                ),
            ],
        ),
        root(
            "pacman-sync",
            {
                let mut v = vec![s("pacman"), s("-Syu"), s("--noconfirm"), s("--needed")];
                v.extend(PACKAGES.iter().map(|p| p.to_string()));
                v
            },
        ),
        root("docker-group", vec![s("usermod"), s("-aG"), s("docker"), user.to_string()]),
        root("docker-enable", vec![s("systemctl"), s("enable"), s("--now"), s("docker")]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn first_boot_order_creates_the_user_before_it_needs_one() {
        let ids: Vec<&str> = first_boot_steps("dml").iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec!["wsl-conf", "useradd", "sudoers", "pacman-sync", "docker-group", "docker-enable"]
        );
    }

    /// First boot runs as root and CANNOT use sudo: the sudoers drop-in is
    /// itself one of these steps, so anything invoking `sudo` before it lands
    /// would prompt for a password on a console nobody is attached to. That is
    /// the invariant — not the tautology that a hardcoded `root(...)` helper
    /// returns `as_root: true`.
    #[test]
    fn no_first_boot_step_reaches_for_a_sudo_that_does_not_exist_yet() {
        for step in first_boot_steps("dml") {
            assert!(step.as_root, "{} must run as root", step.id);
            assert!(
                !step.argv.iter().any(|a| a == "sudo"),
                "{} invokes sudo, but the sudoers drop-in is step 3 of this very list: {:?}",
                step.id,
                step.argv
            );
        }
    }

    #[test]
    fn the_sudoers_rule_is_nopasswd_and_scoped_to_the_user() {
        let step = first_boot_steps("dml").into_iter().find(|s| s.id == "sudoers").unwrap();
        let joined = step.argv.join(" ");
        assert!(joined.contains("dml ALL=(ALL) NOPASSWD: ALL"), "got {joined}");
        assert!(
            joined.contains("/etc/sudoers.d/"),
            "must be a drop-in, never an edit of /etc/sudoers: {joined}"
        );
    }

    #[test]
    fn pacman_never_waits_for_a_confirmation_nobody_can_give() {
        let step = first_boot_steps("dml").into_iter().find(|s| s.id == "pacman-sync").unwrap();
        assert!(step.argv.iter().any(|a| a == "--noconfirm"), "got {:?}", step.argv);
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
