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

try {
  pkg.start();
  console.log("start() ok (module loaded, panic hook installed)");

  const runtime = new pkg.PanGlossGrammar(BINDING_FIXTURE.grammarXml, undefined);
  const engineBeforeAdd = runtime.engineKind();
  const catalog = runtime.classCatalog();
  const signature = catalog.signatures[0].id;
  let structuredError = null;
  try { runtime.addSuppliedEntry({stem: "", gloss: "", signatures: [signature]}); }
  catch (e) { structuredError = e; }
  check("WASM structured error parity", structuredError && structuredError.code === BINDING_FIXTURE.invalidAddErrorCode,
    structuredError && JSON.stringify(structuredError));
  runtime.setGlossLanguage({glossLanguage: BINDING_FIXTURE.glossLanguage});
  const added = runtime.addSuppliedEntry({stem: BINDING_FIXTURE.stem, gloss: BINDING_FIXTURE.gloss, signatures: [signature]});
  check("WASM add does not recompile proposer", runtime.engineKind() === engineBeforeAdd);
  check("WASM list/search/get", runtime.listSuppliedEntries().length === 1
    && runtime.searchSuppliedEntries({query: "bee"}).length === 1
    && runtime.getSuppliedEntry(added.value.id).id === added.value.id);
  const analyzed = runtime.analyzeWord(BINDING_FIXTURE.stem);
  check("WASM supplied provenance", analyzed.structured.some(a => a.provenance.kind === "supplied" && a.provenance.entryId === added.value.id));
  const official = runtime.analyzeWord("a");
  check("WASM grammar/supplied union", official.structured.some(a => a.provenance.kind === "grammar"));
  check("WASM authored spelling/no lowercase", runtime.getSuppliedEntry(added.value.id).stem === BINDING_FIXTURE.stem);
  const exported = runtime.exportSuppliedLexicon();
  check("WASM export schema", exported.schemaVersion === 1 && exported.entries.length === 1);
  const firstText = runtime.analyzeText(BINDING_FIXTURE.stem, {});
  const staleCache = firstText.newCacheEntries;
  const updated = runtime.updateSuppliedEntry({id: added.value.id, stem: BINDING_FIXTURE.stem,
    gloss: "letter bee", signatures: [signature], expectedRevision: added.revision});
  const afterEdit = runtime.analyzeText(BINDING_FIXTURE.stem, staleCache);
  check("WASM revision rejects stale analysis cache", afterEdit.tokens[0].fromCache === false,
    JSON.stringify({token: afterEdit.tokens[0], staleCache, revision: updated.revision}));
  const authority = runtime.setEntryAuthority({id: added.value.id, authority: "supplied", expectedRevision: updated.revision});
  check("WASM authority no-op", authority.changed === false);
  const matrix = runtime.classificationMatrix({stem: BINDING_FIXTURE.stem});
  const guide = new pkg.ClassificationGuide(matrix);
  check("WASM classification guide", guide.remainingSignatures().length === 1
    && guide.finalSelection().signatures.length === 1);
  runtime.removeSuppliedEntry({id: added.value.id});
  runtime.importSuppliedLexicon({document: exported});
  check("WASM remove/import", runtime.listSuppliedEntries().length === 1);
  runtime.clearSuppliedEntries({});
  check("WASM clear", runtime.listSuppliedEntries().length === 0);

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
