// Minimal ambient shim for the untyped `jquery` npm package (no bundled
// types, and this repo doesn't carry @types/jquery -- see model-viewer.ts's
// top-of-file comment for why). This has to live in a script-mode file (no
// top-level import/export of its own): TypeScript treats a `declare module
// "specifier"` block written inside a *module* file as an augmentation of
// whatever real module already resolves for that specifier, and refuses
// ("cannot be augmented") when that real module -- jquery's own JS, here --
// has no type information to merge into. A standalone ambient file avoids
// that entirely by declaring the module fresh.
declare module "jquery" {
  type JQueryStatic = (selector: string) => unknown;
  const jQuery: JQueryStatic;
  export default jQuery;
}
