// Gate F4 (docs/fst-plan/foma-fst-plan.md) wasm32 RUNTIME smoke — the check `cargo check
// --target wasm32-unknown-unknown` (gate F0) cannot do. It loads the actual wasm32 build in node
// and constructs a `PanGlossGrammar`, which builds the foma `FomaAnalyzer` -> `apply_init` and the
// emit-time phonology probe: the two call sites that historically PANICKED on
// wasm32-unknown-unknown at runtime while compiling cleanly (foma's `SystemTime::now()` seed, and
// `probe_surface`'s `thread::spawn`). If this exits 0 the foma engine is active on wasm and both
// hazards are gone.
//
// Usage: from rust/, first build the package, then run:
//   wasm-pack build crates/pg-wasm --target nodejs --dev --out-dir pkg
//   node tools/f4-wasm-smoke.js
// (Expects the reference grammars under ../samples/data — skips a grammar if absent.)
const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const PKG = path.join(ROOT, "crates/pg-wasm/pkg/pg_wasm.js");
const DATA = path.resolve(ROOT, "../samples/data");
const BINDING_FIXTURE = JSON.parse(fs.readFileSync(path.join(ROOT, "tools/fixtures/supplied-lexicon-binding.json"), "utf8"));

if (!fs.existsSync(PKG)) {
  console.error(`missing ${PKG}\nbuild first: wasm-pack build crates/pg-wasm --target nodejs --dev --out-dir pkg`);
  process.exit(2);
}
const pkg = require(PKG);

function readMaybe(p) { try { return fs.readFileSync(p, "utf8"); } catch { return null; } }
function loadGrammar(xmlName, realizeName) {
  const xmlPath = path.join(DATA, xmlName);
  if (!fs.existsSync(xmlPath)) return null;
  const realize = realizeName ? readMaybe(path.join(DATA, realizeName)) : null;
  return new pkg.PanGlossGrammar(fs.readFileSync(xmlPath, "utf8"), realize || undefined);
}
function glossesFor(result, word) {
  const tok = result.tokens.find((t) => (t.text || "").toLowerCase() === word.toLowerCase());
  return tok ? (tok.analyses || []).map((a) => a.gloss || a.leipzig).filter(Boolean) : [];
}

