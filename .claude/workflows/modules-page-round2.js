export const meta = {
  name: 'modules-page-round2',
  description: 'Execute the 5-task Modules-page round-2 plan (click-through feedback) in the shared worktree, sequential',
  whenToUse: 'Run/resume the 2026-08-12 Modules-page round 2 (plan: docs/superpowers/plans/2026-08-12-modules-page-round2.md)',
  phases: [
    { title: 'Task 1: row skeleton', detail: 'one shared row + aligned action column' },
    { title: 'Task 2: chips', detail: 'chip-as-action, silent success, failure chip', model: 'sonnet' },
    { title: 'Task 3: config access', detail: 'tuning action + open-config buttons', model: 'sonnet' },
    { title: 'Task 4: disable toggle', detail: 'registry master switches' },
    { title: 'Task 5: review', detail: 'spec review + fix wave' },
  ],
}

// Sequential BY DESIGN: one shared worktree; Tasks 1-4 all edit ModuleManager.svelte.
const WORKTREE = (args && args.worktree) || 'C:/Users/perzi/dml-desks/modules-round'
const PLAN = 'docs/superpowers/plans/2026-08-12-modules-page-round2.md'

const TASKS = [
  { n: 1, title: 'Task 1: row skeleton', model: null,
    commit: 'feat(launcher): modules rows share one skeleton with aligned action column' },
  { n: 2, title: 'Task 2: chips', model: 'sonnet',
    commit: 'feat(launcher): setup chip is the action; conf activation silent on success, chip on failure' },
  { n: 3, title: 'Task 3: config access', model: 'sonnet',
    commit: 'feat(launcher): config-tuning action on rows; open-config buttons on Tuning tab' },
  { n: 4, title: 'Task 4: disable toggle', model: null,
    commit: 'feat(launcher): per-module enable/disable toggle from tuning-registry master switches' },
  { n: 5, title: 'Task 5: review', model: null, effort: 'high',
    commit: 'fix(launcher): round-2 review fixes' },
]

const REPORT = {
  type: 'object',
  properties: {
    status: { enum: ['DONE', 'SKIPPED_ALREADY_DONE', 'DONE_WITH_CONCERNS', 'BLOCKED'] },
    commit: { type: 'string' },
    tests: { type: 'string', description: 'vitest + svelte-check numbers before/after, measured not assumed' },
    deviations: { type: 'string', description: 'every departure from the plan text, or "none"' },
  },
  required: ['status', 'tests', 'deviations'],
}

const results = []
for (const t of TASKS) {
  phase(t.title)
  const opts = { label: `task-${t.n}`, phase: t.title, schema: REPORT }
  if (t.model) opts.model = t.model
  if (t.effort) opts.effort = t.effort
  const r = await agent(
    `You are implementing ONE task of a written plan. Work ONLY in the git worktree at ${WORKTREE} (branch feat/modules-page-round) — never in the main checkout. Verify the branch with git -C ${WORKTREE} branch --show-current before touching anything.

Plan: ${WORKTREE}/${PLAN} — read the WHOLE plan (Global Constraints included), then execute exactly "${t.title.replace(/Task \d+: /, 'Task ' + t.n + ': ')}" — every checkbox step including tests-first, the verification steps, and the commit.

IDEMPOTENCY FIRST: if git -C ${WORKTREE} log --oneline -30 already shows a commit starting with "${t.commit}", change NOTHING and return status SKIPPED_ALREADY_DONE with that hash.

Hard rules:
- Launcher-only: your diff may touch nothing outside launcher/src/ and the plan's listed doc file (Task 5 only).
- Never touch the network, ssh, or any path outside the worktree.
- Run vitest from ${WORKTREE}/launcher with: npx vitest run (read the real summary; never a piped tail). Run npm run check there too. Both must be clean before you commit.
- Commit message starts EXACTLY with "${t.commit}" and ends with:
Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>

Your final output is consumed by a script — return only the structured result.`,
    opts
  )
  results.push(r)
  log(`Task ${t.n}: ${r ? r.status : 'agent lost'} ${r && r.commit ? '(' + String(r.commit).slice(0, 30) + ')' : ''}`)
  if (!r || r.status === 'BLOCKED') {
    log(`Stopping: task ${t.n} did not complete — later tasks depend on it.`)
    break
  }
}
return { results }
