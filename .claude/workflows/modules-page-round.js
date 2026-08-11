export const meta = {
  name: 'modules-page-round',
  description: 'Execute the 5-task Modules-page round plan in the shared worktree, one agent per task, sequential',
  whenToUse: 'Run/resume the 2026-08-11 Modules-page round (plan: docs/superpowers/plans/2026-08-11-modules-page-round.md)',
  phases: [
    { title: 'Task 1: auto-conf on install', detail: 'both surfaces + tests', model: 'opus' },
    { title: 'Task 2: update honesty', detail: '4 audit fixes + Rust pull tests', model: 'opus' },
    { title: 'Task 3: layout split', detail: 'installed-first + collapsible catalogs' },
    { title: 'Task 4: click-to-open', detail: 'nav store + auto-conf catch-up' },
    { title: 'Task 5: needs-setup', detail: 'setup catalog + chip/panel' },
  ],
}

// Sequential BY DESIGN: all five agents share ONE worktree; Tasks 1-2 edit the
// same Rust/bash files, Tasks 3-5 edit the same Svelte files, and two agents
// committing concurrently in one worktree fight over the git index. Do not
// parallelize this script.
const WORKTREE = (args && args.worktree) || 'C:/Users/perzi/dml-desks/modules-round'
const PLAN = 'docs/superpowers/plans/2026-08-11-modules-page-round.md'

const TASKS = [
  { n: 1, title: 'Task 1: auto-conf on install', model: 'opus',
    commit: 'feat(modules): install activates the module conf itself (both surfaces)' },
  { n: 2, title: 'Task 2: update honesty', model: 'opus',
    commit: 'fix(modules): update honesty -- advisory, pending_rebuild, staged-edit patch + Rust pull tests' },
  { n: 3, title: 'Task 3: layout split', model: 'sonnet',
    commit: 'feat(launcher): modules page splits installed-first with collapsible catalogs' },
  { n: 4, title: 'Task 4: click-to-open', model: 'sonnet',
    commit: 'feat(launcher): click-to-open module tuner/conf + auto-conf catch-up' },
  { n: 5, title: 'Task 5: needs-setup', model: 'sonnet',
    commit: 'feat(launcher): catalog-driven needs-setup notices with guided actions' },
]

const REPORT = {
  type: 'object',
  properties: {
    status: { enum: ['DONE', 'SKIPPED_ALREADY_DONE', 'DONE_WITH_CONCERNS', 'BLOCKED'] },
    commit: { type: 'string' },
    tests: { type: 'string' },
    mutations: { type: 'string' },
    deviations: { type: 'string' },
  },
  required: ['status', 'tests', 'deviations'],
}

const results = []
for (const t of TASKS) {
  phase(t.title)
  const r = await agent(
    `You are implementing ONE task of a written plan. Work ONLY in the git worktree at ${WORKTREE} (branch feat/modules-page-round) — never in the main checkout.

FIRST: run \`git -C ${WORKTREE} log --oneline -30\` and if a commit subject equal to "${t.commit}" already exists, return status SKIPPED_ALREADY_DONE immediately (another session already ran this task).

Then read ${WORKTREE}/${PLAN} — the "Global Constraints" section and **Task ${t.n} only** — and execute Task ${t.n}'s steps EXACTLY in order: failing test first, watch it fail, implement, watch it pass, run the named mutation proofs and watch each go RED then restore via the Edit tool (never git checkout, never bash heredocs for Rust literals). Read the referenced files before editing them; the plan's line numbers are approximate anchors, not gospel.

Rules that override convenience: bats runs only via wsl -d dml-arch -u dml --exec bash -lc with output redirected to a file (read counts from the file); never run bats and cargo tests at the same time; after editing cli/src/*.sh run bash cli/build.sh and commit the regenerated cli/dml; vitest/svelte-check run from ${WORKTREE}/launcher.

Finish by committing in the worktree with EXACTLY this subject: "${t.commit}" plus body lines for what changed, ending with:
Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>

Report honestly: test counts before/after, each mutation's RED evidence, and every deviation from the plan (a defect you found in the plan is worth more than silent compliance). If you cannot complete a step, commit nothing for it and return BLOCKED with the reason.`,
    { label: `task-${t.n}`, phase: t.title, model: t.model, schema: REPORT },
  )
  results.push({ task: t.n, report: r })
  if (!r) { log(`Task ${t.n}: agent died — stopping the chain (later tasks depend on this one).`); break }
  log(`Task ${t.n}: ${r.status} — ${r.tests}`)
  if (r.status === 'BLOCKED') { log(`Task ${t.n} BLOCKED: ${r.deviations} — stopping the chain.`); break }
}
return results
