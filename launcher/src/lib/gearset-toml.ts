// Gear-set TOML export/import (Batch 4 D): serialize a saved GearSet to a
// TOML text block a user can copy/share, and parse a pasted block back into a
// GearSet. Pure functions (no localStorage/DOM), unit-tested for round-trip.
//
// The parser deliberately reuses parseGearSets() for all validation/hardening:
// it builds a plain object from the TOML and runs it through the SAME
// normalization the localStorage path uses, so an imported set can never be
// less-validated than a natively-saved one (bad/empty items dropped, name
// required and capped, missing fields defaulted).
//
// This is a targeted parser for exactly the schema gearSetToToml emits
// (top-level scalars + `[[items]]` array-of-tables with scalar fields), NOT a
// general TOML implementation -- unsupported value kinds (floats, bools, bare
// words, inline tables/arrays) are ignored rather than trusted.

import { parseGearSets, type GearSet, type GearSetItem } from "$lib/gearsets.svelte";

// --- serialize --------------------------------------------------------------

// TOML basic string: wrap in double quotes, escaping backslash/quote/controls.
function tomlString(s: string): string {
  const esc = s
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\n/g, "\\n")
    .replace(/\r/g, "\\r")
    .replace(/\t/g, "\\t");
  return `"${esc}"`;
}

// Integer literal: trunc guards against any accidental float sneaking in.
function tomlInt(n: number): string {
  return String(Math.trunc(Number.isFinite(n) ? n : 0));
}

export function gearSetToToml(set: GearSet): string {
  const lines: string[] = [];
  lines.push("# DML gear set — paste this whole block into Import on another launcher.");
  lines.push(`name = ${tomlString(set.name)}`);
  lines.push(`sourceChar = ${tomlString(set.sourceChar)}`);
  lines.push(`class = ${tomlInt(set.class)}`);
  lines.push(`level = ${tomlInt(set.level)}`);
  lines.push(`capturedAt = ${tomlInt(set.capturedAt)}`);
  for (const it of set.items) {
    lines.push("");
    lines.push("[[items]]");
    lines.push(`slot = ${tomlInt(it.slot)}`);
    lines.push(`entry = ${tomlInt(it.entry)}`);
    lines.push(`name = ${tomlString(it.name)}`);
    lines.push(`quality = ${tomlInt(it.quality)}`);
  }
  return lines.join("\n") + "\n";
}

// --- parse ------------------------------------------------------------------

// Parse a TOML basic string literal (starts with `"`); returns the decoded
// content, or undefined if unterminated.
function parseTomlBasicString(v: string): string | undefined {
  let out = "";
  let i = 1; // skip opening quote
  while (i < v.length) {
    const c = v[i];
    if (c === "\\") {
      const n = v[i + 1];
      if (n === "n") out += "\n";
      else if (n === "t") out += "\t";
      else if (n === "r") out += "\r";
      else if (n === '"') out += '"';
      else if (n === "\\") out += "\\";
      else out += n ?? "";
      i += 2;
      continue;
    }
    if (c === '"') return out; // closing quote
    out += c;
    i++;
  }
  return undefined; // unterminated -> treated as no value
}

function parseTomlValue(raw: string): string | number | undefined {
  if (raw.startsWith('"')) return parseTomlBasicString(raw);
  if (/^-?\d+$/.test(raw)) {
    const n = parseInt(raw, 10);
    return Number.isSafeInteger(n) ? n : undefined;
  }
  return undefined; // unsupported literal kind -> ignore
}

// Parse a pasted TOML block into a GearSet. Throws a friendly Error when the
// text is empty or doesn't yield a usable set (no name / no valid items).
export function gearSetFromToml(text: string): GearSet {
  if (typeof text !== "string" || !text.trim()) {
    throw new Error("Paste the gear-set text first.");
  }
  const top: Record<string, unknown> = {};
  const items: Record<string, unknown>[] = [];
  let ctx: Record<string, unknown> = top;

  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    if (line === "[[items]]") {
      ctx = {};
      items.push(ctx);
      continue;
    }
    if (line.startsWith("[")) {
      // Any other table header (foreign/unknown): point the active context at
      // a throwaway sink so keys under it are discarded, NOT silently merged
      // into the previous [[items]] table (which would overwrite that item).
      ctx = {};
      continue;
    }
    const eq = line.indexOf("=");
    if (eq < 0) continue;
    const key = line.slice(0, eq).trim();
    if (!key) continue;
    const value = parseTomlValue(line.slice(eq + 1).trim());
    if (value !== undefined) ctx[key] = value;
  }

  // Reuse the localStorage-path hardening: build the same object shape and let
  // parseGearSets validate/normalize/drop. `items` last so a stray top-level
  // `items` scalar can't shadow the real array-of-tables.
  const obj: Record<string, unknown> = { ...top, items };
  const sets = parseGearSets(JSON.stringify([obj]));
  // parseGearSets accepts a named set with zero items; an empty gear set is
  // useless here, so require at least one surviving item too.
  if (sets.length === 0 || sets[0].items.length === 0) {
    throw new Error("That doesn't look like a gear set (missing a name or no valid items).");
  }
  return sets[0];
}

// Re-export the item type for callers that want it alongside these helpers.
export type { GearSet, GearSetItem };
