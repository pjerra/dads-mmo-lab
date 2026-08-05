/**
 * Shared helpers for the tests that scan component source.
 *
 * A handful of invariants in this app live in WIRING rather than in logic: a
 * listener registered against the right event name, a button pointing at the
 * right handler, a call site passing the argument it must not default. Pure
 * unit tests structurally cannot see any of it — the pure halves stay green
 * under a full revert of the production wiring, which is exactly how the
 * "Rust prevents the exit, no dialog appears" regression shipped.
 *
 * Three scanners now exist (soap-surface, status-surface, exit-surface) and
 * they all need the same two things, so those two things live here once.
 */

/**
 * Working-tree files are CRLF (`.gitattributes` has no `*.svelte`/`*.ts` rule
 * and core.autocrlf is on) while committed blobs are LF. Any pattern anchored
 * on a line boundary therefore behaves differently for the same file depending
 * on how it reached the disk, so normalise before matching anything.
 */
export function normalizeEol(src: string): string {
  return src.replace(/\r\n/g, "\n");
}

/**
 * Strip comments before matching.
 *
 * This repo was bitten TWICE on 2026-08-01 by source scans that read an
 * explanation of a thing as the thing itself, and these files are dense with
 * prose that names the very shapes the scans look for. A raw grep would report
 * a removed surface as still present — a red test on correct code, which is
 * how a scan like this gets deleted.
 *
 * The `[^:]` guard on the line-comment rule keeps `https://` intact.
 */
export function code(src: string): string {
  return normalizeEol(src)
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

/**
 * Resolve one source out of an `import.meta.glob(..., { query: "?raw" })` map.
 *
 * THROWS on a miss rather than returning "". Every assertion built on top of a
 * scanner is vacuous if the scanner silently finds nothing, so the miss has to
 * be loud at the point it happens.
 */
export function sourceFinder(sources: Record<string, string>): (suffix: string) => string {
  return (suffix: string) => {
    const hit = Object.entries(sources).find(([f]) => f.endsWith(suffix));
    if (!hit) throw new Error(`no source for ${suffix} — the glob is wrong`);
    return normalizeEol(hit[1]);
  };
}

/** Advance past a quoted string starting at `i`; returns the index after it. */
function skipString(src: string, i: number): number {
  const quote = src[i];
  i++;
  while (i < src.length) {
    if (src[i] === "\\") {
      i += 2;
      continue;
    }
    if (src[i] === quote) return i + 1;
    i++;
  }
  return i;
}

/**
 * The `{ … }` block whose opening brace is the first one at or after `from`.
 *
 * Quoted strings are skipped so a brace inside a message or a template literal
 * cannot unbalance the count. Feed this `code()`-stripped source; it does not
 * know about comments.
 */
export function blockAfter(src: string, from: number): string {
  let open = -1;
  for (let i = from; i < src.length; i++) {
    const c = src[i];
    if (c === '"' || c === "'" || c === "`") {
      i = skipString(src, i) - 1;
      continue;
    }
    if (c === "{") {
      open = i;
      break;
    }
  }
  if (open === -1) throw new Error("no block opens after the anchor");

  let depth = 0;
  for (let i = open; i < src.length; i++) {
    const c = src[i];
    if (c === '"' || c === "'" || c === "`") {
      i = skipString(src, i) - 1;
      continue;
    }
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return src.slice(open + 1, i);
    }
  }
  throw new Error("unbalanced braces after the anchor");
}

/**
 * The body of the block introduced by `anchor` (a function signature, a call
 * head, anything). THROWS when the anchor is absent — see `sourceFinder`.
 */
export function blockOf(src: string, anchor: string): string {
  const at = src.indexOf(anchor);
  if (at === -1) throw new Error(`anchor not found: ${anchor}`);
  return blockAfter(src, at + anchor.length);
}
