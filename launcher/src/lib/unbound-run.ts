// Bridging the Wrath Unbound engine into the install terminal.
//
// Reuses `translateNativeEvent` for every event whose rendering is identical
// (section_start / line / section_end / error) and overrides exactly one:
// `done`.
//
// WHY `done` CANNOT BE SHARED. The generic translator renders it as the single
// line "Install finished." — correct for a title install, and actively wrong
// here, because this engine's done payload is the half of the result the user
// most needs:
//
//   * INSTALL carries `manual_step`. Spawning the Mentor NPC is the one thing
//     the engine deliberately does not do (it needs a GM in-game, and a fresh
//     server may have no GM account at all). Swallowing it leaves a user with
//     a rebuilt server, no Mentor, and nothing on screen explaining why —
//     which is precisely the bash banner's lie the port exists to remove.
//   * UNINSTALL carries `residue[]`: everything NOT reverted (mod-ale kept,
//     spells until next login, Mentor Stones in bags) plus any per-statement
//     failure. An "honest inverse" that prints "Finished." is not honest.
//
// Both also carry `backup`, which is what a user reaches for when they want to
// undo the whole thing.

import { translateNativeEvent, type Translated } from "./native-install";
import {
  wowUnboundInstall,
  wowUnboundUninstall,
  type InstallEvent,
  type TermEvent,
} from "./api";

export type UnboundMode = "install" | "uninstall";

/** Shape of the engine's terminal `done` payload, as far as this file reads it. */
interface UnboundDone {
  addon_version?: string;
  backup?: string | null;
  residue?: string[];
  manual_step?: string;
  mentor_stone_cleanup_sql?: string;
}

/**
 * One engine event → terminal output, with the `done` payload rendered.
 *
 * Everything except `done` delegates, so the two engines cannot drift in how
 * they show a stage, a warning or a failure.
 */
export function translateUnboundEvent(e: TermEvent, mode: UnboundMode): Translated {
  if (e.event !== "done") {
    return translateNativeEvent(e);
  }
  const d = ((e as { data?: UnboundDone }).data ?? {}) as UnboundDone;
  const out: string[] = [
    "",
    mode === "install"
      ? `Wrath Unbound ${d.addon_version ?? ""} installed.`.replace("  ", " ")
      : "Wrath Unbound removed.",
  ];

  if (d.backup) {
    out.push(`Safety backup: ${d.backup} (in ~/.dml/backups)`);
  }

  if (mode === "install" && d.manual_step) {
    out.push("");
    out.push("ONE STEP LEFT, and it needs you in-game as a GM:");
    out.push(`  ${d.manual_step}`);
  }

  if (mode === "uninstall") {
    const residue = d.residue ?? [];
    out.push("");
    if (residue.length === 0) {
      // The engine always reports the permanent classes, so an EMPTY array
      // means the engine did not report — say that rather than implying a
      // perfect reversal nobody verified.
      out.push("The engine reported no residue list.");
    } else {
      out.push("Not reverted (by design or by failure):");
      for (const r of residue) out.push(`  - ${r}`);
    }
    if (d.mentor_stone_cleanup_sql) {
      out.push("");
      out.push("To clear Mentor Stones from character bags, run this yourself:");
      out.push(`  ${d.mentor_stone_cleanup_sql}`);
    }
  }

  return { text: `${out.join("\n")}\n`, exit: 0 };
}

/**
 * An `InstallTerminal` runner backed by the Unbound engine.
 *
 * Same contract discipline as `nativeInstallRunner`: the exit code comes from
 * the terminal EVENT, never from the promise settling, because a streamed
 * command resolves Ok even when the run failed.
 */
export function unboundRunner(
  mode: UnboundMode,
  acceptDataChanges: boolean,
  opts?: { repair?: boolean; force?: boolean },
): (id: string, onEvent: (e: InstallEvent) => void) => Promise<void> {
  return (_id, onEvent) => {
    let ended = false;
    const emit = (t: Translated) => {
      if (t.text) onEvent({ event: "chunk", text: t.text });
      if (t.exit !== null) {
        ended = true;
        onEvent({ event: "exit", code: t.exit });
      }
    };
    const handle = (e: TermEvent) => emit(translateUnboundEvent(e, mode));
    const run =
      mode === "install"
        ? wowUnboundInstall(acceptDataChanges, handle, opts?.repair)
        : wowUnboundUninstall(acceptDataChanges, handle, opts?.force);

    return run.then(
      () => {
        if (!ended) {
          onEvent({
            event: "chunk",
            text: "\n[error] The run ended without reporting a result.\n",
          });
          onEvent({ event: "exit", code: -1 });
        }
      },
      (err: unknown) => {
        // A rejection is the IPC failing, not the run failing — a failed run
        // arrives as an `error` event. It still has to close the panel, or
        // every control stays disabled until the app restarts.
        if (ended) return;
        const e = err as { message?: string; hint?: string };
        const parts = [e.message ?? String(err)];
        if (e.hint) parts.push(e.hint);
        onEvent({ event: "chunk", text: `\n[error] ${parts.join(" ")}\n` });
        onEvent({ event: "exit", code: -1 });
      },
    );
  };
}
