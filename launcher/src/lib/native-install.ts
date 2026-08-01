// Bridging the native install engine into the install terminal.
//
// Two event vocabularies meet here, and neither should change to suit the
// other:
//
//   * `InstallTerminal` speaks `InstallEvent` — `chunk` (raw text) and `exit`
//     (a process code). That is right for what it was built for: the WSL
//     install is a raw pty around an interactive bash script.
//   * The native engine speaks the project's ordinary NDJSON `TermEvent`
//     vocabulary — `section_start` / `line` / `section_end` and a terminal
//     `done` or `error` — because it is not a pty and asks no questions.
//
// Rewriting InstallTerminal to understand both would put a second event model
// inside a component whose whole job is the first one. So the translation lives
// here, as a PURE function plus a thin runner, and is vitest-pinned rather than
// click-tested.
//
// THE CONTRACT THAT MATTERS: a streamed command's promise RESOLVES even when
// the run failed — failure travels as an `error` event. So the exit code is
// derived from the terminal event and NEVER from the promise settling. That
// exact mistake shipped once already, in Home's restart flow, and was caught by
// two independent reviewers.

import { gamesInstallNative, type InstallEvent, type TermEvent } from "./api";
import { noteInstallEvent, setInstallActive } from "./install-progress.svelte";

/** What one engine event becomes on the terminal. */
export interface Translated {
  /** Text to append. Empty when the event is not worth showing. */
  text: string;
  /**
   * The process code to report, or `null` when the run is still going.
   *
   * Set ONLY by `done` (0) and `error` (1) — the two terminal events. Anything
   * else leaving this null is what keeps a mid-run hiccup from ending the
   * panel.
   */
  exit: number | null;
}

const NOTHING: Translated = { text: "", exit: null };

/**
 * One engine event → terminal output.
 *
 * Unknown events return nothing rather than throwing. That is the standing
 * forward-compatibility rule for this union (the reserved `pct` event is the
 * canonical example): a future engine event must never be able to crash a
 * running install's terminal.
 */
export function translateNativeEvent(e: TermEvent): Translated {
  switch (e.event) {
    case "section_start":
      // A blank line before each stage keeps an hours-long build readable.
      return { text: `\n== ${(e as { name: string }).name} ==\n`, exit: null };

    case "line": {
      const { level, text } = e as { level: string; text: string };
      // `info` is the overwhelming majority, so tagging it would be noise;
      // warn/error are the ones a user scrolls back to find.
      return { text: level === "info" ? `${text}\n` : `[${level}] ${text}\n`, exit: null };
    }

    case "section_end": {
      const { name, status } = e as { name: string; status: string };
      // Only failures are worth a line: a stage that succeeded is already
      // implied by the next one starting, and the engine has eight of them.
      return status === "ok" ? NOTHING : { text: `[error] ${name} failed\n`, exit: null };
    }

    case "done":
      return { text: "\nInstall finished.\n", exit: 0 };

    case "error": {
      const err = (e as { error?: { code?: string; message?: string; hint?: string } }).error ?? {};
      // The hint is the actionable half and is frequently the ONLY useful part
      // (INSTALL_STACK_CONFLICT's hint names the other stack; the message just
      // says there is one), so it is never dropped.
      const parts = [err.message ?? "The install failed."];
      if (err.hint) parts.push(err.hint);
      return { text: `\n[error] ${parts.join(" ")}\n`, exit: 1 };
    }

    default:
      return NOTHING;
  }
}

/**
 * An `InstallTerminal` runner backed by the native engine.
 *
 * Shaped to match `gamesInstall` exactly so the terminal, its scrollback, the
 * install slot and the exit handling are all reused unchanged.
 *
 * A rejection here means the IPC itself failed — not that the install failed,
 * which would have arrived as an `error` event. It still has to close the
 * panel, or a dead session leaves every control disabled until an app restart,
 * so it reports `-1`: the same code the WSL path uses for "could not even
 * start".
 */
export function nativeInstallRunner(
  id: string,
  onEvent: (e: InstallEvent) => void,
): Promise<void> {
  let ended = false;
  // Committed BEFORE the first event: the engine's preflight takes a few
  // seconds, and a status chip that reads "Stopped" for those seconds and then
  // jumps is worse than one that commits the moment the work starts. Set HERE
  // rather than in Library's startInstall() so it can never leak onto the WSL
  // install path, which speaks raw pty chunks and would never clear it —
  // pinning the chip to "Installing…" until the app restarts.
  setInstallActive(true);
  const emit = (t: Translated) => {
    if (t.text) onEvent({ event: "chunk", text: t.text });
    if (t.exit !== null) {
      ended = true;
      onEvent({ event: "exit", code: t.exit });
    }
  };
  return gamesInstallNative(id, (e) => {
    // The status store and the terminal read the SAME stream independently:
    // `pct` is invisible in the panel (which shows the raw build wall) and
    // `line` is invisible to the status. Neither translation is the other's
    // input, so neither can distort the other.
    noteInstallEvent(e);
    emit(translateNativeEvent(e));
  }).then(
    () => {
      // A stream that resolved without a terminal event is a contract
      // violation somewhere upstream. Close the panel anyway — silently
      // leaving it "running" forever is strictly worse than an honest -1.
      if (!ended) {
        onEvent({ event: "chunk", text: "\n[error] The install ended without reporting a result.\n" });
        onEvent({ event: "exit", code: -1 });
        setInstallActive(false);
      }
    },
    (err: unknown) => {
      if (ended) return;
      const e = err as { message?: string; hint?: string };
      const msg = `${e?.message ?? String(err)}${e?.hint ? ` — ${e.hint}` : ""}`;
      onEvent({ event: "chunk", text: `\n[error] ${msg}\n` });
      onEvent({ event: "exit", code: -1 });
      // The terminal `done`/`error` events clear this through the reducer; this
      // is the path where NEITHER arrived, so nothing else ever will.
      setInstallActive(false);
    },
  );
}
