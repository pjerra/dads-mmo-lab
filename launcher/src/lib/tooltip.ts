const ALLOWED_TAGS = new Set(["TABLE", "TBODY", "TR", "TD", "TH", "SPAN", "DIV", "B", "I", "SMALL", "BR"]);
const CLASS_RE = /^[\w -]+$/;

// wowhead tooltip HTML is REMOTE content rendered via {@html} — everything
// must pass through this allowlist rebuild. <a> demotes to <span>; only the
// class attribute survives; script/style contribute nothing at all.
export function sanitizeTooltipHtml(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const out = doc.createElement("div");
  const copy = (from: Node, to: Element): void => {
    for (const child of Array.from(from.childNodes)) {
      if (child.nodeType === Node.TEXT_NODE) {
        to.appendChild(doc.createTextNode(child.textContent ?? ""));
        continue;
      }
      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      const el = child as Element;
      const tag = el.tagName;
      if (tag === "SCRIPT" || tag === "STYLE") continue;
      if (ALLOWED_TAGS.has(tag) || tag === "A") {
        const name = tag === "A" ? "span" : tag.toLowerCase();
        const clone = doc.createElement(name);
        const cls = el.getAttribute("class");
        if (cls && CLASS_RE.test(cls)) clone.setAttribute("class", cls);
        to.appendChild(clone);
        copy(el, clone);
      } else {
        copy(el, to); // unknown wrapper: keep the text/children, drop the tag
      }
    }
  };
  copy(doc.body, out);
  return out.innerHTML;
}
