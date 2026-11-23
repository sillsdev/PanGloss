//! `Units`: assembles NFD characters into edit units (plan S.2, S.3) and the unit-token table the
//! `Fold` and error layers share.
//!
//! A unit is one non-mark character plus the marks that follow it; marks at the very start of the
//! string form one unit with an empty base. The pending stack is kept exactly while it is a prefix
//! of the stack of some typed unit with the same FOLDED base (a node of that base's trie of typed
//! stacks) and is the single value `other` once a mark leaves the typed prefixes. There is no
//! depth cut and no cap (mutations C130, C147). A mark of a non-zero combining class lower than the
//! previous mark's makes the unit non-canonical and the path is cut; no reordering (C131).
//!
//! Layer shape. States: `start` (accepts a word-initial mark), `mid` (a non-mark only), and one
//! pending state per (exact base, trie node) plus (exact base, `other`, class of the last mark).
//! A pending state finishes its unit by an epsilon-input arc that writes the unit token and goes
//! to `mid`; only `start` and `mid` read a non-mark, so the unit is flushed before the next base
//! arrives and before the end of the word (final states are `start` and `mid` only). This keeps
//! the arc count linear in the pending states.

use std::collections::{BTreeSet, HashMap};

use unicode_normalization::char::{canonical_combining_class, is_combining_mark};
use unicode_normalization::UnicodeNormalization;

use super::budget::{BuildMeter, BuildRefused};
use super::fold::fold_base;
use super::layer::{Input, NfaBuilder, NfaLayer, Output, StateId, Token};

pub type BaseId = u32;

/// The stack part of a unit token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Stack {
    /// A node of the trie of typed stacks of the token's folded base (the root is the empty stack).
    Node(u32),
    /// Any stack that is not a prefix of a typed stack of that base.
    Other,
}

/// A unit as the layers below `Units` see it: exact base and abstracted stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UnitTok {
    pub base: BaseId,
    pub stack: Stack,
}

pub fn nfd_string(s: &str) -> String {
    s.nfd().collect()
}

pub fn is_mark(c: char) -> bool {
    is_combining_mark(c)
}

/// Cut an NFD string into units `(base, marks)`; leading marks are one unit with an empty base.
pub fn cut_units(s: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for c in s.nfd() {
        if is_mark(c) {
            match out.last_mut() {
                Some(u) => u.1.push(c),
                None => out.push((String::new(), c.to_string())),
            }
        } else {
            out.push((c.to_string(), String::new()));
        }
    }
    out
}

#[derive(Clone, Debug)]
struct TrieNode {
    fbase: BaseId,
    last_class: u8,
    children: Vec<(char, u32)>,
}

/// The characters, bases, typed-stack tries and unit tokens of one request (one typed word).
#[derive(Clone, Debug)]
pub struct UnitModel {
    chars: Vec<char>,
    char_ix: HashMap<char, u32>,
    bases: Vec<String>,
    base_ix: HashMap<String, BaseId>,
    fold_of: Vec<BaseId>,
    nodes: Vec<TrieNode>,
    root_of: HashMap<BaseId, u32>,
    tokens: Vec<UnitTok>,
    tok_ix: HashMap<UnitTok, Token>,
    typed: Vec<Token>,
    max_typed_marks: usize,
}

