import { describe, it, expect } from "vitest";
import {
  firstRunState,
  firstRunNeedsProbe,
  PROJECT_URL,
  type BackendStatusReport,
  type FirstRunState,
} from "./first-run";

// SHIP-LIST 4.4. A stranger who installs the launcher lands on Home and sees a
// status card for a server that does not exist. These tests pin the decision
// that replaces that dead end: which of the three setup states we are in, what
// the ONE sentence says, and what the ONE button does.
//
// Two rules are load-bearing and have a test each below:
//   * a could-not-tell probe must never render as "you have nothing installed"
//     -- it says the launcher could not check, and why;
//   * the launcher deliberately does NOT create WSL or distros, so no state may
//     offer a button that pretends it can.

function report(o: Partial<BackendStatusReport> = {}): BackendStatusReport {
  return {
    state: "ready",
    blocked_at: null,
    distro: "dml-arch",
    expected_cli_version: "3.0.0",
    probes: { wsl: "yes", distro: "yes", cli: "yes", cli_version: "3.0.0", titles: 2 },
    backend_mode: "wsl",
    payload: { present: "yes", dir: "C:\\Program Files\\DML Launcher", missing: [] },
    ...o,
  };
}

/** The screen a given report produces, or a hard failure if it produced none. */
function shown(o: Parameters<typeof firstRunState>[0]): FirstRunState {
  const s = firstRunState(o);
  if (!s) throw new Error(`expected a first-run screen, got null for ${JSON.stringify(o)}`);
  return s;
}

describe("firstRunState — when NOT to take over Home", () => {
  it("renders nothing before the first probe has landed", () => {
    // Same call the serverGate makes: a null answer is "we don't know yet",
    // and taking over Home on it would flash a setup screen at every user on
    // every cold start.
    expect(firstRunState({ report: null, everReady: false })).toBeNull();
  });

  it("renders nothing once the machine is fully set up", () => {
    expect(firstRunState({ report: report(), everReady: false })).toBeNull();
  });

  it("renders nothing in native mode, whatever the WSL chain says", () => {
    // A native-backend user runs a real server with no distro at all. The
    // chain honestly answers no_wsl for them, and showing it would tell
    // someone with a working server to go install WSL.
    expect(
      firstRunState({ report: report({ state: "no_wsl", backend_mode: "native" }), everReady: false }),
    ).toBeNull();
  });

  it("never takes Home away again once this session has seen the machine ready", () => {
    // The regression this latch prevents: an existing user's `games list`
    // times out once, the chain answers Unknown, and their working Home is
    // replaced by a "couldn't check" card.
    expect(
      firstRunState({
        report: report({ state: "unknown", blocked_at: "titles" }),
        everReady: true,
      }),
    ).toBeNull();
  });
});

