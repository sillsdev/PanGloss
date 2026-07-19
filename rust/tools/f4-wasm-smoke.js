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
