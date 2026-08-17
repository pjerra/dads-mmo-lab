// Pure: catch-up outcome -> chip (Modules-page round 2, Task 2). The
// auto-conf catch-up in ModuleManager.svelte (the wowModuleConfActivate loop,
// added round 1 Task 4) is silent on success -- `activated` (fresh) and
// `already-active` (a user's own edit already there) both mean "the module
// has a conf, nothing to say". The other three outcomes get a chip on that
// module's row instead of vanishing into a swallowed catch{} the way round 1
// left them: `no-dist` (NEEDS_REBUILD), `no-conf` (NO_CONF), and `error`
// (anything unexpected) are all real failures worth a click-to-explain.

import type { RowChip } from "./module-row";

export type ConfActivationOutcome = "activated" | "already-active" | "no-dist" | "no-conf" | "error";

export function confActivationChip(outcome: ConfActivationOutcome): RowChip | null {
  if (outcome === "activated" || outcome === "already-active") return null;
  return { id: "conf-activation", kind: "conf-failed", label: "conf not activated", clickable: true };
}

// Maps the CmdError.code thrown by wowModuleConfActivate onto the outcome
// above. EXISTS/NEEDS_REBUILD/NO_CONF are the three codes both surfaces emit
// for ConfActivateOutcome::AlreadyActive/NoDistYet/NoConf (see
// crates/dml-wow/src/moduletail.rs's conf_activate and launcher/src-tauri/
// src/lib.rs's wow_module_conf_activate_native, mirrored by 90-main.sh's
// conf-activate arm) -- anything else (including no code at all) is an
// unexpected error, not one of the three known quiet outcomes.
export function outcomeFromErrorCode(code: string | undefined): ConfActivationOutcome {
  if (code === "EXISTS") return "already-active";
  if (code === "NEEDS_REBUILD") return "no-dist";
  if (code === "NO_CONF") return "no-conf";
  return "error";
}
