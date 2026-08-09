// can_build is additive (2026-08-09): an older CLI omits it, and a missing
// answer must never disable the rebuild button on a server that can build —
// the authoritative refusal lives in the CLI arm. Fail OPEN, exactly like
// normalizeCatalog's install_supported.
export function canBuild(list: { can_build?: boolean } | null): boolean {
  return list?.can_build !== false;
}
