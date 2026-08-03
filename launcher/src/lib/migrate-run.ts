// Bridging the migration import engine into the install terminal.
//
// Same shape and the same reasoning as `unbound-run.ts`: everything reuses
// `translateNativeEvent`, and only `done` is overridden — because the generic
// arm renders "Install finished." and would swallow the two payloads that are
// the whole point of a migration's ending.
//
// Those two are the verification COUNTS (the number a user compares against the
// server they left behind — the only evidence the import moved what they think
// it moved) and the SNAPSHOT note. The snapshot point is the one people get
// wrong, and it is not discoverable afterwards: the old server keeps running
// and keeps diverging, so a week of play on it is a week that never comes
// across.

import { wowMigrateImport, type InstallEvent, type TermEvent } from "./api";
import { noteInstallEvent, setInstallActive } from "./install-progress.svelte";
import { translateNativeEvent, type Translated } from "./native-install";

/** The migration engine's `done` payload. */
interface MigrateDone {
  id?: string;
  dir?: string;
  project?: string;
  note?: string;
}

/** The `counts` event the db-restore stage emits after a successful restore. */
interface Counts {
  characters?: string;
  accounts?: string;
}

export function translateMigrateEvent(e: TermEvent): Translated {
  if (e.event === "counts") {
    const c = e as unknown as Counts;
    // Shown as its own line rather than folded into `done`, because it arrives
    // an hour earlier — the world still has to boot — and it is the moment the
    // user learns whether their characters are really there.
    return {
      text: `\nRestored: ${c.characters ?? "?"} characters, ${c.accounts ?? "?"} accounts.\n`,
      exit: null,
    };
  }
  if (e.event !== "done") {
    return translateNativeEvent(e);
  }
  const d = ((e as { data?: MigrateDone }).data ?? {}) as MigrateDone;
  const out: string[] = ["", "Migration complete."];
  if (d.dir) out.push(`Server folder: ${d.dir}`);
  if (d.note) {
    out.push("");
    out.push(d.note);
  }
  return { text: `${out.join("\n")}\n`, exit: 0 };
}

/**
 * An `InstallTerminal` runner backed by the migration engine.
 *
 * Same contract discipline as `nativeInstallRunner` and `unboundRunner`: the
 * exit code comes from the terminal EVENT and never from the promise settling,
 * because a streamed command resolves Ok even when the run was refused. A
 * migration that refuses on a non-empty target is exactly such a run, and it is
 * the one the user most needs to see reported as a failure.
 */
export function migrateRunner(): (id: string, onEvent: (e: InstallEvent) => void) => Promise<void> {
  return (id, onEvent) => {
    let ended = false;
    // Committed BEFORE the first event, as the other two runners do: without
    // it the status chip falls through to the polled verdict, which during an
    // import reports a stopped server for the whole run.
    setInstallActive(true);
    const emit = (t: Translated) => {
      if (t.text) onEvent({ event: "chunk", text: t.text });
      if (t.exit !== null) {
        ended = true;
        setInstallActive(false);
        onEvent({ event: "exit", code: t.exit });
      }
    };
    const handle = (e: TermEvent) => {
      noteInstallEvent(e);
      emit(translateMigrateEvent(e));
    };
    return wowMigrateImport(id, handle).then(
      () => {
        if (!ended) {
          // The stream ended with no terminal event. Report a failure rather
          // than a silent success: an import whose outcome we do not know must
          // not look like one that worked.
          setInstallActive(false);
          onEvent({ event: "chunk", text: "\nThe import stopped without reporting a result.\n" });
          onEvent({ event: "exit", code: 1 });
        }
      },
      (err: { code?: string; message?: string; hint?: string }) => {
        setInstallActive(false);
        const msg = `${err?.message ?? String(err)}${err?.hint ? ` — ${err.hint}` : ""}`;
        onEvent({ event: "chunk", text: `\n${err?.code ?? "IPC"}: ${msg}\n` });
        onEvent({ event: "exit", code: 1 });
      },
    );
  };
}
