const fs = require("fs");
const path = require("path");

const EXPECTED = {
  ClassificationGuide: {
    allUsefulForms: "allUsefulForms\\(\\): any;",
    answer: "answer\\(form_id: string, judgment: any\\): any;",
    finalSelection: "finalSelection\\(\\): any;",
    matrix: "matrix\\(\\): any;",
    nextForm: "nextForm\\(\\): any;",
    remainingSignatures: "remainingSignatures\\(\\): any;",
    undo: "undo\\(\\): boolean;",
  },
  PanGlossGrammar: {
    addSuppliedEntry: "addSuppliedEntry\\(request: any\\): any;",
    analyzeText: "analyzeText\\(text: string, cache: any\\): any;",
    analyzeWord: "analyzeWord\\(word: string\\): any;",
    classCatalog: "classCatalog\\(\\): any;",
    classificationMatrix: "classificationMatrix\\(request: any\\): any;",
    clearSuppliedEntries: "clearSuppliedEntries\\(request: any\\): any;",
    exportSuppliedLexicon: "exportSuppliedLexicon\\(\\): any;",
    getSuppliedEntry: "getSuppliedEntry\\(id: string\\): any;",
    importSuppliedLexicon: "importSuppliedLexicon\\(request: any\\): any;",
    listSuppliedEntries: "listSuppliedEntries\\(\\): any;",
    removeSuppliedEntry: "removeSuppliedEntry\\(request: any\\): any;",
    searchSuppliedEntries: "searchSuppliedEntries\\(request: any\\): any;",
    setEntryAuthority: "setEntryAuthority\\(request: any\\): any;",
    setGlossLanguage: "setGlossLanguage\\(request: any\\): any;",
    updateSuppliedEntry: "updateSuppliedEntry\\(request: any\\): any;",
  },
};
const LEGACY = ["candidateClasses", "disambiguatingForms", "applyUserLexicon"];

function checkGeneratedWasmApi(root) {
  const pkg = path.join(root, "crates/pg-wasm/pkg");
  const jsPath = path.join(pkg, "pg_wasm.js");
  const dtsPath = path.join(pkg, "pg_wasm.d.ts");
  const wasmPath = path.join(pkg, "pg_wasm_bg.wasm");
  for (const file of [jsPath, dtsPath, wasmPath]) {
    if (!fs.existsSync(file)) throw new Error(`missing generated binding: ${file}`);
  }
  const sourceMtime = Math.max(
    fs.statSync(path.join(root, "crates/pg-wasm/src/lib.rs")).mtimeMs,
    fs.statSync(path.join(root, "crates/pg-wasm/Cargo.toml")).mtimeMs,
  );
  for (const file of [jsPath, dtsPath, wasmPath]) {
    if (fs.statSync(file).mtimeMs < sourceMtime) {
      throw new Error(`stale generated binding: ${file}; run wasm-pack build first`);
    }
  }
  const js = fs.readFileSync(jsPath, "utf8");
  const dts = fs.readFileSync(dtsPath, "utf8");
  function classBody(text, declaration) {
    const start = text.indexOf(declaration);
    if (start < 0) throw new Error(`missing generated class ${declaration}`);
    const open = text.indexOf("{", start);
    let depth = 0;
    for (let i = open; i < text.length; i++) {
      if (text[i] === "{") depth++;
      if (text[i] === "}" && --depth === 0) return text.slice(open + 1, i);
    }
    throw new Error(`unterminated generated class ${declaration}`);
  }
  for (const [className, methods] of Object.entries(EXPECTED)) {
    const dtsClass = classBody(dts, `export class ${className}`);
    const jsClass = classBody(js, `class ${className}`);
    for (const [method, signature] of Object.entries(methods)) {
      if (!new RegExp(signature).test(dtsClass)) throw new Error(`bad/missing .d.ts signature: ${className}.${method}`);
      if (!new RegExp(`\\n    ${method}\\(`).test(jsClass)) throw new Error(`missing generated JS method: ${className}.${method}`);
    }
  }
  for (const legacy of LEGACY) {
    if (js.includes(legacy) || dts.includes(legacy)) throw new Error(`legacy generated API remains: ${legacy}`);
  }
}

module.exports = { checkGeneratedWasmApi };

if (require.main === module) {
  checkGeneratedWasmApi(path.resolve(__dirname, ".."));
  console.log("WASM generated JS/.d.ts API: PASS");
}
