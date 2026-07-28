// The one-line content summary under a backup row.
//
// Pulled out of the Backups.svelte template purely so the pluralization is
// vitest-pinned -- the counts come straight off the `.meta` sidecar, whose
// exact key order/format is byte-pinned server-side (crates/dml-wow backup.rs
// format_summary_line / valid_summary_line), so a display bug like "1 bots"
// must never be tempted to be "fixed" by touching the emitted line.

import type { BackupSummary } from "./api";

/** e.g. "3 characters · 2 accounts · 1 bot". */
export function backupSummaryLine(s: BackupSummary): string {
  const parts = [
    `${s.characters} character${s.characters === 1 ? "" : "s"}`,
    `${s.accounts} account${s.accounts === 1 ? "" : "s"}`,
  ];
  // Truthiness, not `!== null`, on purpose: `bots` is null when the playerbots
  // schema was unreadable at backup time and 0 when there simply were none --
  // neither is worth a clause, so both stay silent.
  if (s.bots) parts.push(`${s.bots} bot${s.bots === 1 ? "" : "s"}`);
  return parts.join(" · ");
}