let failures = 0;
function check(name, cond, detail) {
  console.log(`${cond ? "PASS" : "FAIL"}  ${name}${detail ? "  -- " + detail : ""}`);
  if (!cond) failures++;
}
function expandRefs(value, fragments) {
  if (Array.isArray(value)) return value.map(v => expandRefs(v, fragments));
  if (value && typeof value === "object") {
    if (Object.keys(value).length === 1 && value.$ref) return expandRefs(fragments[value.$ref], fragments);
    return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, expandRefs(v, fragments)]));
  }
  return value;
}
function normalizeBinding(value, signature, key) {
  if (typeof value === "string") {
    if (value === signature) return "$signature";
    if (value.startsWith("pgl_")) return "$entry";
    if (key === "dateCreated" || key === "dateModified") return "$date";
    if (key === "grammarFingerprint" || key === "sourceGrammarFingerprint") return "$grammarFingerprint";
    return value;
  }
  if (Array.isArray(value)) return value.map(v => normalizeBinding(v, signature));
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([k, v]) =>
    [k === signature ? "$signature" : k, normalizeBinding(v, signature, k === signature ? "$signature" : k)]));
  return value;
}
function captureError(action) {
  try { action(); return null; }
  catch (error) { return {code: error.code, message: error.message, details: error.details}; }
}
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.keys(value).sort().map(k => `${JSON.stringify(k)}:${canonical(value[k])}`).join(",")}}`;
  return JSON.stringify(value);
}

try {
  pkg.start();
  console.log("start() ok (module loaded, panic hook installed)");

  const runtime = new pkg.PanGlossGrammar(BINDING_FIXTURE.grammarXml, undefined);
  const engineBeforeAdd = runtime.engineKind();
  const catalog = runtime.classCatalog();
  const signature = catalog.signatures[0].id;
  const invalidAdd = captureError(() => runtime.addSuppliedEntry({stem: "", gloss: "", signatures: [signature]}));
  const gloss = runtime.setGlossLanguage({glossLanguage: BINDING_FIXTURE.glossLanguage});
  const added = runtime.addSuppliedEntry({stem: BINDING_FIXTURE.stem, gloss: BINDING_FIXTURE.gloss,
    signatures: [signature], expectedRevision: gloss.revision});
  check("WASM add does not recompile proposer", runtime.engineKind() === engineBeforeAdd);
  const get = runtime.getSuppliedEntry(added.value.id);
  const list = runtime.listSuppliedEntries();
  const search = runtime.searchSuppliedEntries({query: "bee", signature, state: "active", pos: "posN"});
  const revisionConflict = captureError(() => runtime.updateSuppliedEntry({id: added.value.id, stem: "b",
    gloss: "letter bee", signatures: [signature], expectedRevision: "rev_0"}));
  const updated = runtime.updateSuppliedEntry({id: added.value.id, stem: BINDING_FIXTURE.stem,
    gloss: "letter bee", signatures: [signature], expectedRevision: added.revision});
  const authority = runtime.setEntryAuthority({id: added.value.id, authority: "supplied", expectedRevision: updated.revision});
  const exported = runtime.exportSuppliedLexicon();
  const matrix = runtime.classificationMatrix({stem: BINDING_FIXTURE.stem});
  const guideMatrix = structuredClone(matrix);
  guideMatrix.forms = [{id: "form-1", surface: "bs", predictions: [{signatureId: signature,
    derivations: [[{id: "rule-pl", label: "plural"}]]}]}];
  const guide = new pkg.ClassificationGuide(guideMatrix);
  const guideResult = {remaining: guide.remainingSignatures(), next: guide.nextForm(), useful: guide.allUsefulForms(),
    selection: guide.finalSelection()};
  guideResult.answer = guide.answer("form-1", "yes");
  guideResult.afterAnswer = guide.remainingSignatures();
  guideResult.undo = guide.undo();
  guideResult.invalidAnswer = captureError(() => guide.answer("missing", "yes"));
  const suppliedAnalysis = runtime.analyzeWord(BINDING_FIXTURE.stem);
  const grammarAnalysis = runtime.analyzeWord("a");
  const removed = runtime.removeSuppliedEntry({id: added.value.id, expectedRevision: authority.revision});
  const imported = runtime.importSuppliedLexicon({document: exported});
  const afterImport = runtime.listSuppliedEntries();
  const transcript = {catalog, invalidAdd, setGlossLanguage: gloss, add: added, get, list, search,
    revisionConflict, update: updated, setAuthority: authority, export: exported,
    classificationMatrix: matrix, guide: guideResult,
    analysis: {supplied: suppliedAnalysis, grammar: grammarAnalysis}, remove: removed, import: imported, afterImport};
  const normalized = normalizeBinding(transcript, signature);
  const expected = expandRefs(BINDING_FIXTURE.expectedTranscript, BINDING_FIXTURE.fragments);
  const transcriptMatches = canonical(normalized) === canonical(expected);
  check("WASM/native full normalized JSON transcript", transcriptMatches,
    transcriptMatches ? undefined : JSON.stringify({normalized, expected}));

  const ind = loadGrammar("indonesian-hc.xml", "indonesian-realize.toml");
  if (ind) {
    check("indonesian engineKind=foma", ind.engineKind() === "foma", `diag=${ind.engineDiagnostic()}`);
    const g = glossesFor(ind.analyzeText("ajar", {}), "ajar");
    console.log("  ajar ->", JSON.stringify(g));
    check("indonesian 'ajar' analyses (expect instruct/teach)",
      g.includes("instruct") && g.includes("teach"), `${g.length} analyses`);
  } else console.log("SKIP  indonesian (sample data absent)");

  const sena = loadGrammar("sena-hc.xml", "sena-realize.toml");
  if (sena) {
    check("sena engineKind=foma", sena.engineKind() === "foma", `diag=${sena.engineDiagnostic()}`);
    const g = glossesFor(sena.analyzeText("mbali", {}), "mbali");
    console.log("  mbali ->", JSON.stringify(g));
    check("sena 'mbali' produced analyses", g.length > 0, `${g.length} analyses`);
  } else console.log("SKIP  sena (sample data absent)");

  console.log(failures === 0 ? "\nF4 SMOKE: ALL PASS" : `\nF4 SMOKE: ${failures} FAILURE(S)`);
  process.exit(failures === 0 ? 0 : 1);
} catch (e) {
  console.error("F4 SMOKE CRASHED:", e && e.stack ? e.stack : e);
  process.exit(2);
}
