//! Character inventories: two ordinary sizes and one that draws every character kind.

use std::collections::BTreeSet;

use super::features::CharKind;

pub const FEATURES: [(&str, &str); 4] = [
    ("cons", "featCons"),
    ("voice", "featVoice"),
    ("cont", "featCont"),
    ("back", "featBack"),
];

#[derive(Clone, Debug)]
pub struct Seg {
    pub id: String,
    pub reps: Vec<String>,
    pub vowel: bool,
    /// Values of `cons`, `voice`, `cont`, `back`; `None` leaves the lane unspecified.
    pub feats: [Option<bool>; 4],
}

impl Seg {
    pub fn spelled(&self) -> &str {
        &self.reps[0]
    }
}

#[derive(Clone, Debug)]
pub struct Alphabet {
    pub segs: Vec<Seg>,
    pub boundary: Option<(String, String)>,
    pub kinds: BTreeSet<CharKind>,
    pub large: bool,
    /// Shapes of roots that exercise the character kinds, spelled with segment text.
    pub kind_roots: Vec<Vec<String>>,
}

fn seg(id: &str, reps: &[&str], vowel: bool, feats: [Option<bool>; 4]) -> Seg {
    Seg {
        id: format!("c_{id}"),
        reps: reps.iter().map(|r| r.to_string()).collect(),
        vowel,
        feats,
    }
}

fn v(back: bool) -> [Option<bool>; 4] {
    [Some(false), Some(true), None, Some(back)]
}

fn c(voice: bool, cont: bool) -> [Option<bool>; 4] {
    [Some(true), Some(voice), Some(cont), None]
}

impl Alphabet {
    /// `a p t s`: every grammar built on it is within the in-bound alphabet limit.
    pub fn small() -> Alphabet {
        Alphabet {
            segs: vec![
                seg("a", &["a"], true, v(true)),
                seg("p", &["p"], false, c(false, false)),
                seg("t", &["t"], false, c(true, false)),
                seg("s", &["s"], false, c(false, true)),
            ],
            boundary: Some(("bnd".into(), "+".into())),
            kinds: BTreeSet::from([CharKind::WrittenBoundary]),
            large: false,
            kind_roots: Vec::new(),
        }
    }

    /// 25 segments including a digraph and a segment with two written representations.
    pub fn large() -> Alphabet {
        let mut segs = Vec::new();
        for (ch, back) in [
            ("a", true),
            ("e", false),
            ("i", false),
            ("o", true),
            ("u", true),
        ] {
            segs.push(seg(ch, &[ch], true, v(back)));
        }
        for ch in ["p", "t", "k"] {
            segs.push(seg(ch, &[ch], false, c(false, false)));
        }
        for ch in ["b", "d", "g"] {
            segs.push(seg(ch, &[ch], false, c(true, false)));
        }
        for ch in ["f", "s", "h", "x"] {
            segs.push(seg(ch, &[ch], false, c(false, true)));
        }
        for ch in ["v", "z"] {
            segs.push(seg(ch, &[ch], false, c(true, true)));
        }
        for ch in ["m", "n"] {
            segs.push(seg(ch, &[ch], false, c(true, false)));
        }
        for ch in ["l", "r", "j", "w"] {
            segs.push(seg(ch, &[ch], false, c(true, true)));
        }
        segs.push(seg("ts", &["ts"], false, c(false, false)));
        segs.push(seg("q", &["q", "kh"], false, c(false, false)));
        Alphabet {
            segs,
            boundary: Some(("bnd".into(), "+".into())),
            kinds: BTreeSet::from([CharKind::WrittenBoundary]),
            large: true,
            kind_roots: Vec::new(),
        }
    }

