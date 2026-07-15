import type { DmlErr, TermEvent } from "./api";

export interface TermLine {
  level: string;
  text: string;
}

export interface Section {
  name: string;
  lines: TermLine[];
  status: "running" | "ok" | "error";
  collapsed: boolean;
}

export interface TermState {
  sections: Section[];
  startedAt: number | null;
  finished: null | { kind: "done"; data: unknown } | { kind: "error"; error: DmlErr };
  totalLines: number;
}

export function initialTermState(): TermState {
  return { sections: [], startedAt: null, finished: null, totalLines: 0 };
}

export function applyEvent(s: TermState, e: TermEvent, now: number = Date.now()): TermState {
  const st: TermState = {
    ...s,
    sections: s.sections.map((sec) => ({ ...sec, lines: sec.lines })),
    startedAt: s.startedAt ?? now,
  };

  switch (e.event) {
    case "section_start":
      st.sections = [
        ...st.sections,
        { name: String(e.name), lines: [], status: "running", collapsed: false },
      ];
      break;

    case "line": {
      let cur = st.sections[st.sections.length - 1];
      if (!cur || cur.status !== "running") {
        cur = { name: "output", lines: [], status: "running", collapsed: false };
        st.sections = [...st.sections, cur];
      }
      cur.lines = [...cur.lines, { level: String(e.level), text: String(e.text) }];
      st.totalLines += 1;
      break;
    }

    case "section_end":
      st.sections = st.sections.map((sec) =>
        sec.name === e.name && sec.status === "running"
          ? { ...sec, status: e.status === "ok" ? "ok" : "error", collapsed: e.status === "ok" }
          : sec,
      );
      break;

    case "done":
      st.finished = { kind: "done", data: (e as { data: unknown }).data };
      break;

    case "error": {
      st.finished = { kind: "error", error: (e as { error: DmlErr }).error };
      st.sections = st.sections.map((sec) =>
        sec.status === "running" ? { ...sec, status: "error" } : sec,
      );
      break;
    }

    default:
      // Unknown events (e.g. reserved "pct") are intentionally ignored.
      break;
  }
  return st;
}
