import { describe, expect, it } from "vitest";
import { applyEvent, initialTermState } from "./terminal-state";

const T0 = 1_000_000;

describe("terminal state reducer", () => {
  it("stamps startedAt on the first event", () => {
    const s = applyEvent(initialTermState(), { event: "section_start", name: "start" }, T0);
    expect(s.startedAt).toBe(T0);
    expect(s.sections).toHaveLength(1);
    expect(s.sections[0]).toMatchObject({ name: "start", status: "running", collapsed: false });
  });

  it("appends lines to the running section and counts them", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" }, T0);
    s = applyEvent(s, { event: "line", level: "info", text: "one" });
    s = applyEvent(s, { event: "line", level: "warn", text: "two" });
    expect(s.sections[0].lines).toEqual([
      { level: "info", text: "one" },
      { level: "warn", text: "two" },
    ]);
    expect(s.totalLines).toBe(2);
  });

  it("creates an implicit output section for orphan lines", () => {
    const s = applyEvent(initialTermState(), { event: "line", level: "info", text: "hello" });
    expect(s.sections[0].name).toBe("output");
    expect(s.sections[0].lines[0].text).toBe("hello");
  });

  it("section_end ok collapses the section", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    s = applyEvent(s, { event: "section_end", name: "start", status: "ok" });
    expect(s.sections[0]).toMatchObject({ status: "ok", collapsed: true });
  });

  it("done finishes the run", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    s = applyEvent(s, { event: "done", data: { id: "wow", state: "running" } });
    expect(s.finished).toEqual({ kind: "done", data: { id: "wow", state: "running" } });
  });

  it("done closes every still-running section, including the implicit output one", () => {
    // The REAL event sequence of a native `games start` with Docker Desktop
    // down, transcribed from native.rs + lifecycle.rs. The engine-progress lines
    // arrive OUTSIDE any section, so the `line` arm fabricates one named
    // "output" -- and nothing in this repo ever emits section_end{name:"output"},
    // so only the terminal event can close it. Before this, the user watched
    // that spinner turn forever next to a server that had already started.
    let s = applyEvent(initialTermState(), {
      event: "line",
      level: "info",
      text: "Docker engine is down. Starting Docker Desktop...",
    });
    s = applyEvent(s, {
      event: "line",
      level: "info",
      text: "Waiting for Docker Desktop to be ready...",
    });
    s = applyEvent(s, { event: "line", level: "info", text: "Docker Desktop engine is ready." });
    s = applyEvent(s, { event: "section_start", name: "start" });
    s = applyEvent(s, { event: "line", level: "info", text: "starting containers..." });
    s = applyEvent(s, { event: "section_end", name: "start", status: "ok" });
    s = applyEvent(s, { event: "done", data: { id: "wow", state: "running" } });

    expect(s.sections.map((x) => x.name)).toEqual(["output", "start"]);
    expect(s.sections.every((x) => x.status !== "running")).toBe(true);
    // The engine lines stay READABLE -- closing the section must not hide the
    // three lines the user was watching.
    expect(s.sections[0]).toMatchObject({ status: "ok", collapsed: false });
    expect(s.sections[0].lines).toHaveLength(3);
  });

  it("done does not overwrite a section that already failed", () => {
    // Closing running sections must not launder a section_end error into "ok".
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    s = applyEvent(s, { event: "section_end", name: "start", status: "error" });
    s = applyEvent(s, { event: "done", data: {} });
    expect(s.sections[0].status).toBe("error");
  });

  it("error finishes the run and fails running sections", () => {
    let s = applyEvent(initialTermState(), { event: "section_start", name: "start" });
    const err = { code: "START_FAILED", message: "boom", hint: "" };
    s = applyEvent(s, { event: "error", error: err });
    expect(s.finished).toEqual({ kind: "error", error: err });
    expect(s.sections[0].status).toBe("error");
  });

  it("ignores unknown events (pct is reserved)", () => {
    const s0 = applyEvent(initialTermState(), { event: "section_start", name: "x" }, T0);
    const s1 = applyEvent(s0, { event: "pct", value: 42 } as never);
    expect(s1.sections).toEqual(s0.sections);
    expect(s1.finished).toBeNull();
  });

  it("never mutates its input", () => {
    const s0 = applyEvent(initialTermState(), { event: "section_start", name: "x" }, T0);
    const frozen = JSON.stringify(s0);
    applyEvent(s0, { event: "line", level: "info", text: "y" });
    applyEvent(s0, { event: "section_end", name: "x", status: "ok" });
    expect(JSON.stringify(s0)).toBe(frozen);
  });
});
