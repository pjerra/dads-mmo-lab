export function dirtyKeys(
  settings: { key: string; value: string }[],
  edits: Record<string, string>,
): string[] {
  return settings
    .filter((s) => edits[s.key] !== undefined && edits[s.key] !== s.value)
    .map((s) => s.key);
}
