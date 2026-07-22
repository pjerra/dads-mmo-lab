// Bot World all-keys browser logic (Batch 1 F2), kept pure for unit tests.
// The Config page's "Bot World" tab feeds `wow config pb-keys` rows through
// these: a case-insensitive key search and a staged-edits diff that becomes
// one `config set conf:playerbots.conf:<Key>` call per changed key.
//
// The Module-tuning rework generalized both helpers to every module conf --
// the real implementations now live in conf-keys.ts; these are thin aliases
// that keep the Bot World tab's (and this module's tests') names stable.

import { filterConfKeys, stagedConfChanges } from "./conf-keys";

export interface PbKeyRow {
  key: string;
  value: string;
  default: string | null;
  line: number;
}

// Case-insensitive substring match on the KEY. An empty/whitespace query
// returns the full list unchanged.
export const filterPbKeys = filterConfKeys;

// Staged edits -> the writes Save will perform. Only keys that exist in the
// parsed list AND whose edit differs from the current value count; edit
// order is preserved so saves run in the order the user typed them.
export const stagedPbChanges = stagedConfChanges;
