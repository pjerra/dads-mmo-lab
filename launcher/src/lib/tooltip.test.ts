// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { sanitizeTooltipHtml } from "./tooltip";

describe("sanitizeTooltipHtml", () => {
  it("drops <script> tags AND their text content entirely", () => {
    const out = sanitizeTooltipHtml("<div>before<script>alert(document.cookie)</script>after</div>");
    expect(out).not.toContain("script");
    expect(out).not.toContain("alert");
    expect(out).not.toContain("document.cookie");
    expect(out).toContain("before");
    expect(out).toContain("after");
  });

  it("strips onerror/onclick/style/href attributes while class survives", () => {
    const out = sanitizeTooltipHtml(
      '<div class="q4" onclick="evil()" onerror="evil()" style="color:red" href="http://evil.example">text</div>',
    );
    expect(out).toContain('class="q4"');
    expect(out).not.toContain("onclick");
    expect(out).not.toContain("onerror");
    expect(out).not.toContain("style");
    expect(out).not.toContain("href");
    expect(out).not.toContain("evil");
    expect(out).toContain("text");
  });

  it("demotes <a> to <span>, keeping only its class", () => {
    const out = sanitizeTooltipHtml('<a class="q4" href="http://evil.example">X</a>');
    expect(out).toBe('<span class="q4">X</span>');
  });

  it("drops unknown tags but keeps their text content (img gone, iframe text kept)", () => {
    const out = sanitizeTooltipHtml('<div><img src="x.png">before<iframe>istext</iframe></div>');
    expect(out).not.toContain("<img");
    expect(out).not.toContain("<iframe");
    expect(out).toContain("before");
    expect(out).toContain("istext");
  });

  it("preserves nested table structure", () => {
    const out = sanitizeTooltipHtml(
      '<table><tbody><tr><td>Cell</td><th>Head</th></tr></tbody></table>',
    );
    expect(out).toBe("<table><tbody><tr><td>Cell</td><th>Head</th></tr></tbody></table>");
  });

  it("drops a malicious class attribute that fails the allowlist regex", () => {
    const out = sanitizeTooltipHtml("<div class='q4\" onmouseover=\"x'>text</div>");
    expect(out).not.toContain("onmouseover");
    expect(out).not.toContain("class=");
    expect(out).toContain("text");
  });
});
