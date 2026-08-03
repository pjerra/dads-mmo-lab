// What the Library's "Bring a server across" card says, and whether its button
// does anything.
//
// Pure on purpose, like title-install.ts and unbound-badge.ts: the page fetches,
// this decides. The whole value of the card is that it answers three questions
// BEFORE the click — is there an export here, is it complete, and would this
// button import or resume — because every one of them used to be answerable
// only by starting a multi-gigabyte operation and watching it fail.

import type { MigrateStatus } from "./api";

export type MigrateAction = "import" | "resume" | "blocked";

export interface MigrateCard {
  /** Button label. */
  label: string;
  action: MigrateAction;
  /** One-line summary above the button. */
  body: string;
  /** Longer detail, or "" when there is nothing useful to add. */
  detail: string;
  /** The button does nothing useful — render it disabled. */
  disabled: boolean;
}

/**
 * `null` status means "not asked yet, or the probe failed".
 *
 * Rendered as a blocked card that says so, NEVER as "no export found": those
 * are different facts, and claiming the second when we only know the first is
 * how a user gets sent to re-run a 40-minute export they already have.
 */
export function migrateCard(status: MigrateStatus | null | undefined): MigrateCard {
  if (!status) {
    return {
      label: "Import",
      action: "blocked",
      body: "Couldn't check the folder for an export.",
      detail: "",
      disabled: true,
    };
  }

  // A previous run that got somewhere outranks a payload check. The engine
  // continues from `next_stage`, and after `load-images` the tarballs may well
  // have been consumed — so "your export is incomplete" would be both wrong and
  // alarming about a migration that is most of the way done.
  if (status.state_present && status.next_stage) {
    return {
      label: "Resume import",
      action: "resume",
      body: `A previous import stopped at "${status.next_stage}".`,
      detail: status.last_error
        ? `It stopped with: ${status.last_error}. Resuming continues from that stage — the work already done is not repeated.`
        : "Resuming continues from that stage — the work already done is not repeated.",
      disabled: false,
    };
  }

  if (!status.export_present) {
    const missing = status.missing.length ? status.missing.join(", ") : "everything";
    return {
      label: "Import",
      action: "blocked",
      body: "No complete export in this folder yet.",
      detail:
        `Missing: ${missing}. Run export-from-wsl.sh inside the distro — it writes its payload into ` +
        `${status.title_dir}, and the folder name has to stay as it is, because that name is the id ` +
        `every other part of the launcher looks this server up by.`,
      disabled: true,
    };
  }

  return {
    label: "Import",
    action: "import",
    body: "An export is ready to import.",
    // Two things a user needs to know before pressing, and neither is
    // discoverable afterwards. The snapshot point is the one people get wrong:
    // the old server keeps running and keeps diverging, so a week of play on it
    // is a week that does not come across.
    detail:
      "This restores the databases, the game data and the server's own settings. It refuses if the " +
      "target already has characters. The import is a snapshot: anything you do on the old server " +
      "afterwards does not come across.",
    disabled: false,
  };
}

/**
 * Turn a refusal code into copy that names the way out.
 *
 * The two emptiness refusals are NOT collapsed. They mean different things and
 * need opposite actions — one is "you pointed at somebody's server", the other
 * is "the database did not answer" — and a shared message would send half the
 * users to do the wrong thing.
 */
export function migrateErrorHint(code: string): string {
  switch (code) {
    case "MIGRATE_TARGET_NOT_EMPTY":
      return "That server already has characters on it. Import into an empty folder instead — or, if you meant to replace this server, use Backups → Restore, which takes a safety copy first.";
    case "MIGRATE_TARGET_UNKNOWN":
      return "The database didn't answer, so the import stopped rather than guess. Nothing was written. Check the Console for the database container, then try again.";
    case "MIGRATE_NO_OVERRIDE":
      return "The export is missing the source server's own settings file, and importing without it would build a server that looks healthy and isn't yours. Re-run the export.";
    case "MIGRATE_INCOMPLETE_EXPORT":
      return "Some of the export is missing. Re-run export-from-wsl.sh inside the distro.";
    case "MIGRATE_COMPOSE_EXISTS":
      return "That folder already holds a server this launcher didn't create. Import into a different folder.";
    case "MIGRATE_STACK_CONFLICT":
      return "Another AzerothCore stack owns the container names. Only one can exist at a time — stop it first.";
    case "MIGRATE_ENGINE_DOWN":
      return "Docker didn't answer. Start Docker Desktop and try again.";
    case "MIGRATE_READY_TIMEOUT":
      return "Everything imported, but the world server hasn't reported ready yet. The containers are still running — check the Console.";
    default:
      return "";
  }
}
