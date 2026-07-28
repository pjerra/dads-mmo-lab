// Why the Library page's Install control is (un)available, and what to SAY.
//
// The live blocker this module fixes: `script_available` is a single boolean
// with two very different causes, and Library hardcoded one explanation for
// both -- "Re-run cli/dev-install.ps1 to ship installer scripts". On the
// Windows-native backend the bash CLI runs under Git Bash, where the
// installers dir (a Linux path) does not exist, so EVERY title came back
// `script_available:false` and every user was told to re-run a dev-install
// step that was already fine and that could never have helped: the six DML
// installers are Linux scripts (sudo, pacman/apt, systemd, usermod,
// /var/run/docker.sock) and cannot run on Windows at all.
//
// So the CLI now answers the host question separately (`install_supported`
// in the `games catalog` envelope, cli/src/80-titles.sh `_installers_supported`)
// and this module turns the pair into one decision plus honest copy.
//
// Kept free of Svelte/Tauri -- same shape as server-gate.ts and
// docker-restart.ts -- so the user-visible decisions (is Install offered, what
// does the disabled tooltip say, does the page explain itself) are
// vitest-pinned.

import type { TitleInfo } from "./api";

/** Why Install is not offered. `none` = it is offered. */
export type InstallBlock = "none" | "unsupported-host" | "missing-scripts";

export interface InstallGate {
  block: InstallBlock;
  /** Whether an armed Install button should be rendered at all. */
  canInstall: boolean;
  /** Tooltip for the disabled button. Empty string when `canInstall`. */
  hint: string;
}

/**
 * The host cannot run the installers. Naming the backend is the whole point:
 * this is the sentence that replaces the dev-install blame.
 */
export const UNSUPPORTED_HOST_HINT =
  "Installing titles needs the WSL backend — these installers are Linux scripts and can't run on the Docker Desktop (native) backend.";

/**
 * The host CAN run them, they just aren't shipped here. This is the only case
 * where re-running the dev-install step is the actual fix, so it is the only
 * case that says so.
 *
 * The PowerShell script is named parenthetically, not as the instruction: this
 * state is reachable on a Linux host too (a Linux dev running the launcher
 * against a local dml), where `cli/dev-install.ps1` is not the route and
 * naming it would repeat the original sin of this bug in a smaller way.
 */
export const MISSING_SCRIPTS_HINT =
  "Installer scripts aren't shipped on this backend yet — re-run the dev-install step (cli/dev-install.ps1 on Windows) to ship them.";

/**
 * Install-from-URL runs a community installer fetched from a git repo, but it
 * runs it the SAME way and on the SAME host as a catalog title, so the host
 * verdict binds it identically. It is deliberately NOT gated on
 * `scriptAvailable`: nothing is shipped ahead of time for a URL install, so
 * that half of the question does not apply.
 *
 * Missed on the first cut of this fix: the page-level notice said "installing
 * titles needs the WSL backend" while the URL card right below it stayed
 * armed, so the user was told the truth and then handed a control that walked
 * into the identical failure.
 */
export function urlInstallGate(o: { installSupported: boolean }): InstallGate {
  return o.installSupported
    ? { block: "none", canInstall: true, hint: "" }
    : { block: "unsupported-host", canInstall: false, hint: UNSUPPORTED_HOST_HINT };
}

/**
 * Decide the Install control for ONE available title.
 *
 * Host verdict outranks the file check: on Windows the installers dir is
 * missing too, so checking the file first is exactly how the misdiagnosis
 * happened. (The CLI's own `games install` arm orders the two checks the same
 * way -- cli/src/90-main.sh.)
 */
export function titleInstallGate(o: {
  installSupported: boolean;
  scriptAvailable: boolean;
}): InstallGate {
  if (!o.installSupported) {
    return { block: "unsupported-host", canInstall: false, hint: UNSUPPORTED_HOST_HINT };
  }
  if (!o.scriptAvailable) {
    return { block: "missing-scripts", canInstall: false, hint: MISSING_SCRIPTS_HINT };
  }
  return { block: "none", canInstall: true, hint: "" };
}

export interface InstallNotice {
  title: string;
  body: string;
}

/**
 * A page-level explanation shown above the "Available titles" cards, in the
 * spirit of ServerRequired: state the real reason and the route out of it,
 * rather than leaving a row of dead buttons to be hovered one by one.
 *
 * Returns null when there is nothing to explain -- installs work, or there is
 * no available title to install anyway (an explanation over an empty list is
 * noise, not honesty).
 */
export function installUnavailableNotice(o: {
  installSupported: boolean;
  availableCount: number;
}): InstallNotice | null {
  if (o.installSupported) return null;
  if (o.availableCount <= 0) return null;
  return {
    title: "Installing titles needs the WSL backend",
    body:
      "The title installers are Linux scripts — they install Docker with sudo and pacman/apt and enable it with systemd — so they can't run on the Docker Desktop (native) backend. " +
      'To install a title, switch Settings → Launcher → Server backend to "WSL (dml-arch distro)"; if you have no dml-arch distro yet, that is what Install-DML.ps1 sets up. ' +
      "Titles you already installed keep working here.",
  };
}

/** The `games catalog` envelope, as the frontend wants it. */
export interface TitleCatalog {
  titles: TitleInfo[];
  /** Whether the CLI's HOST can run the installers at all. */
  installSupported: boolean;
}

/**
 * Normalize the raw `games catalog` payload.
 *
 * `install_supported` FAILS OPEN: a `dml` predating this field (an older copy
 * installed in dml-arch that the launcher has not re-shipped yet) omits it,
 * and on that backend installs genuinely work -- treating "absent" as "not
 * supported" would swap one wrong story for another. Only an explicit `false`
 * blocks.
 *
 * `display_name` / `custom_name` (server rename) fail open for the SAME
 * version-skew reason: an older `dml` omits them, and the label chain must
 * still end somewhere non-empty -- custom name, else the built-in name, else
 * the id. A title card with a blank heading is a worse failure than one
 * showing an id.
 */
export function normalizeCatalog(d: {
  titles?: TitleInfo[];
  install_supported?: boolean;
}): TitleCatalog {
  return {
    titles: (d.titles ?? []).map((t) => ({
      ...t,
      display_name: t.display_name?.trim() ? t.display_name : (t.name?.trim() ? t.name : t.id),
      custom_name: t.custom_name ?? null,
    })),
    installSupported: d.install_supported !== false,
  };
}
