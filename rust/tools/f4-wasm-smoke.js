// Usage: from rust/, first build the package, then run:
//   wasm-pack build crates/pg-wasm --target nodejs --dev --out-dir pkg
//   node tools/f4-wasm-smoke.js
// (Expects the reference grammars under ../samples/data — skips a grammar if absent.)
const fs = require("fs");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const PKG_DIR = process.env.PANGLOSS_WASM_PACKAGE_DIR
  ? path.resolve(process.env.PANGLOSS_WASM_PACKAGE_DIR)
  : path.join(ROOT, "crates/pg-wasm/pkg");
const PKG = path.join(PKG_DIR, "pg_wasm.js");
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

function checkWasmTokenization() {
  const charsXml = BINDING_FIXTURE.grammarXml.replace(
    "</SegmentDefinitions>",
    '<SegmentDefinition id="e-acute"><Representations><Representation>é</Representation></Representations></SegmentDefinition>' +
      '<SegmentDefinition id="bang"><Representations><Representation>!</Representation></Representations></SegmentDefinition>' +
      "</SegmentDefinitions>",
  );
  const boundaryXml = charsXml.replace(
    "</CharacterDefinitionTable>",
    '<BoundaryDefinitions><BoundaryDefinition id="space"><Representations><Representation>&#x20;</Representation></Representations></BoundaryDefinition></BoundaryDefinitions></CharacterDefinitionTable>',
  );
  const grammarXml = boundaryXml.replace(
    "</LexicalEntries>",
    '<LexicalEntry id="authored-e-acute" partOfSpeech="posN"><Allomorphs><Allomorph id="e-acute-root"><PhoneticShape>é</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>' +
      '<LexicalEntry id="authored-bang" partOfSpeech="posN"><Allomorphs><Allomorph id="bang-root"><PhoneticShape>!</PhoneticShape></Allomorph></Allomorphs></LexicalEntry>' +
      "</LexicalEntries>",
  );
  const runtime = new pkg.PanGlossGrammar(grammarXml, undefined);
  const wordTexts = ["é", "e\u0301", "!", "e\u0300", "b'b"];
  const whitespaceMark = " \u0301";
  const text = wordTexts.join(" ") + whitespaceMark;
  const first = runtime.analyzeText(text, {});
  const wordTokens = first.tokens.filter((token) => token.kind === "word");
  const tokenByText = new Map(wordTokens.map((token) => [token.text, token]));
  const owner = new Map(wordTexts.map((word) => [word, runtime.analyzeWord(word)]));
  const NFC = "é";
  const NFD = "e\u0301";
  const nfcOwner = owner.get(NFC);
  const nfdOwner = owner.get(NFD);
  const nfcToken = tokenByText.get(NFC);
  const nfdToken = tokenByText.get(NFD);
  const identity = (analysis) => ({
    guessed: analysis.guessed,
    provenance: analysis.provenance,
    posId: analysis.posId,
    rootMorphemeIndex: analysis.rootMorphemeIndex,
    suppliedRoot: analysis.suppliedRoot,
    synFs: analysis.synFs,
  });
  const ownerStructured = (outcome) => (outcome.structured || []).map(identity);
  const tokenStructured = (token) => (token.analyses || []).map(({guessed, provenance}) => ({guessed, provenance}));
  const tokenMatchesOwner = (token, outcome) => !!token &&
    token.analyses.length === outcome.structured.length &&
    canonical(tokenStructured(token)) === canonical(outcome.structured.map(({guessed, provenance}) => ({guessed, provenance}))) &&
    token.invalidShape === outcome.invalidShape &&
    token.capped === outcome.capped &&
    token.candidatesGenerated === outcome.candidatesGenerated &&
    token.candidatesAccepted === outcome.structured.length;

  check("WASM authored NFC/NFD roots and punctuation parse through real bindings",
    wordTokens.length === wordTexts.length &&
      wordTexts.every((word, index) => wordTokens[index].text === word) &&
      ["é", "e\u0301", "!"].every((word) => owner.get(word).structured.length > 0 &&
        !owner.get(word).invalidShape && tokenByText.get(word)?.analyses.length > 0),
    JSON.stringify(wordTokens.map(({text, analyses, invalidShape}) => ({text, analyses: analyses.length, invalidShape}))));

  check("WASM NFC/NFD owner identities and token analyses agree",
    !!nfcOwner && !!nfdOwner &&
      canonical(ownerStructured(nfcOwner)) === canonical(ownerStructured(nfdOwner)) &&
      tokenMatchesOwner(nfcToken, nfcOwner) &&
      tokenMatchesOwner(nfdToken, nfdOwner),
    JSON.stringify({nfcOwner, nfdOwner, nfcToken, nfdToken}));

  const nfcDisplay = nfcToken?.analyses.map(({gloss, complete, residue, guessed, provenance, morphemeIds}) =>
    ({gloss, complete, residue, guessed, provenance, morphemeIds}));
  const nfdDisplay = nfdToken?.analyses.map(({gloss, complete, residue, guessed, provenance, morphemeIds}) =>
    ({gloss, complete, residue, guessed, provenance, morphemeIds}));
  check("WASM NFC/NFD displayed analyses preserve multiplicity and completion",
    !!nfcDisplay && canonical(nfcDisplay) === canonical(nfdDisplay) &&
      nfcDisplay.every(({complete, residue}) => typeof complete === "boolean" && Array.isArray(residue)),
    JSON.stringify({nfcDisplay, nfdDisplay}));

  const unknownCombining = tokenByText.get("e\u0300");
  check("WASM unknown combining mark stays attached and rejects the whole word",
    !!unknownCombining && unknownCombining.invalidShape === true &&
      unknownCombining.analyses.length === 0 && owner.get("e\u0300").invalidShape === true &&
      owner.get("e\u0300").structured.length === 0,
    JSON.stringify({token: unknownCombining, owner: owner.get("e\u0300")}));

  check("WASM legacy ASCII apostrophe remains word material",
    tokenByText.has("b'b") && tokenByText.get("b'b").invalidShape === true &&
      tokenByText.get("b'b").analyses.length === 0,
    JSON.stringify(wordTokens.map(({text, kind, invalidShape}) => ({text, kind, invalidShape}))));

  check("WASM whitespace plus a declared mark remains exact separator text",
    first.tokens.some((token) => token.kind === "other" && token.text === whitespaceMark),
    JSON.stringify(first.tokens.map(({kind, text}) => ({kind, text}))));

  const rebuiltText = first.tokens.map((token) => token.text).join("");
  const exactCacheKeys = Object.keys(first.newCacheEntries).sort();
  const expectedCacheKeys = [...wordTexts].sort();
  const replay = runtime.analyzeText(text, first.newCacheEntries);
  const replayWords = replay.tokens.filter((token) => token.kind === "word");
  check("WASM authored text and exact surface cache identity are preserved",
    rebuiltText === text &&
      canonical(exactCacheKeys) === canonical(expectedCacheKeys) &&
      replayWords.length === wordTexts.length &&
      replayWords.every((token, index) => token.text === wordTexts[index] && token.fromCache) &&
      Object.keys(replay.newCacheEntries).length === 0,
    JSON.stringify({rebuiltText, exactCacheKeys, replayWords: replayWords.map(({text, fromCache}) => ({text, fromCache}))}));
}

try {
  if (!process.env.PANGLOSS_WASM_PACKAGE_DIR) {
    checkGeneratedWasmApi(ROOT);
    console.log("generated JS/.d.ts API surface ok");
  }
  pkg.start();
  console.log("start() ok (module loaded, panic hook installed)");
  checkWasmTokenization();

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