impl UnitModel {
    /// `alphabet` are the NFD characters the cascade can write (any order); `typed` is the typed
    /// word (normalised here). Typed characters join the alphabet as tokens even when the cascade
    /// cannot write them.
    pub fn new(alphabet: &[char], typed: &str) -> UnitModel {
        let typed_nfd = nfd_string(typed);
        let mut set: BTreeSet<char> = alphabet.iter().copied().collect();
        set.extend(typed_nfd.chars());
        let chars: Vec<char> = set.into_iter().collect();
        let char_ix = chars
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, i as u32))
            .collect();
        let mut m = UnitModel {
            chars,
            char_ix,
            bases: Vec::new(),
            base_ix: HashMap::new(),
            fold_of: Vec::new(),
            nodes: Vec::new(),
            root_of: HashMap::new(),
            tokens: Vec::new(),
            tok_ix: HashMap::new(),
            typed: Vec::new(),
            max_typed_marks: 0,
        };
        m.intern_base("");
        let nonmarks: Vec<char> = m.chars.iter().copied().filter(|&c| !is_mark(c)).collect();
        for c in nonmarks {
            m.intern_base(&c.to_string());
        }
        // Folded bases, closed under folding.
        let mut i = 0;
        while i < m.bases.len() {
            let f = fold_base(&m.bases[i].clone());
            let fid = m.intern_base(&f);
            m.fold_of[i] = fid;
            i += 1;
        }
        // Typed stacks.
        let mut typed_units = Vec::new();
        for (b, marks) in cut_units(&typed_nfd) {
            let fb = m.intern_base(&fold_base(&b));
            let mut node = m.root(fb);
            let mut cnt = 0;
            for c in marks.chars() {
                node = m.child_or_insert(node, c);
                cnt += 1;
            }
            m.max_typed_marks = m.max_typed_marks.max(cnt);
            typed_units.push((fb, node));
        }
        // Every fold target has a root.
        for b in 0..m.bases.len() as u32 {
            let t = m.fold_of[b as usize];
            m.root(t);
        }
        // Unit tokens: every base with every node of its fold target's trie, and `other`.
        for b in 0..m.bases.len() as u32 {
            let target = m.fold_of[b as usize];
            let nodes: Vec<u32> = (0..m.nodes.len() as u32)
                .filter(|&n| m.nodes[n as usize].fbase == target)
                .collect();
            for n in nodes {
                m.intern_token(UnitTok {
                    base: b,
                    stack: Stack::Node(n),
                });
            }
            m.intern_token(UnitTok {
                base: b,
                stack: Stack::Other,
            });
        }
        m.typed = typed_units
            .into_iter()
            .map(|(fb, n)| {
                m.tok_ix[&UnitTok {
                    base: fb,
                    stack: Stack::Node(n),
                }]
            })
            .collect();
        m
    }

    fn intern_base(&mut self, s: &str) -> BaseId {
        if let Some(&i) = self.base_ix.get(s) {
            return i;
        }
        let i = self.bases.len() as BaseId;
        self.bases.push(s.to_string());
        self.base_ix.insert(s.to_string(), i);
        self.fold_of.push(i);
        i
    }

    fn root(&mut self, fbase: BaseId) -> u32 {
        if let Some(&r) = self.root_of.get(&fbase) {
            return r;
        }
        let r = self.nodes.len() as u32;
        self.nodes.push(TrieNode {
            fbase,
            last_class: 0,
            children: Vec::new(),
        });
        self.root_of.insert(fbase, r);
        r
    }

    fn child_or_insert(&mut self, node: u32, mark: char) -> u32 {
        if let Some(c) = self.child(node, mark) {
            return c;
        }
        let id = self.nodes.len() as u32;
        let fbase = self.nodes[node as usize].fbase;
        self.nodes.push(TrieNode {
            fbase,
            last_class: canonical_combining_class(mark),
            children: Vec::new(),
        });
        self.nodes[node as usize].children.push((mark, id));
        id
    }

    fn child(&self, node: u32, mark: char) -> Option<u32> {
        self.nodes[node as usize]
            .children
            .iter()
            .find(|&&(c, _)| c == mark)
            .map(|&(_, n)| n)
    }

    fn intern_token(&mut self, t: UnitTok) -> Token {
        if let Some(&i) = self.tok_ix.get(&t) {
            return i;
        }
        let i = self.tokens.len() as Token;
        self.tokens.push(t);
        self.tok_ix.insert(t, i);
        i
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn char_token(&self, c: char) -> Option<Token> {
        self.char_ix.get(&c).copied()
    }

    pub fn token_count(&self) -> u32 {
        self.tokens.len() as u32
    }

    /// The alphabet size every layer of one product uses: characters and unit tokens share it.
    pub fn universe(&self) -> u32 {
        self.token_count().max(self.chars.len() as u32)
    }

    pub fn unit_tok(&self, t: Token) -> UnitTok {
        self.tokens[t as usize]
    }

    pub fn token(&self, base: &str, stack: Stack) -> Option<Token> {
        let b = *self.base_ix.get(base)?;
        self.tok_ix.get(&UnitTok { base: b, stack }).copied()
    }

    pub fn base_str(&self, b: BaseId) -> &str {
        &self.bases[b as usize]
    }

    /// The folded unit tokens of the typed word (the `W` layer's word, S.3).
    pub fn typed_tokens(&self) -> &[Token] {
        &self.typed
    }

    /// Deepest stack among the typed units.
    pub fn max_typed_marks(&self) -> usize {
        self.max_typed_marks
    }

    pub fn empty_base(&self) -> BaseId {
        0
    }

    /// True for tokens whose base is its own folding: what `Fold` writes, and the error layers' alphabet.
    pub fn is_folded_token(&self, t: Token) -> bool {
        let b = self.tokens[t as usize].base;
        self.fold_of[b as usize] == b
    }

    /// The token `Fold` writes for `t`: folded base, same stack.
    pub fn fold_token(&self, t: Token) -> Token {
        let u = self.tokens[t as usize];
        self.tok_ix[&UnitTok {
            base: self.fold_of[u.base as usize],
            stack: u.stack,
        }]
    }

    /// Human-readable token (tests).
    pub fn describe(&self, t: Token) -> String {
        let u = self.tokens[t as usize];
        match u.stack {
            Stack::Other => format!("{}+other", self.bases[u.base as usize]),
            Stack::Node(n) => format!("{}+n{}", self.bases[u.base as usize], n),
        }
    }

    /// Marks of a trie node, root first.
    pub fn node_marks(&self, node: u32) -> String {
        let mut path = Vec::new();
        let mut cur = node;
        'outer: loop {
            for (i, n) in self.nodes.iter().enumerate() {
                if let Some(&(c, _)) = n.children.iter().find(|&&(_, ch)| ch == cur) {
                    path.push(c);
                    cur = i as u32;
                    continue 'outer;
                }
            }
            break;
        }
        path.iter().rev().collect()
    }

    fn mark_classes(&self) -> Vec<u8> {
        let set: BTreeSet<u8> = self
            .chars
            .iter()
            .filter(|&&c| is_mark(c))
            .map(|&c| canonical_combining_class(c))
            .collect();
        set.into_iter().collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Pend {
    Node(BaseId, u32),
    Other(BaseId, u8),
}

/// The `Units` layer over the character tokens of `model` (`Token` = index in `model.chars()`).
pub fn build_units_layer(
    model: &UnitModel,
    meter: &mut BuildMeter,
) -> Result<NfaLayer, BuildRefused> {
    let mut b = NfaBuilder::new("Units", model.universe(), meter);
    let start = b.add_state(true)?;
    let mid = b.add_state(true)?;
    b.set_start(start);
    let classes = model.mark_classes();
    let mut ids: HashMap<Pend, StateId> = HashMap::new();
    let mut order: Vec<Pend> = Vec::new();
    // Exact bases that a character can start: every non-mark character, plus the empty base.
    let mut exact_bases: Vec<BaseId> = vec![0];
    for &c in &model.chars {
        if !is_mark(c) {
            exact_bases.push(model.base_ix[&c.to_string()]);
        }
    }
    for &eb in &exact_bases {
        let target = model.fold_of[eb as usize];
        for (n, node) in model.nodes.iter().enumerate() {
            if node.fbase == target {
                let p = Pend::Node(eb, n as u32);
                ids.insert(p, b.add_state(false)?);
                order.push(p);
            }
        }
        for &cl in &classes {
            let p = Pend::Other(eb, cl);
            ids.insert(p, b.add_state(false)?);
            order.push(p);
        }
    }
    let root = |eb: BaseId| model.root_of[&model.fold_of[eb as usize]];
    for &c in &model.chars {
        if is_mark(c) {
            continue;
        }
        let eb = model.base_ix[&c.to_string()];
        let to = ids[&Pend::Node(eb, root(eb))];
        let t = model.char_ix[&c];
        b.add_arc(start, Input::Tok(t), Output::Eps, to, &[], 0, 0)?;
        b.add_arc(mid, Input::Tok(t), Output::Eps, to, &[], 0, 0)?;
    }
    // Word-initial marks: the empty-base unit.
    for &m in model.chars.iter().filter(|&&c| is_mark(c)) {
        let cl = canonical_combining_class(m);
        let t = model.char_ix[&m];
        let to = match model.child(root(0), m) {
            Some(n) => ids[&Pend::Node(0, n)],
            None => ids[&Pend::Other(0, cl)],
        };
        b.add_arc(start, Input::Tok(t), Output::Eps, to, &[], 0, 0)?;
    }
    for &p in &order {
        let (eb, stack, last) = match p {
            Pend::Node(eb, n) => (eb, Stack::Node(n), model.nodes[n as usize].last_class),
            Pend::Other(eb, cl) => (eb, Stack::Other, cl),
        };
        let from = ids[&p];
        for &m in model.chars.iter().filter(|&&c| is_mark(c)) {
            let cl = canonical_combining_class(m);
            if cl != 0 && cl < last {
                continue;
            }
            let to = match p {
                Pend::Node(eb, n) => match model.child(n, m) {
                    Some(ch) => ids[&Pend::Node(eb, ch)],
                    None => ids[&Pend::Other(eb, cl)],
                },
                Pend::Other(eb, _) => ids[&Pend::Other(eb, cl)],
            };
            b.add_arc(
                from,
                Input::Tok(model.char_ix[&m]),
                Output::Eps,
                to,
                &[],
                0,
                0,
            )?;
        }
        let tok = model.tok_ix[&UnitTok { base: eb, stack }];
        b.add_arc(from, Input::Eps, Output::Tok(tok), mid, &[], 0, 0)?;
    }
    Ok(b.finish())
}
