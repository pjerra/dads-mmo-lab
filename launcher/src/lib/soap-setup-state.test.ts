import { describe, it, expect, beforeEach } from "vitest";
import {
  soapSetupState,
  applyAutosetupOutcome,
  clearSoapSetup,
  dismissSoapSetup,
  showSoapSetup,
} from "./soap-setup-state.svelte";

// What the shell renders the manual card on. Stated once here so these tests
// pin the actual visibility rule rather than each asserting on whichever flag
// they happen to touch.
const cardVisible = () => soapSetupState.needed && !soapSetupState.dismissed;

beforeEach(() => {
  clearSoapSetup();
});

describe("applyAutosetupOutcome", () => {
  it("a created account announces itself and never raises the manual card", () => {
    const settled = applyAutosetupOutcome({ status: "created", user: "dmlsoap", reason: null });
    expect(settled).toBe(true);
    expect(soapSetupState.autoResult).toEqual({ user: "dmlsoap" });
    // THE point of the whole change: success must not also show the card the
    // user was never meant to see.
    expect(soapSetupState.needed).toBe(false);
  });

  it("only a give-up raises the manual card", () => {
    const settled = applyAutosetupOutcome({
      status: "gave_up",
      user: null,
      reason: "Both names exist.",
    });
    expect(settled).toBe(true);
    expect(soapSetupState.needed).toBe(true);
    expect(soapSetupState.gaveUpReason).toBe("Both names exist.");
    expect(soapSetupState.autoResult).toBeNull();
  });

  it("pending settles nothing, so the next poll tries again", () => {
    expect(applyAutosetupOutcome({ status: "pending", user: null, reason: null })).toBe(false);
    expect(soapSetupState.needed).toBe(false);
    expect(soapSetupState.autoResult).toBeNull();
  });

  it("not_needed is silent and shows nothing at all", () => {
    expect(applyAutosetupOutcome({ status: "not_needed", user: null, reason: null })).toBe(false);
    expect(soapSetupState.needed).toBe(false);
    expect(soapSetupState.autoResult).toBeNull();
  });

  it("a concluded run re-derives its verdict, so a reload gets the card back", () => {
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "Both names exist." });
    // A webview reload wipes this module-level store AND the poll's
    // autosetupSettled flag, so the next poll asks the backend again. The
    // second answer used to be a contentless "latched": nothing to rebuild the
    // card from, on the one path where setup had already failed. A concluded
    // run now repeats its real verdict, and the card comes back with its
    // reason.
    clearSoapSetup();
    const settled = applyAutosetupOutcome({
      status: "gave_up",
      user: null,
      reason: "Both names exist.",
    });
    expect(settled).toBe(true);
    expect(soapSetupState.needed).toBe(true);
    expect(soapSetupState.gaveUpReason).toBe("Both names exist.");
  });

  it("an unknown status is ignored rather than crashing", () => {
    // Same rule the TermEvent union follows: an older/newer backend must not
    // take the UI down.
    expect(applyAutosetupOutcome({ status: "who-knows", user: null, reason: null })).toBe(false);
    expect(soapSetupState.needed).toBe(false);
  });

  it("clearSoapSetup wipes the banner as well as the card", () => {
    applyAutosetupOutcome({ status: "created", user: "dmlsoap", reason: null });
    clearSoapSetup();
    expect(soapSetupState.autoResult).toBeNull();
    expect(soapSetupState.needed).toBe(false);
  });
});

describe('"Later" hides the card without resolving it', () => {
  it("dismissing hides the card but leaves the problem standing", () => {
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "Both names exist." });
    dismissSoapSetup();
    expect(cardVisible()).toBe(false);
    // The half that was broken: "Later" used to run clearSoapSetup(), and the
    // only thing that can raise `needed` again is a gave_up the autosetupSettled
    // latch has already made unreachable. Dropping it here is a one-way door out
    // of a server whose GM Tools, My Party and console are all dead.
    expect(soapSetupState.needed).toBe(true);
    expect(soapSetupState.gaveUpReason).toBe("Both names exist.");
  });

  it("a re-derived gave_up does NOT undo the dismissal", () => {
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "Both names exist." });
    dismissSoapSetup();
    // Same verdict restated (a second poll, a re-derive after the latch). It is
    // not new information, so a card the user just hid must stay hidden --
    // otherwise "Later" visibly undoes itself seconds later.
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "Both names exist." });
    expect(cardVisible()).toBe(false);
  });

  it("the way back brings the same card back", () => {
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "Both names exist." });
    dismissSoapSetup();
    showSoapSetup();
    expect(cardVisible()).toBe(true);
    // With its reason intact -- the route back has to land on the step the user
    // left, not on a bare form with no explanation of why it is there.
    expect(soapSetupState.gaveUpReason).toBe("Both names exist.");
  });

  it("un-dismissing cannot conjure a card nobody asked for", () => {
    // Home offers the control only while `needed`, but the flag must be inert
    // on its own regardless: the shell's gate is what decides, and a stale
    // dismissal must never be the thing that shows a resolved step.
    showSoapSetup();
    expect(cardVisible()).toBe(false);
  });

  it("resolving clears a stale dismissal, so a later failure can show itself", () => {
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "Both names exist." });
    dismissSoapSetup();
    clearSoapSetup();
    expect(soapSetupState.dismissed).toBe(false);
    applyAutosetupOutcome({ status: "gave_up", user: null, reason: "SOAP rejected the new account" });
    expect(cardVisible()).toBe(true);
  });
});
