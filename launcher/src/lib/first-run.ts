// The first-run decision: what a stranger sees instead of Home, and the ONE
// button that moves them forward.
//
// SHIP-LIST 4.4. Until now a new user installed the launcher, landed on Home,
// and got a status card for a server that does not exist with nothing to
// click. This module turns the `backend_status` probe chain
// (crates/dml-core/src/setup.rs) into that screen.
//
// CONSUME, DO NOT RE-DERIVE. The chain already decided which link is missing
// and put it in `state`; `probes` is diagnostics for display only. Re-deriving
// the decision here is how the screen and the setup command end up disagreeing
// about what state the machine is in.
//
// Kept free of Svelte and Tauri -- same shape as server-gate.ts and
// title-install.ts -- so the copy, the state mapping and the button are
// vitest-pinned rather than click-tested.
//
// THREE RULES THIS FILE EXISTS TO ENFORCE:
//
//  1. The launcher does NOT create Windows features or WSL distros. That needs
//     elevation and stays with Install-DML.ps1. So `no_wsl` and `no_distro`
//     get an explanation and a way to reach the installer -- never a button
//     that pretends the launcher can do it.
//  2. A could-not-tell must never render as "you have nothing installed". It
//     says the launcher could not check, names the step that went dark, and
//     offers a retry. Nothing gets repaired off a shrug.
//  3. This is the first thing a stranger sees, so it reads as calm and
//     finished, following ServerRequired.svelte's precedent: a greeting
//     carrying the fixing action, not a failure report.

import type { PageId } from "./nav";

// --- the `backend_status` wire shape ---------------------------------------
// Mirrors dml_core::setup::BackendStatus (flattened) plus the two fields
// launcher/src-tauri/src/lib.rs adds. Declared HERE rather than in api.ts for
// the same reason TitleCatalog lives in title-install.ts: this module is the
// one that reasons about the shape, and api.ts re-exports it.

export type Tri = "yes" | "no" | "unknown";

export type SetupState =
  | "no_wsl"
  | "no_distro"
  | "no_cli"
  | "cli_outdated"
  | "no_titles"
  | "ready"
  | "unknown";

export type SetupStep = "wsl" | "distro" | "cli" | "titles";

export interface BackendProbes {
  wsl: Tri;
  distro: Tri;
  cli: Tri;
  cli_version: string | null;
  titles: number | null;
}

/** Whether the backend payload the installer carries actually arrived. */
export interface PayloadStatus {
  present: Tri;
  dir: string | null;
  missing: string[];
}

export interface BackendStatusReport {
  state: SetupState;
  blocked_at: SetupStep | null;
  /**
   * What the probe that went dark actually SAID — one bounded line of
   * wsl.exe's (or the distro's) own output, UTF-16-decoded on the Rust side.
   * Set only alongside `blocked_at`; null whenever there was nothing to quote.
   */
  detail: string | null;
  distro: string;
  expected_cli_version: string;
  probes: BackendProbes;
  backend_mode: "wsl" | "native";
  payload: PayloadStatus;
}

// --- the screen -------------------------------------------------------------

export type FirstRunKind =
  | "no-wsl"
  | "no-distro"
  | "no-cli"
  | "cli-outdated"
  | "no-titles"
  | "payload-missing"
  | "payload-unknown"
  | "unknown";

/**
 * The one button. A discriminated union so the component is forced by
 * svelte-check to handle every arm -- a new state cannot ship with a button
 * that does nothing.
 *
 * There is deliberately no "create the distro" arm.
 */
export type FirstRunAction =
  | { kind: "setup"; label: string }
  | { kind: "nav"; label: string; page: PageId }
  | { kind: "link"; label: string; url: string }
  | { kind: "retry"; label: string };

export interface FirstRunState {
  kind: FirstRunKind;
  title: string;
  /** The whole explanation: one sentence, or two when a state has two remedies. */
  body: string;
  action: FirstRunAction;
  /** Small diagnostics line under the button. Empty when there is nothing useful to add. */
  detail: string;
}

