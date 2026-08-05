export type ExitPrompt = 'prompt_running' | 'prompt_unknown';

export interface ExitCopy {
  title: string;
  body: string;
  confirm: string;
  cancel: string;
}

/** Pure, so the wording is testable without mounting anything. */
export function exitCopy(kind: ExitPrompt): ExitCopy {
  const confirm = 'Stop server and close';
  const cancel = 'Cancel';
  if (kind === 'prompt_running') {
    return {
      title: 'Your server is running',
      body: 'Closing DML Launcher will stop it. Windows shuts the WSL distro down shortly after the launcher exits, so the server cannot keep running without it.',
      confirm,
      cancel
    };
  }
  return {
    title: 'Your server may be running',
    body: "Couldn't confirm whether your server is running. Closing DML Launcher may stop it, so it will be stopped cleanly first.",
    confirm,
    cancel
  };
}

/** Module-level so it survives navigation, mirroring restart-state.svelte.ts. */
export const exitGuard = $state<{ open: boolean; kind: ExitPrompt; busy: boolean; note: string }>({
  open: false,
  kind: 'prompt_running',
  busy: false,
  note: ''
});
