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
const { checkGeneratedWasmApi } = require("./check-wasm-api.js");

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
  checkGeneratedWasmApi(ROOT);
  console.log("generated JS/.d.ts API surface ok");
  pkg.start();
  console.log("start() ok (module loaded, panic hook installed)");

  const runtime = new pkg.PanGlossGrammar(BINDING_FIXTURE.grammarXml, undefined);
  const catalog = runtime.classCatalog();
  const signature = catalog.signatures[0].id;
  const invalidAdd = captureError(() => runtime.addSuppliedEntry({stem: "", gloss: "", signatures: [signature]}));
  const gloss = runtime.setGlossLanguage({glossLanguage: BINDING_FIXTURE.glossLanguage});
  const added = runtime.addSuppliedEntry({stem: BINDING_FIXTURE.stem, gloss: BINDING_FIXTURE.gloss,
    signatures: [signature], expectedRevision: gloss.revision});
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
  const cleared = runtime.clearSuppliedEntries({expectedRevision: imported.revision});
  const afterClear = runtime.listSuppliedEntries();
  const restored = runtime.importSuppliedLexicon({document: exported});
  const afterRestore = runtime.listSuppliedEntries();

  const caseRuntime = new pkg.PanGlossGrammar(BINDING_FIXTURE.grammarXml, undefined);
  const caseSignature = caseRuntime.classCatalog().signatures[0].id;
  const caseAdded = caseRuntime.addSuppliedEntry({stem: "B", gloss: "", signatures: [caseSignature]});
  const caseGet = caseRuntime.getSuppliedEntry(caseAdded.value.id);
  const caseList = caseRuntime.listSuppliedEntries();
  const caseSearch = caseRuntime.searchSuppliedEntries({query: "B"});
  const caseExport = caseRuntime.exportSuppliedLexicon();
  const caseAnalysis = caseRuntime.analyzeWord("B");
  const transcript = {catalog, invalidAdd, setGlossLanguage: gloss, add: added, get, list, search,
    revisionConflict, update: updated, setAuthority: authority, export: exported,
    classificationMatrix: matrix, guide: guideResult,
    analysis: {supplied: suppliedAnalysis, grammar: grammarAnalysis}, remove: removed, import: imported, afterImport,
    clear: cleared, afterClear, restore: restored, afterRestore,
    authoredCase: {add: caseAdded, get: caseGet, list: caseList, search: caseSearch, export: caseExport, analysis: caseAnalysis}};
  const normalized = normalizeBinding(transcript, signature);
  const expected = expandRefs(BINDING_FIXTURE.expectedTranscript, BINDING_FIXTURE.fragments);
  const transcriptMatches = canonical(normalized) === canonical(expected);
  check("WASM/native full normalized JSON transcript", transcriptMatches,
    transcriptMatches ? undefined : JSON.stringify({normalized, expected}));

  const originalCaseText = caseRuntime.analyzeText("B", {});
  const staleCaseCache = originalCaseText.newCacheEntries;
  const caseGloss = caseRuntime.setGlossLanguage({glossLanguage: "en", expectedRevision: caseAdded.revision});
  const caseUpdated = caseRuntime.updateSuppliedEntry({id: caseAdded.value.id, stem: "B", gloss: "updated capital bee",
    signatures: [caseSignature], expectedRevision: caseGloss.revision});
  const refreshedCaseText = caseRuntime.analyzeText("B", staleCaseCache);
  const staleCaseRejected = staleCaseCache.B.overlayRevision === caseAdded.revision
      && refreshedCaseText.tokens[0].fromCache === false
      && refreshedCaseText.newCacheEntries.B.overlayRevision === caseUpdated.revision
      && refreshedCaseText.tokens[0].analyses.some(a => a.provenance.kind === "supplied" && a.provenance.entryId === caseAdded.value.id);
  check("WASM stale caller cache rejected after gloss-only edit", staleCaseRejected,
    staleCaseRejected ? undefined : JSON.stringify({staleCaseCache, refreshedCaseText, caseUpdated}));
  check("WASM authored-case cache identity is exact",
    Object.hasOwn(staleCaseCache, "B") && !Object.hasOwn(staleCaseCache, "b")
      && Object.hasOwn(refreshedCaseText.newCacheEntries, "B")
      && !Object.hasOwn(refreshedCaseText.newCacheEntries, "b")
      && refreshedCaseText.tokens[0].text === "B");

  const ind = loadGrammar("indonesian-hc.xml", "indonesian-realize.toml");
  if (ind) {
    const g = glossesFor(ind.analyzeText("ajar", {}), "ajar");
    console.log("  ajar ->", JSON.stringify(g));
    check("indonesian 'ajar' analyses (expect instruct/teach)",
      g.includes("instruct") && g.includes("teach"), `${g.length} analyses`);
  } else console.log("SKIP  indonesian (sample data absent)");

  const sena = loadGrammar("sena-hc.xml", "sena-realize.toml");
  if (sena) {
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