/**
 * Where the elevated substrate installer lives. Same URL Help.svelte's
 * Community card opens -- never invent a second one, and never invent a
 * deep link to a file that may be renamed.
 *
 * SCOPE: this is the UPSTREAM project. It carries `guides/DML-Windows/
 * Install-DML.ps1`, which is why `no_wsl`/`no_distro` link to it. It does NOT
 * carry this launcher and publishes no release of it, so it must never be
 * offered as a place to download the app itself.
 */
export const PROJECT_URL = "https://github.com/DadsMmoLab/dads-mmo-lab";

/**
 * The diagnostics line, made readable by a human.
 *
 * The chain quotes the FIRST line the probe printed, verbatim — which is right
 * for `wsl.exe`, whose complaints are sentences, and wrong for the two probes
 * that run `dml ... --json` inside the distro: their first line is our own
 * envelope, so a titles-step (or an unparseable cli-step) shrug rendered
 * `{"ok":false,"error":{"code":...}}` at 12px as the user's one clue.
 *
 * The envelope's `message` IS the human sentence, so this pulls it out rather
 * than inventing anything. Nothing is lost: `report.detail` still carries the
 * raw text for a bug report, this only changes what the card shows.
 *
 * Anything that does not look like JSON (every `wsl.exe` message) is returned
 * untouched — including the truncated tail the Rust side appends at 200 chars,
 * which is why the fields are also matched textually when `JSON.parse` fails.
 */
