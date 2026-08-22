//! Static backend capability-card data and deterministic Markdown rendering.

use std::fmt::Write;

pub const CARD_SCHEMA_VERSION: u32 = 1;
const SAFETY_WARNING: &str = "Don't make any change that would make your language invalid!";
const ADVICE_CATALOG_LINK: &str = "../../../rust/crates/pg-foma/assets/backend-advice-v1.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCard {
    pub backend_id: &'static str,
    pub display_name: &'static str,
    pub summary: &'static str,
    pub envelopes: &'static [Envelope],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Envelope {
    pub id: &'static str,
    pub name: &'static str,
    pub control: EnvelopeControl,
    pub big_o: BigO,
    pub contributors: &'static [&'static str],
    pub remedy_ids: &'static [&'static str],
    pub source_refs: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeControl {
    Inherent,
    SwitchControlled {
        switch_id: &'static str,
        default: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BigO {
    pub time: &'static str,
    pub space: &'static str,
    pub variables: &'static [&'static str],
}

const TUNED_CONTRIBUTORS: &[&str] = &[
    "E = emitted lexical entries",
    "J = surface/deletion junction variants",
    "P = ordered phonological rules",
    "N = reachable composite states",
    "Rule ordering changes probe reuse and the number of distinct junctions",
    "Null realizations and deletion increase reachable zero-width and truncated branches",
];
const TUNED_REMEDIES: &[&str] = &[
    "retry-larger-closure-envelope",
    "use-loop-capable-backend",
    "order-or-slot-localize-rules",
];
const TUNED_SOURCES: &[&str] = &["src/emit.rs", "src/junctions.rs", "src/preexpand.rs"];
const TUNED_ENVELOPES: &[Envelope] = &[Envelope {
    id: "tuned-surface-closure",
    name: "Surface-probed composite closure",
    control: EnvelopeControl::SwitchControlled {
        switch_id: "PG_FOMA_TUNED_SURFACE_CLOSURE_BUDGET",
        default: "managed default",
    },
    big_o: BigO {
        time: "O(E x J x P + N)",
        space: "O(E + J + N)",
        variables: &[
            "E: emitted entries",
            "J: junction variants",
            "P: ordered rule count",
            "N: composite states",
        ],
    },
    contributors: TUNED_CONTRIBUTORS,
    remedy_ids: TUNED_REMEDIES,
    source_refs: TUNED_SOURCES,
}];

const TEMPLATED_CONTRIBUTORS: &[&str] = &[
    "E = emitted lexical entries",
    "P = ordered rewrite rules and their environment composition",
    "T = template obligations and token lanes",
    "Rule ordering affects cascade depth and intermediate alphabets",
    "Null and deletion rules add epsilon/truncation branches to the relation",
];
const TEMPLATED_REMEDIES: &[&str] = &[
    "use-whole-grammar-backend",
    "regularize-phonology",
    "order-rules",
];
const TEMPLATED_SOURCES: &[&str] = &["src/emit.rs", "src/replace.rs", "src/enumerate.rs"];
const TEMPLATED_ENVELOPES: &[Envelope] = &[Envelope {
    id: "templated-underlying-rewrite",
    name: "Underlying-token rewrite cascade",
    control: EnvelopeControl::Inherent,
    big_o: BigO {
        time: "O(E x P x T)",
        space: "O(E + P x T)",
        variables: &[
            "E: emitted entries",
            "P: ordered rewrite rules",
            "T: template/token lanes",
        ],
    },
    contributors: TEMPLATED_CONTRIBUTORS,
    remedy_ids: TEMPLATED_REMEDIES,
    source_refs: TEMPLATED_SOURCES,
}];

const PLAN_CONTRIBUTORS: &[&str] = &[
    "G = reachable gate groups",
    "R = rewrite rules in authored order",
    "Q = required plan subtrees",
    "Rule ordering changes the content-addressed replacement cascade",
    "Null, deletion, and structural marker leaves can require unsupported subtrees",
    "Branching multiplies gate-group and replacement combinations",
];
const PLAN_REMEDIES: &[&str] = &[
    "use-whole-grammar-backend",
    "implement-required-plan-subtrees",
    "use-obligation-templates",
];
const PLAN_SOURCES: &[&str] = &["src/enumerate.rs", "src/plan.rs", "src/build.rs"];
const PLAN_ENVELOPES: &[Envelope] = &[Envelope {
    id: "plan-composed-materialization",
    name: "Controllable plan materialization",
    control: EnvelopeControl::Inherent,
    big_o: BigO {
        time: "O(G x R + Q)",
        space: "O(G + R + Q)",
        variables: &[
            "G: gate groups",
            "R: rewrite rules",
            "Q: required plan subtrees",
        ],
    },
    contributors: PLAN_CONTRIBUTORS,
    remedy_ids: PLAN_REMEDIES,
    source_refs: PLAN_SOURCES,
}];

const CARDS: &[BackendCard] = &[
    BackendCard {
        backend_id: "plan-composed",
        display_name: "Plan Composed",
        summary: "Materializes the controllable, content-addressed portion of the enumerated plan.",
        envelopes: PLAN_ENVELOPES,
    },
    BackendCard {
        backend_id: "templated-underlying-tokens",
        display_name: "Templated Underlying Tokens",
        summary: "Emits underlying token lanes and applies the whole rewrite cascade.",
        envelopes: TEMPLATED_ENVELOPES,
    },
    BackendCard {
        backend_id: "tuned-surface-probed",
        display_name: "Tuned Surface Probed",
        summary: "Pre-probes surface and deletion junctions, then emits a whole-grammar relation.",
        envelopes: TUNED_ENVELOPES,
    },
];

pub fn catalog() -> &'static [BackendCard] {
    CARDS
}

