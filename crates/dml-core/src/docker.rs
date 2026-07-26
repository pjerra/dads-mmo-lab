use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Environment override for the docker executable, so a boxed/portable install
/// or CI can point at an arbitrary `docker.exe` without patching discovery.
pub const DOCKER_ENV: &str = "DML_DOCKER";

/// Known absolute install locations for `docker.exe`, tried in order AFTER the
/// `DML_DOCKER` override and a bare `docker` on PATH both miss. The per-user
/// path is first because that is where a default Docker Desktop install landed
/// on this box (`%LOCALAPPDATA%\Programs\DockerDesktop\...`) — it is NOT on the
/// machine PATH, which is exactly why plain `Command::new("docker")` fails and
/// discovery has to look here.
fn candidate_docker_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(
            PathBuf::from(&local)
                .join("Programs/DockerDesktop/resources/bin/docker.exe"),
        );
    }
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        out.push(PathBuf::from(&pf).join("Docker/Docker/resources/bin/docker.exe"));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        out.push(PathBuf::from(&pf86).join("Docker/Docker/resources/bin/docker.exe"));
    }
    out
}

/// Resolve which docker executable to invoke. Pure (no filesystem of its own)
/// so it is unit-testable: the caller supplies the override, the candidate
/// list, and an existence predicate.
///
/// Order: explicit `DML_DOCKER` override (used verbatim, even if absent — an
/// intentional override should surface its own error, not be silently
/// second-guessed) → first candidate the predicate accepts → bare `docker`
/// (let PATH resolution and its own "not found" error speak).
fn resolve_docker_program(
    env_override: Option<OsString>,
    candidates: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> OsString {
    if let Some(ov) = env_override {
        if !ov.is_empty() {
            return ov;
        }
    }
    for c in candidates {
        if exists(c) {
            return c.clone().into_os_string();
        }
    }
    OsString::from("docker")
}

/// The docker executable this process should use, resolved against the real
/// environment and filesystem.
pub fn docker_program() -> OsString {
    resolve_docker_program(
        std::env::var_os(DOCKER_ENV),
        &candidate_docker_paths(),
        |p| p.exists(),
    )
}

/// Environment override for the Docker Desktop APP exe (the `Docker Desktop.exe`
/// GUI, not the `docker` CLI), used by the "Start Docker Desktop" fix action.
pub const DOCKER_DESKTOP_ENV: &str = "DML_DOCKER_DESKTOP";

/// Known absolute install locations for `Docker Desktop.exe`, tried in order.
/// Per-user first (matches this box's `%LOCALAPPDATA%\Programs\DockerDesktop`
/// install — the same tree `candidate_docker_paths` finds the CLI under), then
/// the system location.
fn candidate_docker_desktop_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(PathBuf::from(&local).join("Programs/DockerDesktop/Docker Desktop.exe"));
    }
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        out.push(PathBuf::from(&pf).join("Docker/Docker/Docker Desktop.exe"));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        out.push(PathBuf::from(&pf86).join("Docker/Docker/Docker Desktop.exe"));
    }
    out
}

/// Resolve the Docker Desktop app exe. Order: `DML_DOCKER_DESKTOP` override
/// (used verbatim) → first candidate the predicate accepts → `None`. Unlike the
/// CLI resolver there is NO bare-name fallback: `Docker Desktop.exe` is never on
/// PATH, so a miss means "not found" and the caller declines to launch a guess
/// rather than spawn something arbitrary. Pure, for testing.
fn resolve_docker_desktop(
    env_override: Option<OsString>,
    candidates: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> Option<OsString> {
    if let Some(ov) = env_override {
        if !ov.is_empty() {
            return Some(ov);
        }
    }
    for c in candidates {
        if exists(c) {
            return Some(c.clone().into_os_string());
        }
    }
    None
}

/// The Docker Desktop app exe to launch, resolved against the real environment
/// and filesystem. `None` when no known install location exists.
pub fn docker_desktop_program() -> Option<OsString> {
    resolve_docker_desktop(
        std::env::var_os(DOCKER_DESKTOP_ENV),
        &candidate_docker_desktop_paths(),
        |p| p.exists(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_everything() {
        let got = resolve_docker_program(
            Some(OsString::from(r"C:\custom\docker.exe")),
            &[PathBuf::from(r"C:\a\docker.exe")],
            |_| true,
        );
        assert_eq!(got, OsString::from(r"C:\custom\docker.exe"));
    }

    #[test]
    fn empty_override_is_ignored() {
        // An empty env var must not short-circuit to "" — fall through to
        // candidates instead.
        let got = resolve_docker_program(
            Some(OsString::new()),
            &[PathBuf::from(r"C:\a\docker.exe")],
            |_| true,
        );
        assert_eq!(got, OsString::from(r"C:\a\docker.exe"));
    }

    #[test]
    fn first_existing_candidate_wins() {
        let cands = vec![
            PathBuf::from(r"C:\missing\docker.exe"),
            PathBuf::from(r"C:\present\docker.exe"),
        ];
        let got = resolve_docker_program(None, &cands, |p| {
            p == Path::new(r"C:\present\docker.exe")
        });
        assert_eq!(got, OsString::from(r"C:\present\docker.exe"));
    }

    #[test]
    fn falls_back_to_bare_docker_on_path() {
        let got = resolve_docker_program(
            None,
            &[PathBuf::from(r"C:\missing\docker.exe")],
            |_| false,
        );
        assert_eq!(got, OsString::from("docker"));
    }

    #[test]
    fn docker_desktop_override_wins() {
        let got = resolve_docker_desktop(
            Some(OsString::from(r"C:\custom\Docker Desktop.exe")),
            &[PathBuf::from(r"C:\a\Docker Desktop.exe")],
            |_| true,
        );
        assert_eq!(got, Some(OsString::from(r"C:\custom\Docker Desktop.exe")));
    }

    #[test]
    fn docker_desktop_first_existing_candidate_wins() {
        let cands = vec![
            PathBuf::from(r"C:\missing\Docker Desktop.exe"),
            PathBuf::from(r"C:\present\Docker Desktop.exe"),
        ];
        let got = resolve_docker_desktop(None, &cands, |p| {
            p == Path::new(r"C:\present\Docker Desktop.exe")
        });
        assert_eq!(got, Some(OsString::from(r"C:\present\Docker Desktop.exe")));
    }

    #[test]
    fn docker_desktop_none_when_nothing_exists() {
        // No bare-name fallback: a miss must be None, never a guessed launch.
        let got = resolve_docker_desktop(
            None,
            &[PathBuf::from(r"C:\missing\Docker Desktop.exe")],
            |_| false,
        );
        assert_eq!(got, None);
    }

    #[test]
    fn candidate_paths_include_per_user_location() {
        // With LOCALAPPDATA set, the per-user Docker Desktop path is a candidate
        // and comes before the system one.
        std::env::set_var("LOCALAPPDATA", r"C:\Users\test\AppData\Local");
        let cands = candidate_docker_paths();
        assert!(cands
            .first()
            .unwrap()
            .to_string_lossy()
            .contains("Programs"));
        assert!(cands
            .iter()
            .any(|p| p.to_string_lossy().contains("DockerDesktop")));
    }
}
