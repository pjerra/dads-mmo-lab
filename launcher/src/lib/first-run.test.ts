import { describe, it, expect } from "vitest";
import {
  firstRunState,
  firstRunNeedsProbe,
  firstRunButton,
  humanDetail,
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
    detail: null,
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

  it("does not send the payload-missing user to a repo that has no launcher in it", () => {
    // PROJECT_URL is the UPSTREAM project. It carries Install-DML.ps1 -- which
    // is why the no-wsl/no-distro arms link to it -- but it contains no
    // launcher/ and publishes no release of this app, so "Get a complete
    // release" pointed the one user who cannot fix themselves at a page where
    // the thing they were told to download does not exist.
    const s = shown({
      report: report({
        state: "no_cli",
        payload: { present: "no", dir: "C:\\app", missing: ["cli/dml"] },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("payload-missing");
    expect(s.action.kind).not.toBe("link");
    expect(JSON.stringify(s)).not.toContain(PROJECT_URL);
    // And it must not promise a download that does not exist yet either.
    expect(`${s.title} ${s.body} ${s.action.label}`).not.toMatch(/\brelease\b/i);
  });

  it("tells the payload-missing user something they can actually do, and lets them re-check", () => {
    // The remedy has to be performable with what they have in front of them:
    // the installer they already ran, plus the antivirus that is the usual
    // reason a shipped file is not on disk. Re-checking is the button, because
    // the screen's whole job is to stop being true once the files are back.
    const s = shown({
      report: report({
        state: "cli_outdated",
        payload: { present: "no", dir: "C:\\app", missing: ["cli/lua/gm"] },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("payload-missing");
    expect(s.action.kind).toBe("retry");
    expect(s.body).toMatch(/installer|reinstall/i);
    expect(s.body).toMatch(/antivirus|security software/i);
    // The diagnostics still name what is gone and where we looked -- that is
    // what makes "allow it in your antivirus" actionable.
    expect(s.detail).toContain("cli/lua/gm");
    expect(s.detail).toContain("C:\\app");
  });

  it("names a remedy for BOTH of the causes it claims, not just the quarantine one", () => {
    // The copy names two causes -- a copy moved out of its install folder, and
    // an antivirus that ate the payload -- but offered ONE remedy, and
    // "run the installer again" does not fix the moved copy: reinstalling
    // repairs the ORIGINAL install directory while the exe the user is looking
    // at stays exactly as broken, so Check again fails forever with no hint
    // that WHICH exe they launched is the problem.
    const s = shown({
      report: report({
        state: "no_cli",
        payload: { present: "no", dir: "C:\\Users\\me\\Desktop", missing: ["cli/dml"] },
      }),
      everReady: false,
    });
    expect(s.kind).toBe("payload-missing");
    // The moved-copy case, and what to do about it.
    expect(s.body).toMatch(/moved|copied/i);
    expect(s.body).toMatch(/folder it was installed into|install folder/i);
    // ...without losing the reinstall/antivirus remedy for the other cause.
    expect(s.body).toMatch(/installer|reinstall/i);
    expect(s.body).toMatch(/antivirus|security software/i);
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

  it("shows what wsl.exe actually said, so a dead end is at least reportable", () => {
    // SHIP-LIST Phase 4 review, P9. This is the ONE screen with no repair on
    // it: a non-English Windows, or any wording Microsoft changes, lands here
    // and the Check-again button will never succeed. Without the message
    // neither the user nor a bug report has a single word to go on. The chain
    // now carries it (crates/dml-core/src/setup.rs), and this is the end of
    // that wire.
    const s = shown({
      report: report({
        state: "unknown",
        blocked_at: "cli",
        detail: "The distribution failed to start: Wsl/Service/0x8007019e",
      }),
      everReady: false,
    });
    expect(s.detail).toContain("Wsl/Service/0x8007019e");
  });

  it("prefers the probe's own words over the IPC error when both exist", () => {
    // `error` is set when the invoke itself failed; `detail` is what the
    // machine said. When a report DID land, the machine's words are the
    // specific ones -- "channel closed" describes our plumbing, not their PC.
    const s = shown({
      report: report({ state: "unknown", blocked_at: "wsl", detail: "0x80370102" }),
      error: "some transport hiccup",
      everReady: false,
    });
    expect(s.detail).toBe("0x80370102");
  });

  it("falls all the way down the diagnostics ladder: probe words, then IPC error, then nothing", () => {
    // Replaces a test that asserted only the LAST rung (detail null, no error
    // -> ""). That assertion is satisfied by every candidate fallback chain,
    // including the two wrong ones below, so it read as coverage of the
    // `r.detail ?? o.error ?? ""` fix while pinning none of it:
    //   * `o.error ?? ""`  (drops the probe's words)  -- old test passed;
    //   * `r.detail ?? ""` (drops the IPC error)      -- the WHOLE old file passed.
    // All three rungs together are what discriminate, so all three are here.
    const at = (detail: string | null, error: string | null) =>
      shown({
        report: report({ state: "unknown", blocked_at: "wsl", detail }),
        error,
        everReady: false,
      }).detail;

    // Rung 1: the machine's own words win. They describe the user's PC;
    // "channel closed" describes our plumbing.
    expect(at("0x80370102", "channel closed")).toBe("0x80370102");
    expect(at("0x80370102", null)).toBe("0x80370102");
    // Rung 2: nothing to quote, but the invoke itself failed -- reachable,
    // because a failed re-probe leaves the PREVIOUS report standing next to the
    // new error. That error is then the only fact this run produced, so the one
    // screen with no repair on it must not throw it away.
    expect(at(null, "channel closed")).toBe("channel closed");
    // Rung 3: neither. A missing wsl.exe says nothing, and inventing a sentence
    // for it would be the same guessing the tri-state rule exists to prevent.
    expect(at(null, null)).toBe("");
  });

  it("does not put a raw JSON envelope on the card as the user's diagnostic", () => {
    // The titles step runs `dml games list --json` INSIDE the distro, so when
    // it fails the first line the chain quotes is our own envelope. Verbatim is
    // right for wsl.exe (its complaints are sentences) and wrong here: a user
    // hitting a titles-step shrug was shown `{"ok":false,"error":{...}}` at
    // 12px as the one clue on the ONE screen with no repair on it.
    const s = shown({
      report: report({
        state: "unknown",
        blocked_at: "titles",
        detail:
          '{"ok":false,"error":{"code":"DOCKER_DOWN","message":"Docker is not running","hint":"Start Docker Desktop"}}',
      }),
      everReady: false,
    });
    expect(s.detail).toContain("Docker is not running");
    expect(s.detail).not.toContain('{"ok"');
    expect(s.detail).not.toContain('"code"');
  });
});

describe("humanDetail", () => {
  it("leaves what wsl.exe said exactly as it said it", () => {
    // Every message from the host-side probes is a sentence already, and this
    // is the evidence a bug report is built on.
    expect(humanDetail("Wsl/Service/CreateInstance/0x80370102")).toBe(
      "Wsl/Service/CreateInstance/0x80370102",
    );
    expect(humanDetail("Das Windows-Subsystem für Linux wurde nicht installiert.")).toBe(
      "Das Windows-Subsystem für Linux wurde nicht installiert.",
    );
    expect(humanDetail("")).toBe("");
  });

  it("reads the sentence out of an error envelope, with the code alongside it", () => {
    expect(
      humanDetail('{"ok":false,"error":{"code":"NOT_FOUND","message":"No such game: wow"}}'),
    ).toBe("No such game: wow (NOT_FOUND)");
  });

  it("still finds the message when the quote was cut off at the 200-char bound", () => {
    // `first_diagnostic_line` truncates, so the commonest JSON detail is not
    // valid JSON at all -- which is exactly when a blob is least readable.
    const cut =
      '{"ok":false,"error":{"code":"CLI_BAD_OUTPUT","message":"The backend answered with something this launcher does not underst...';
    const got = humanDetail(cut);
    expect(got).toContain("The backend answered with something");
    expect(got).not.toContain('{"ok"');
  });

  it("says something readable even for JSON with nothing quotable in it", () => {
    const got = humanDetail('{"ok":false,"data":{}}');
    expect(got).not.toContain("{");
    expect(got.length).toBeGreaterThan(0);
  });
});

describe("firstRunButton", () => {
  const retry = { kind: "retry", label: "Check again" } as const;
  const setup = { kind: "setup", label: "Set up backend" } as const;
  const idle = { setupRunning: false, rechecking: false, setupLocked: false };

  it("refuses the second press while a probe is in flight, and says it is working", () => {
    // The screen this protects is the could-not-tell one, whose retry can now
    // cost the full 120 s cold-start budget. Left enabled and unchanged, the
    // user re-clicks, +page.svelte's re-entry guard silently drops each press,
    // and the button reads as broken.
    const b = firstRunButton(retry, { ...idle, rechecking: true });
    expect(b.disabled).toBe(true);
    expect(b.label).not.toBe("Check again");
    expect(b.label).toMatch(/check|work/i);
  });

  it("leaves the retry pressable when nothing is running", () => {
    const b = firstRunButton(retry, idle);
    expect(b).toEqual({ label: "Check again", disabled: false });
  });

  it("still shows the setup button's own busy state", () => {
    const b = firstRunButton(setup, { ...idle, setupRunning: true });
    expect(b).toEqual({ label: "Setting up…", disabled: true });
  });

  it("refuses the setup button underneath the re-probe its own run ends in", () => {
    // The reciprocal of the rule above, and the one that was missing. runSetup
    // clears backendSetupRun.running and then re-probes, so for the WHOLE
    // verification probe -- up to the 140 s cold budget -- the state is
    // setupRunning:false, rechecking:true. The setup arm ignored `rechecking`,
    // so it sat there enabled and labelled "Set up backend" while the launcher
    // was still working out whether the setup had taken. A second press there
    // re-runs the entire backend install on top of the answer being fetched.
    const b = firstRunButton(setup, { ...idle, rechecking: true });
    expect(b.disabled).toBe(true);
    expect(b.label).not.toBe("Set up backend");
    expect(b.label).toMatch(/check|work/i);
  });

  it("disables the setup button when the feature registry locks it", () => {
    const b = firstRunButton(setup, { ...idle, setupLocked: true });
    expect(b).toEqual({ label: "Set up backend", disabled: true });
  });

  it("does not lock a locked feature onto the buttons that are not it", () => {
    // Only `setup` presses backend_setup; a locked registry entry must not
    // disable "Open Library" or the installer link.
    const nav = { kind: "nav", label: "Open Library", page: "library" } as const;
    expect(firstRunButton(nav, { ...idle, setupLocked: true }).disabled).toBe(false);
    expect(firstRunButton(nav, { ...idle, rechecking: true }).disabled).toBe(false);
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
