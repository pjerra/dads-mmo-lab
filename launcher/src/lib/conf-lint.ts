// Pure sanity check for raw AzerothCore .conf files (improvements Batch 3 F4).
// AC .conf files are flat "Key = Value" with "#" comments -- there's no full
// parser here, just a cheap pass that flags any line which is neither blank, a
// comment, nor a Key = Value assignment. A fat-fingered edit gets caught before
// it's written; the backup/reset safety net still applies either way.
export interface ConfLintIssue {
  line: number; // 1-indexed
  text: string;
}

export function lintConfContent(content: string): ConfLintIssue[] {
  const issues: ConfLintIssue[] = [];
  const lines = content.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim();
    if (trimmed === "" || trimmed.startsWith("#")) continue;
    // INI-style section headers ("[worldserver]", "[authserver]", the AH
    // module's "[worldserver]", ...) are valid AzerothCore .conf syntax, not
    // Key = Value lines -- don't flag them (they open worldserver.conf /
    // authserver.conf / mod_ahbot.conf).
    if (/^\[.*\]$/.test(trimmed)) continue;
    // A valid assignment has a non-empty key before the first '='. eq === -1
    // (no '=') or eq === 0 (empty key) both fail.
    if (trimmed.indexOf("=") <= 0) {
      issues.push({ line: i + 1, text: trimmed });
    }
  }
  return issues;
}