    /// Every character kind of [`CharKind`], on a small base so words stay short.
    pub fn speller_kinds() -> Alphabet {
        let acute = "\u{301}";
        let grave = "\u{300}";
        let macron = "\u{304}";
        let segs = vec![
            seg("a", &["a"], true, v(true)),
            seg("e", &["e"], true, v(false)),
            seg("e_acute", &["\u{e9}"], true, v(false)),
            seg("o", &["o"], true, v(true)),
            seg("k", &["k", "K"], false, c(false, false)),
            seg("t", &["t"], false, c(false, false)),
            seg("n", &["n"], false, c(true, false)),
            seg("s", &["s"], false, c(false, true)),
            seg("cap_q", &["Q"], false, c(false, false)),
            seg("sharp", &["\u{df}"], false, c(false, true)),
            seg("ngap", &["ng'"], false, c(true, false)),
            seg("hyphen", &["-"], false, [None, None, None, None]),
            seg("m_acute", &[acute], false, [None, None, None, None]),
            seg("m_grave", &[grave], false, [None, None, None, None]),
            seg("m_macron", &[macron], false, [None, None, None, None]),
        ];
        let w = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let nine = {
            let mut word = vec!["a"];
            for _ in 0..3 {
                word.extend([acute, grave, macron]);
            }
            word
        };
        let kind_roots = vec![
            w(&["k", "a", "t", "e"]),
            w(&["k", "a", "t", "\u{e9}"]),
            w(&["o", acute, "t", "o"]),
            w(&["t", "a", acute, grave, "n", "e"]),
            w(&["K", "a", "t", "a"]),
            w(&["k", "a", "t", "a"]),
            w(&["n", "e", macron, "s", "a"]),
            w(&["n", "g'", "a", "t", "a"]),
            w(&["a", "-", "t", "a"]),
            w(&["Q", "a", "s"]),
            w(&["\u{df}", "a", "n"]),
            w(&["k", acute, "a", "n"]),
            w(&["t", "e", acute, grave, macron, "n"]),
            nine.iter().map(|x| x.to_string()).collect(),
        ];
        Alphabet {
            segs,
            boundary: Some(("bnd".into(), "+".into())),
            kinds: CharKind::ALL.into_iter().collect(),
            large: false,
            kind_roots,
        }
    }

    pub fn vowels(&self) -> Vec<usize> {
        (0..self.segs.len())
            .filter(|&i| self.segs[i].vowel)
            .collect()
    }

    pub fn consonants(&self) -> Vec<usize> {
        (0..self.segs.len())
            .filter(|&i| !self.segs[i].vowel && self.segs[i].feats[0].is_some())
            .collect()
    }

    pub fn feature_system_xml(&self) -> String {
        let mut xml = String::from("\n    <PhonologicalFeatureSystem>");
        for (name, id) in FEATURES {
            xml.push_str(&format!(
                "\n      <SymbolicFeature id=\"{id}\"><Name>{name}</Name><Symbols>\
                 <Symbol id=\"{id}P\">+</Symbol><Symbol id=\"{id}M\">-</Symbol></Symbols></SymbolicFeature>"
            ));
        }
        xml.push_str("\n    </PhonologicalFeatureSystem>");
        xml
    }

    pub fn table_xml(&self, table_id: &str) -> String {
        let mut xml = format!("\n    <CharacterDefinitionTable id=\"{table_id}\">\n      <Name>{table_id}</Name>\n      <SegmentDefinitions>");
        for s in &self.segs {
            let reps: String = s
                .reps
                .iter()
                .map(|r| format!("<Representation>{r}</Representation>"))
                .collect();
            let mut values = String::new();
            for (i, (_, fid)) in FEATURES.iter().enumerate() {
                if let Some(plus) = s.feats[i] {
                    let sym = if plus { "P" } else { "M" };
                    values.push_str(&format!(
                        "<FeatureValue feature=\"{fid}\" symbolValues=\"{fid}{sym}\" />"
                    ));
                }
            }
            xml.push_str(&format!(
                "\n        <SegmentDefinition id=\"{}_{table_id}\"><Representations>{reps}</Representations>{values}</SegmentDefinition>",
                s.id
            ));
        }
        xml.push_str("\n      </SegmentDefinitions>");
        if let Some((id, rep)) = &self.boundary {
            xml.push_str(&format!(
                "\n      <BoundaryDefinitions><BoundaryDefinition id=\"{id}_{table_id}\"><Representations>\
                 <Representation>{rep}</Representation></Representations></BoundaryDefinition></BoundaryDefinitions>"
            ));
        }
        xml.push_str("\n    </CharacterDefinitionTable>");
        xml
    }

    pub fn seg_id(&self, index: usize, table_id: &str) -> String {
        format!("{}_{table_id}", self.segs[index].id)
    }

    pub fn boundary_id(&self, table_id: &str) -> Option<String> {
        self.boundary
            .as_ref()
            .map(|(id, _)| format!("{id}_{table_id}"))
    }
}