describe("firstRunState — state 1: no WSL, no distro (the launcher cannot fix these)", () => {
  it("explains that WSL itself is missing and sends the user to the elevated installer", () => {
    const s = shown({
      report: report({ state: "no_wsl", probes: { wsl: "no", distro: "no", cli: "unknown", cli_version: null, titles: null } }),
      everReady: false,
    });
    expect(s.kind).toBe("no-wsl");
    expect(s.body).toContain("Install-DML.ps1");
    expect(s.action.kind).toBe("link");
    expect(s.action.kind === "link" && s.action.url).toBe(PROJECT_URL);
  });

  it("names the distro it looked for when only the distro is missing", () => {
    const s = shown({
      report: report({
        state: "no_distro",
        distro: "dml-arch",
        probes: { wsl: "yes", distro: "no", cli: "unknown", cli_version: null, titles: null },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("no-distro");
    expect(s.body).toContain("dml-arch");
    expect(s.body).toContain("Install-DML.ps1");
  });

  it("uses the distro name from the report, not a hardcoded one", () => {
    const s = shown({ report: report({ state: "no_distro", distro: "dml-test" }), everReady: false });
    expect(s.body).toContain("dml-test");
    expect(s.body).not.toContain("dml-arch");
  });

  it.each(["no_wsl", "no_distro"] as const)(
    "never offers a setup button for %s — the launcher does not create distros",
    (state) => {
      const s = shown({ report: report({ state }), everReady: false });
      expect(s.action.kind).not.toBe("setup");
    },
  );
});

describe("firstRunState — state 2: distro but no CLI (the launcher CAN fix this)", () => {
  it("offers the setup button when the backend is simply absent", () => {
    const s = shown({
      report: report({
        state: "no_cli",
        probes: { wsl: "yes", distro: "yes", cli: "no", cli_version: null, titles: null },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("no-cli");
    expect(s.action.kind).toBe("setup");
    expect(s.action.label).toBeTruthy();
  });

  it("offers the same fix for an outdated backend, and names both versions", () => {
    // The COMMON case: Install-DML.ps1 still bootstraps CLI 2.6.0, so a
    // freshly-created distro arrives outdated rather than empty.
    const s = shown({
      report: report({
        state: "cli_outdated",
        expected_cli_version: "3.0.0",
        probes: { wsl: "yes", distro: "yes", cli: "yes", cli_version: "2.6.0", titles: null },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("cli-outdated");
    expect(s.action.kind).toBe("setup");
    expect(s.body).toContain("2.6.0");
    expect(s.body).toContain("3.0.0");
  });

  it("withdraws the setup button when the payload did not ship with this build", () => {
    // The button is powered by bundled resources. Offering a fix that cannot
    // possibly work is worse than saying so.
    const s = shown({
      report: report({
        state: "no_cli",
        payload: { present: "no", dir: "C:\\app", missing: ["cli/dml", "installers/install-wow-wotlk.sh"] },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("payload-missing");
    expect(s.action.kind).not.toBe("setup");
    expect(s.detail).toContain("cli/dml");
  });

  it("says it could not FIND its setup files rather than accusing them of being incomplete", () => {
    // payload present=unknown means the resource dir would not resolve. That
    // is a could-not-tell, not a missing payload.
    const s = shown({
      report: report({ state: "no_cli", payload: { present: "unknown", dir: null, missing: [] } }),
      everReady: false,
    });
    expect(s.kind).toBe("payload-unknown");
    expect(s.action.kind).toBe("retry");
    expect(s.body).toMatch(/can'?t|couldn'?t/i);
  });

  it("lets the payload gate only the states that would press it into service", () => {
    // no_titles is fixed in the Library, not by the setup command, so a
    // missing payload must not hijack that screen.
    const s = shown({
      report: report({
        state: "no_titles",
        payload: { present: "no", dir: "C:\\app", missing: ["cli/dml"] },
        probes: { wsl: "yes", distro: "yes", cli: "yes", cli_version: "3.0.0", titles: 0 },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("no-titles");
  });
});

describe("firstRunState — state 3: CLI but no titles", () => {
  it("points at the Library", () => {
    const s = shown({
      report: report({
        state: "no_titles",
        probes: { wsl: "yes", distro: "yes", cli: "yes", cli_version: "3.0.0", titles: 0 },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("no-titles");
    expect(s.action.kind).toBe("nav");
    expect(s.action.kind === "nav" && s.action.page).toBe("library");
  });
});

describe("firstRunState — could not tell", () => {
  const steps = ["wsl", "distro", "cli", "titles"] as const;

  it.each(steps)("reports an unanswered %s probe as unknown with a retry, never as a diagnosis", (step) => {
    const s = shown({
      report: report({
        state: "unknown",
        blocked_at: step,
        probes: { wsl: "unknown", distro: "unknown", cli: "unknown", cli_version: null, titles: null },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("unknown");
    expect(s.action.kind).toBe("retry");
    // The rule: say the launcher could not check...
    expect(s.body).toMatch(/can'?t|couldn'?t/i);
    // ...and never claim absence.
    expect(s.body).not.toMatch(/\b(not|isn't|aren't) installed\b/i);
    expect(s.body).not.toMatch(/\bnothing (is )?installed\b/i);
  });

  it("says WHY it could not check, by naming the step that went dark", () => {
    const atCli = shown({
      report: report({ state: "unknown", blocked_at: "cli", distro: "dml-arch" }),
      everReady: false,
    });
    const atWsl = shown({ report: report({ state: "unknown", blocked_at: "wsl" }), everReady: false });
    expect(atCli.body).not.toBe(atWsl.body);
    expect(atCli.body).toContain("dml-arch");
  });

  it("never offers to install a title off an unknown titles probe", () => {
    // Offering "install your first title" to someone who already has a server
    // is the same lie as the WSL one, in the other direction.
    const s = shown({
      report: report({ state: "unknown", blocked_at: "titles" }),
      everReady: false,
    });
    expect(s.action.kind).not.toBe("nav");
  });

  it("treats a failed probe call itself as could-not-tell, and shows the reason", () => {
    // The IPC never landed, so we know nothing at all about this machine --
    // including whether it is set up.
    const s = shown({ report: null, error: "channel closed", everReady: false });
    expect(s.kind).toBe("unknown");
    expect(s.action.kind).toBe("retry");
    expect(s.detail).toContain("channel closed");
    expect(s.body).not.toMatch(/\b(not|isn't) installed\b/i);
  });

  it("keeps a working Home over a failed probe call once the machine was seen ready", () => {
    expect(firstRunState({ report: null, error: "channel closed", everReady: true })).toBeNull();
  });
});

describe("firstRunState — every screen is renderable", () => {
  const cases: BackendStatusReport[] = [
    report({ state: "no_wsl" }),
    report({ state: "no_distro" }),
    report({ state: "no_cli" }),
    report({ state: "cli_outdated", probes: { wsl: "yes", distro: "yes", cli: "yes", cli_version: "2.6.0", titles: null } }),
    report({ state: "no_titles", probes: { wsl: "yes", distro: "yes", cli: "yes", cli_version: "3.0.0", titles: 0 } }),
    report({ state: "unknown", blocked_at: "wsl" }),
    report({ state: "no_cli", payload: { present: "no", dir: "C:\\app", missing: ["cli/dml"] } }),
    report({ state: "no_cli", payload: { present: "unknown", dir: null, missing: [] } }),
  ];

  it.each(cases)("gives $state a title, one sentence and one labelled button", (r) => {
    const s = shown({ report: r, everReady: false });
    expect(s.title).toBeTruthy();
    expect(s.body).toBeTruthy();
    expect(s.action.label).toBeTruthy();
    expect(typeof s.detail).toBe("string");
  });

  it("reads as calm and finished — no screen shouts an error word", () => {
    // This is the FIRST thing a stranger sees. ServerRequired's precedent:
    // a greeting carrying the fixing action, not a failure report.
    for (const r of cases) {
      const s = shown({ report: r, everReady: false });
      expect(`${s.title} ${s.body}`).not.toMatch(/\b(error|failed|failure|fatal)\b/i);
    }
  });
});

describe("firstRunNeedsProbe", () => {
  it("probes when nothing is known yet", () => {
    expect(firstRunNeedsProbe(null, false)).toBe(true);
  });

  it("keeps probing while the machine is still not set up", () => {
    // The user goes to the Library, installs a title, and comes back to Home:
    // without this the screen would still say "no game server installed".
    expect(firstRunNeedsProbe(report({ state: "no_titles" }), false)).toBe(true);
    expect(firstRunNeedsProbe(report({ state: "no_cli" }), false)).toBe(true);
    expect(firstRunNeedsProbe(report({ state: "unknown", blocked_at: "wsl" }), false)).toBe(true);
  });

  it("stops probing once the machine has been seen ready", () => {
    // Every probe is up to three wsl.exe spawns; re-running them on every
    // Home visit for the rest of the session buys nothing.
    expect(firstRunNeedsProbe(report(), false)).toBe(false);
    expect(firstRunNeedsProbe(null, true)).toBe(false);
  });

  it("stops probing in native mode, which has no distro to ask about", () => {
    expect(firstRunNeedsProbe(report({ state: "no_wsl", backend_mode: "native" }), false)).toBe(false);
  });
});