export function humanDetail(raw: string): string {
  const t = raw.trim();
  if (!t.startsWith("{")) return raw;

  let message: string | undefined;
  let code: string | undefined;
  try {
    const obj = JSON.parse(t) as Record<string, unknown>;
    const err = (obj.error ?? obj) as Record<string, unknown>;
    if (typeof err?.message === "string") message = err.message;
    if (typeof err?.code === "string") code = err.code;
  } catch {
    // A bounded quote is very often CUT OFF mid-object (the Rust side stops at
    // 200 chars and appends an ellipsis), which is exactly the case that most
    // needs rescuing from a raw blob -- and the case where the string we want
    // has no closing quote left, hence the second, open-ended pattern.
    const unescape = (s: string) =>
      s.replace(/\\n/g, " ").replace(/\\"/g, '"').replace(/\\\\/g, "\\");
    const grab = (key: string): string | undefined => {
      const body = `"${key}"\\s*:\\s*"((?:[^"\\\\]|\\\\.)*)`;
      const closed = new RegExp(`${body}"`).exec(t);
      if (closed) return unescape(closed[1]);
      const open = new RegExp(`${body}$`).exec(t);
      return open ? unescape(open[1]) : undefined;
    };
    message = grab("message");
    code = grab("code");
  }

  message = message?.trim();
  code = code?.trim();
  if (message) return code ? `${message} (${code})` : message;
  if (code) return `The DML backend reported ${code}.`;
  // JSON we cannot read anything out of. Still not a blob on the card.
  return "The DML backend answered with something the launcher couldn't read.";
}

/** The screen for every could-not-tell, whatever produced it. */
function couldNotTell(step: SetupStep | null, distro: string | null, detail: string): FirstRunState {
  const where = distro ?? "the Linux environment";
  const body =
    step === "wsl"
      ? "The launcher asked Windows about WSL and didn't get an answer back, so it can't tell you what's set up here yet — this usually clears on its own."
      : step === "distro"
        ? `The launcher couldn't read back the list of WSL environments, so it can't tell yet whether ${where} is on this PC.`
        : step === "cli"
          ? `The launcher couldn't get an answer out of ${where}, so it can't tell yet which version of the DML backend is in there.`
          : step === "titles"
            ? "The DML backend answered but the launcher couldn't read back the list of installed games, so it can't tell yet what you have."
            : "The launcher couldn't run its setup check just now, so it can't tell yet what's on this PC.";
  return {
    kind: "unknown",
    title: "Couldn't check this PC's setup",
    body,
    action: { kind: "retry", label: "Check again" },
    detail: humanDetail(detail),
  };
}

/**
 * The screen for the two states the launcher can actually fix -- but only
 * when the payload that powers the fix is really here.
 *
 * `no_cli`/`cli_outdated` are the only states with a "Set up backend" button
 * on them, and that button installs from the resources bundled into this exe.
 * If they did not ship (or we cannot even locate them), the honest screen says
 * so; offering a fix that cannot work is worse than admitting it.
 */
function setupScreen(r: BackendStatusReport): FirstRunState {
  if (r.payload.present === "unknown") {
    return {
      kind: "payload-unknown",
      title: "Couldn't find the launcher's own setup files",
      body: `The DML backend isn't in ${r.distro} yet, and the launcher couldn't work out where its bundled setup files live, so it can't install it just now.`,
      action: { kind: "retry", label: "Check again" },
      detail: "",
    };
  }
  if (r.payload.present === "no") {
    const where = r.payload.dir ? ` (looked in ${r.payload.dir})` : "";
    return {
      kind: "payload-missing",
      title: "This copy of the launcher didn't bring its setup files",
      // NOT a link to PROJECT_URL. That repository carries Install-DML.ps1 but
      // no launcher and no release of it, so "get a complete release" sent the
      // one user who cannot fix themselves to a page where the thing they were
      // told to download does not exist.
      //
      // TWO CAUSES, TWO REMEDIES — and the copy has to carry both, because the
      // one remedy it used to name does not fix the commoner cause. If the exe
      // was dragged out of its install folder, reinstalling repairs the
      // ORIGINAL directory and the copy in front of the user stays exactly as
      // broken: Check again then fails forever with no hint that WHICH exe they
      // launched is the problem. So the moved case is named first (the fix is
      // free and instant), the quarantine case second. Both are repaired IN
      // PLACE, which is why the button re-checks rather than navigating away,
      // and `detail` names the directory we looked in — the thread to pull when
      // neither sentence describes you.
      body: `The DML backend isn't in ${r.distro} yet and the launcher can't put it there, because the files it ships for that job aren't in this copy. If you moved or copied the launcher out of the folder it was installed into, start it from that folder instead — otherwise running the launcher's installer again puts the files back, and if your antivirus quarantined them you'll need to allow them first.`,
      action: { kind: "retry", label: "Check again" },
      detail: `Missing: ${r.payload.missing.join(", ")}${where}`,
    };
  }
  if (r.state === "cli_outdated") {
    const found = r.probes.cli_version ?? "an older build";
    return {
      kind: "cli-outdated",
      title: "The backend needs bringing up to date",
      body: `${r.distro} has DML backend ${found} in it and this launcher speaks ${r.expected_cli_version} — installing the copy that shipped with the launcher puts the two back in step.`,
      action: { kind: "setup", label: "Update backend" },
      detail: "",
    };
  }
  return {
    kind: "no-cli",
    title: "One step left: set up the backend",
    body: `Your ${r.distro} environment is ready but doesn't have the DML backend in it yet — the launcher can install it from the files it shipped with, which takes a few seconds.`,
    action: { kind: "setup", label: "Set up backend" },
    detail: "",
  };
}

/**
 * The first-run screen to render INSTEAD of Home, or null to render Home
 * exactly as it renders today.
 *
 * `everReady` is a session latch, set the first time the chain answers
 * `ready`. It exists for a specific regression: an established user's
 * `games list` probe times out once, the chain honestly answers `unknown`, and
 * their working Home is replaced by a "couldn't check" card. Once a machine has
 * proven itself set up, this screen is finished with it for the session.
 *
 * A null report with no error is "no probe has landed yet" and deliberately
 * does NOT take over Home -- the serverGate makes the same call, for the same
 * reason: it would flash a setup screen at every user on every cold start.
 */
export function firstRunState(o: {
  report: BackendStatusReport | null;
  error?: string | null;
  everReady: boolean;
}): FirstRunState | null {
  if (o.everReady) return null;

  const r = o.report;
  if (!r) {
    // The probe call itself never landed. That says nothing about the machine,
    // so it is a could-not-tell like any other.
    return o.error ? couldNotTell(null, null, o.error) : null;
  }

  // Native backend: a real server with no distro at all. The chain honestly
  // answers `no_wsl` for these users, and showing it would tell someone with a
  // working server to go install WSL.
  if (r.backend_mode === "native") return null;

  switch (r.state) {
    case "ready":
      return null;

    case "unknown":
      // The machine's OWN words first: `error` describes our IPC, `detail`
      // describes their PC, and this is the one screen with no repair on it.
      return couldNotTell(r.blocked_at, r.distro, r.detail ?? o.error ?? "");

    case "no_wsl":
      return {
        kind: "no-wsl",
        title: "Windows Subsystem for Linux isn't set up on this PC yet",
        body: "Your server runs inside WSL2, and creating it takes one elevated run of Install-DML.ps1 from the Dad's MMO Lab project — the launcher deliberately doesn't switch Windows features on for you.",
        action: { kind: "link", label: "Where to get Install-DML.ps1 ↗", url: PROJECT_URL },
        detail: "",
      };

    case "no_distro":
      return {
        kind: "no-distro",
        title: `The ${r.distro} environment isn't set up yet`,
        body: `WSL2 is working on this PC, but there's no ${r.distro} environment yet — one elevated run of Install-DML.ps1 from the Dad's MMO Lab project creates it, and the launcher takes over from there.`,
        action: { kind: "link", label: "Where to get Install-DML.ps1 ↗", url: PROJECT_URL },
        detail: "",
      };

    case "no_cli":
    case "cli_outdated":
      return setupScreen(r);

    case "no_titles":
      return {
        kind: "no-titles",
        title: "No game server installed yet",
        body: "Everything the launcher needs is in place — pick a game in the Library and it will install and set up that server for you.",
        action: { kind: "nav", label: "Open Library", page: "library" },
        detail: "",
      };
  }
}

/** How the one button renders right now. */
export interface FirstRunButton {
  label: string;
  disabled: boolean;
}

/**
 * The button's live state, given what is currently running.
 *
 * WHY THIS IS NOT JUST `action.label`. The retry arm is the one that hurts: the
 * probe chain's cold-start budget is 120 s, so on a wedged distro "Check again"
 * used to sit there enabled and unchanged for two minutes while `probeBackend`
 * silently dropped every re-click on its re-entry guard. A user reasonably
 * concludes the button is broken — on the ONE screen that has no other repair
 * on it. Both slow arms therefore say so and refuse the second press:
 * `setup` while `backend_setup` streams, `retry` while a probe is in flight.
 *
 * Lives here rather than in FirstRun.svelte for the same reason the copy does:
 * this is a decision, and decisions in this module are vitest-pinned instead of
 * click-tested.
 */
export function firstRunButton(
  action: FirstRunAction,
  o: { setupRunning: boolean; rechecking: boolean; setupLocked: boolean },
): FirstRunButton {
  if (action.kind === "setup") {
    return {
      // A setup run always ends in a re-probe (FirstRun.svelte's runSetup), and
      // that probe outlives the run: `running` is already false while the
      // launcher is still finding out whether the install took. Saying nothing
      // for those seconds left the ONE button on the screen enabled and reading
      // "Set up backend", which invites a second full install on top of the
      // answer being fetched.
      label: o.setupRunning ? "Setting up…" : o.rechecking ? "Checking…" : action.label,
      disabled: o.setupRunning || o.rechecking || o.setupLocked,
    };
  }
  if (action.kind === "retry") {
    return {
      label: o.rechecking ? "Checking…" : action.label,
      // The mirror image of the same rule: neither slow job may leave the
      // other's button pressable.
      disabled: o.rechecking || o.setupRunning,
    };
  }
  // nav / link: instant, and neither of them is what the running job is about.
  return { label: action.label, disabled: o.setupRunning };
}

/**
 * Whether it is worth spending another probe (up to three `wsl.exe` spawns) on
 * this machine.
 *
 * Called on every navigation to Home, so a user who leaves for the Library,
 * installs a title and comes back sees the screen update instead of a stale
 * "no game server installed". It stops once the machine is set up (or is a
 * native-backend one, which has no distro to ask about) so an established user
 * pays exactly one probe per session.
 */
export function firstRunNeedsProbe(
  report: BackendStatusReport | null,
  everReady: boolean,
): boolean {
  if (everReady) return false;
  if (!report) return true;
  if (report.backend_mode === "native") return false;
  return report.state !== "ready";
}