pub fn checked_in_relative_path(backend_id: &str) -> String {
    format!("docs/fst-plan/backend-cards/{}.md", backend_id)
}

pub fn render_markdown(card: &BackendCard) -> String {
    let mut output = String::new();
    writeln!(output, "# {}", card.display_name).unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "> Static backend contract (schema v{}); this card contains no language, corpus, timing, or machine observations.",
        CARD_SCHEMA_VERSION
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(output, "- Backend ID: `{}`", card.backend_id).unwrap();
    writeln!(output, "- Summary: {}", card.summary).unwrap();
    writeln!(output).unwrap();
    writeln!(output, "## Capability envelopes").unwrap();
    writeln!(output).unwrap();
    for envelope in card.envelopes {
        writeln!(output, "### `{}` — {}", envelope.id, envelope.name).unwrap();
        match envelope.control {
            EnvelopeControl::Inherent => {
                writeln!(
                    output,
                    "- Control: inherent; always part of this backend's contract."
                )
                .unwrap();
            }
            EnvelopeControl::SwitchControlled { switch_id, default } => {
                writeln!(
                    output,
                    "- Control: switch-controlled by `{}`; default: `{}`.",
                    switch_id, default
                )
                .unwrap();
            }
        }
        writeln!(output, "- Time: `{}`", envelope.big_o.time).unwrap();
        writeln!(output, "- Space: `{}`", envelope.big_o.space).unwrap();
        writeln!(
            output,
            "- Variables: {}.",
            envelope.big_o.variables.join(", ")
        )
        .unwrap();
        writeln!(output, "- Contributors:").unwrap();
        for contributor in envelope.contributors {
            writeln!(output, "  - {}", contributor).unwrap();
        }
        writeln!(output, "- Remedies:").unwrap();
        for remedy_id in envelope.remedy_ids {
            writeln!(output, "  - `{}`", remedy_id).unwrap();
        }
        writeln!(
            output,
            "- Advice: [authoritative remedy text and shape-specific effort]({}). A remedy would make this backend work for your language only when its stated prerequisites hold.",
            ADVICE_CATALOG_LINK
        )
        .unwrap();
        writeln!(
            output,
            "- Source references: {}.",
            envelope.source_refs.join(", ")
        )
        .unwrap();
        writeln!(output).unwrap();
    }
    writeln!(output, "{}", SAFETY_WARNING).unwrap();
    output
}
