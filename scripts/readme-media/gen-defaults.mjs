// Builds a placeholder value for every type in src/bindings.ts (first enum
// literal, false, 0, "", [], null for nullable), so the UI can render in a
// browser without the Rust backend. Output: defaults.json { TypeName: value }.
import ts from "typescript";
import { readFileSync, writeFileSync } from "fs";

const src = readFileSync("src/bindings.ts", "utf8");
const sf = ts.createSourceFile("b.ts", src, ts.ScriptTarget.Latest, true);
const aliases = new Map();
sf.forEachChild((n) => {
  if (ts.isTypeAliasDeclaration(n)) aliases.set(n.name.text, n.type);
});
const out = {};
const val = (t, depth = 0) => {
  if (depth > 8) return null;
  switch (t.kind) {
    case ts.SyntaxKind.BooleanKeyword:
      return false;
    case ts.SyntaxKind.NumberKeyword:
      return 0;
    case ts.SyntaxKind.StringKeyword:
      return "";
    case ts.SyntaxKind.NullKeyword:
      return null;
    case ts.SyntaxKind.AnyKeyword:
    case ts.SyntaxKind.UnknownKeyword:
      return null;
  }
  if (ts.isLiteralTypeNode(t)) {
    if (ts.isStringLiteral(t.literal)) return t.literal.text;
    if (t.literal.kind === ts.SyntaxKind.NullKeyword) return null;
    if (t.literal.kind === ts.SyntaxKind.TrueKeyword) return true;
    if (t.literal.kind === ts.SyntaxKind.FalseKeyword) return false;
    return Number(t.literal.getText());
  }
  if (ts.isArrayTypeNode(t) || ts.isTupleTypeNode(t)) return [];
  if (ts.isParenthesizedTypeNode(t)) return val(t.type, depth);
  if (ts.isUnionTypeNode(t)) {
    if (
      t.types.some(
        (x) =>
          ts.isLiteralTypeNode(x) &&
          x.literal.kind === ts.SyntaxKind.NullKeyword,
      )
    )
      return null;
    return val(t.types[0], depth + 1);
  }
  if (ts.isTypeLiteralNode(t)) {
    const o = {};
    for (const m of t.members) {
      if (ts.isPropertySignature(m) && m.type) {
        const name = m.name.getText().replace(/^"|"$/g, "");
        o[name] = m.questionToken ? null : val(m.type, depth + 1);
      }
    }
    return o;
  }
  if (ts.isTypeReferenceNode(t)) {
    const name = t.typeName.getText();
    if (name === "Partial" || name === "Record" || name === "Map") return {};
    if (name === "Array") return [];
    if (aliases.has(name)) return val(aliases.get(name), depth + 1);
    return null;
  }
  return null;
};
for (const [name, t] of aliases) out[name] = val(t);
// Each command's placeholder answer, from its return type.
const commands = {};
const visit = (n) => {
  if (ts.isMethodDeclaration(n) && n.type && n.body) {
    const m = /TAURI_INVOKE\("([^"]+)"/.exec(n.body.getText());
    if (m) {
      let t = n.type; // Promise<X>
      if (ts.isTypeReferenceNode(t) && t.typeArguments) t = t.typeArguments[0];
      if (
        ts.isTypeReferenceNode(t) &&
        t.typeName.getText() === "Result" &&
        t.typeArguments
      )
        t = t.typeArguments[0];
      commands[m[1]] = val(t);
    }
  }
  n.forEachChild(visit);
};
visit(sf);
writeFileSync(
  process.argv[2],
  JSON.stringify({ types: out, commands }, null, 1),
);
console.log("types", aliases.size, "commands", Object.keys(commands).length);
