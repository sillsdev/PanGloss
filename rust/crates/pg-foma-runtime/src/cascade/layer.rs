//! The layer interface and the backend's own NFA tables (plan 3.1, 2.0).
//!
//! A layer is an eager, immutable table shared between words; everything a word changes lives in a
//! [`LayerScratch`] that the product owns and resets per word (mutation C77). Arcs are symbolic:
//! an arc is labelled with one token or with a token-set id, tested at expansion, so `Sigma*`
//! loops and `Modify` images cost one arc, never `|Sigma|`. No tag lives on a state: a tagged
//! epsilon loop is a self-loop arc (2.0). Nothing here calls foma.

use std::collections::HashMap;

use super::budget::{BuildMeter, BuildRefused};

pub type Token = u32;
pub type StateId = u32;

/// A tag on an arc: `root(entry)`, `nonhead(entry)` or `rule(id)`, packed into one word.
///
/// The two high bits are the kind (`00` root, `01` non-head, `10` rule, `11` never built) and the
/// low 30 bits the id. Use [`Tag::root`], [`Tag::nonhead`] and [`Tag::rule`] to build one and
/// [`Tag::kind`] to read it back; the decoder is total and an unknown word is an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag(pub u32);

/// What a [`Tag`] says, with its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TagKind {
    /// A head root entry.
    Root(u32),
    /// A non-head (compounding) entry: never pinned.
    NonHead(u32),
    Rule(u32),
}

/// A tag word whose kind bits are not any [`TagKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BadTag(pub Tag);

impl Tag {
    /// Ids are 30 bits.
    pub const ID_BITS: u32 = 30;
    const KIND_ROOT: u32 = 0;
    const KIND_NONHEAD: u32 = 1 << Self::ID_BITS;
    const KIND_RULE: u32 = 2 << Self::ID_BITS;

    fn pack(kind: u32, id: u32) -> Tag {
        assert!(id < 1 << Self::ID_BITS, "tag id {id} does not fit 30 bits");
        Tag(kind | id)
    }

    pub fn root(entry: u32) -> Tag {
        Self::pack(Self::KIND_ROOT, entry)
    }

    pub fn nonhead(entry: u32) -> Tag {
        Self::pack(Self::KIND_NONHEAD, entry)
    }

    pub fn rule(id: u32) -> Tag {
        Self::pack(Self::KIND_RULE, id)
    }

    /// The kind and id this tag was built as, or an error for a word nothing builds.
    pub fn kind(self) -> Result<TagKind, BadTag> {
        let id = self.0 & ((1 << Self::ID_BITS) - 1);
        match self.0 & !((1 << Self::ID_BITS) - 1) {
            Self::KIND_ROOT => Ok(TagKind::Root(id)),
            Self::KIND_NONHEAD => Ok(TagKind::NonHead(id)),
            Self::KIND_RULE => Ok(TagKind::Rule(id)),
            _ => Err(BadTag(self)),
        }
    }
}

/// An interned bitset over the alphabet `0..universe`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TokenSet {
    universe: u32,
    words: Vec<u64>,
}

impl TokenSet {
    pub fn new(universe: u32) -> Self {
        TokenSet {
            universe,
            words: vec![0; (universe as usize).div_ceil(64)],
        }
    }

    pub fn full(universe: u32) -> Self {
        let mut s = Self::new(universe);
        for t in 0..universe {
            s.insert(t);
        }
        s
    }

    pub fn from_tokens(universe: u32, tokens: &[Token]) -> Self {
        let mut s = Self::new(universe);
        for &t in tokens {
            s.insert(t);
        }
        s
    }

    pub fn insert(&mut self, t: Token) {
        assert!(t < self.universe, "token outside the alphabet");
        self.words[(t / 64) as usize] |= 1u64 << (t % 64);
    }

    pub fn contains(&self, t: Token) -> bool {
        t < self.universe && self.words[(t / 64) as usize] >> (t % 64) & 1 == 1
    }

    /// Bitset words touched by a membership test or a walk; the step cost of a symbolic arc.
    pub fn word_count(&self) -> u64 {
        self.words.len() as u64
    }

    pub fn iter(&self) -> impl Iterator<Item = Token> + '_ {
        (0..self.universe).filter(move |&t| self.contains(t))
    }

    pub fn covers_all(&self) -> bool {
        (0..self.universe).all(|t| self.contains(t))
    }
}

/// One layer arc as the product sees it. `tags` borrow from the immutable layer.
#[derive(Clone, Copy, Debug)]
pub struct Arc<'a> {
    pub to: StateId,
    /// `None` is an epsilon output (the chain of a move ends here).
    pub output: Option<Token>,
    pub tags: &'a [Tag],
    /// Fixed-point thousandths (S.5).
    pub weight: u32,
    pub edits: u8,
}

/// Per-word mutable state of one layer. Eager layers use none of it; a lazy layer (the bounded copy
/// buffer of 2.4.4) interns its states here. Reset before every word.
///
/// Interning is charged to the product byte budget BEFORE it allocates: the product calls
/// [`LayerScratch::begin`] with the bytes still available, `intern` refuses (allocates nothing,
/// returns `u32::MAX`, sets [`LayerScratch::refused`]) once the allowance would be passed, and the
/// product then ends the word with `ProductBytes` without using the arcs of that call.
#[derive(Debug)]
pub struct LayerScratch {
    pub interned: Vec<Vec<u32>>,
    pub index: HashMap<Vec<u32>, u32>,
    allowance: u64,
    charged: u64,
    refused: bool,
}

impl Default for LayerScratch {
    fn default() -> Self {
        LayerScratch {
            interned: Vec::new(),
            index: HashMap::new(),
            allowance: u64::MAX,
            charged: 0,
            refused: false,
        }
    }
}

impl LayerScratch {
    pub fn reset(&mut self) {
        self.interned.clear();
        self.index.clear();
        self.begin(u64::MAX);
    }

    /// Start a metered call: at most `allowance` bytes may be interned until the next `begin`.
    pub fn begin(&mut self, allowance: u64) {
        self.allowance = allowance;
        self.charged = 0;
        self.refused = false;
    }

    /// The bytes interned since the last `begin` (for the product to add to its meter).
    pub fn take_charged(&mut self) -> u64 {
        std::mem::take(&mut self.charged)
    }

    /// True when an `intern` since the last `begin` was refused for want of bytes.
    pub fn refused(&self) -> bool {
        self.refused
    }

    pub fn intern(&mut self, key: &[u32]) -> u32 {
        if let Some(&id) = self.index.get(key) {
            return id;
        }
        let bytes = super::budget::interned_bytes(key.len());
        let id_space_full = self.interned.len() as u64 >= super::budget::ID_LIMIT;
        if self.refused || id_space_full || self.charged.saturating_add(bytes) > self.allowance {
            self.refused = true;
            return u32::MAX;
        }
        self.charged += bytes;
        let id = self.interned.len() as u32;
        self.interned.push(key.to_vec());
        self.index.insert(key.to_vec(), id);
        id
    }
}

/// Moves are generated downward, so arcs are indexed by INPUT token (3.1).
pub trait Layer: Sync {
    /// The alphabet size `0..universe`; every layer of one product must agree.
    fn universe(&self) -> u32;
    /// Eager layers may sit in the pre-pass suffix; a lazy one may not (3.3).
    fn is_eager(&self) -> bool {
        true
    }
    /// True when some arc carries `edits > 0` (the pre-pass suffix must contain every such layer).
    fn has_edits(&self) -> bool {
        false
    }
    fn start(&self, scratch: &mut LayerScratch) -> StateId;
    fn is_final(&self, scratch: &LayerScratch, s: StateId) -> bool;
    /// Arcs from `s` whose input is `input` (`None` = epsilon input). Returns the steps spent: one
    /// per arc visited, plus one per 64 tokens of a symbolic arc's set that were touched.
    fn arcs_from<'a>(
        &'a self,
        scratch: &mut LayerScratch,
        s: StateId,
        input: Option<Token>,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64;
    /// Arcs from `s` for a FREE input tape: every epsilon-input arc once and, for every token arc
    /// or class arc, one arc per input token that can be read. The pre-pass and the main pass's
    /// top layer both move through it, so it must be complete: a missing arc is a silently lost
    /// word.
    fn arcs_free<'a>(
        &'a self,
        scratch: &mut LayerScratch,
        s: StateId,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Input {
    Eps,
    Tok(Token),
    Class(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Output {
    Eps,
    Tok(Token),
    /// Identity: write the token read (a `Tok`/`Class` input).
    Same,
    /// A `Modify`-style arc: each member of the class maps to its own image (table id).
    Image(u32),
}

const NO_IMAGE: Token = Token::MAX;

#[derive(Clone, Debug)]
struct TableArc {
    input: Input,
    output: Output,
    to: StateId,
    tags: Box<[Tag]>,
    weight: u32,
    edits: u8,
}

#[derive(Clone, Debug, Default)]
struct NState {
    is_final: bool,
    eps: Vec<u32>,
    toks: Vec<(Token, u32)>,
    classes: Vec<u32>,
}

/// An eager NFA layer in the backend's own tables. Never determinised.
#[derive(Clone, Debug)]
pub struct NfaLayer {
    universe: u32,
    start: StateId,
    sets: Vec<TokenSet>,
    images: Vec<Vec<Token>>,
    states: Vec<NState>,
    arcs: Vec<TableArc>,
    has_edits: bool,
}

impl NfaLayer {
    pub fn universe(&self) -> u32 {
        self.universe
    }

    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    pub fn arc_count(&self) -> usize {
        self.arcs.len()
    }

    /// The tags of every arc from `s` to `to` (for tests of tag-set union on duplicate arcs).
    pub fn tags_between(&self, s: StateId, to: StateId) -> Vec<Vec<Tag>> {
        let st = &self.states[s as usize];
        st.eps
            .iter()
            .chain(st.toks.iter().map(|(_, i)| i))
            .chain(st.classes.iter())
            .map(|&i| &self.arcs[i as usize])
            .filter(|a| a.to == to)
            .map(|a| a.tags.to_vec())
            .collect()
    }

    fn output_for(&self, a: &TableArc, t: Token) -> Option<Token> {
        match a.output {
            Output::Eps => None,
            Output::Tok(x) => Some(x),
            Output::Same => Some(t),
            Output::Image(i) => Some(self.images[i as usize][t as usize]),
        }
    }

    fn emit<'a>(&'a self, a: &'a TableArc, t: Token, f: &mut dyn FnMut(Arc<'a>)) {
        f(Arc {
            to: a.to,
            output: self.output_for(a, t),
            tags: &a.tags,
            weight: a.weight,
            edits: a.edits,
        });
    }

    /// Tokens `t` on which `s` has a FAITHFUL x:x arc (no tag, no weight, no edit), as a set.
    fn identity_cover(&self, s: StateId, targets: &mut Vec<StateId>, steps: &mut u64) -> TokenSet {
        let mut cover = TokenSet::new(self.universe);
        let st = &self.states[s as usize];
        let all = st
            .toks
            .iter()
            .map(|(_, i)| *i)
            .chain(st.classes.iter().copied());
        for ai in all {
            let a = &self.arcs[ai as usize];
            *steps += 1;
            if !a.tags.is_empty() || a.weight != 0 || a.edits != 0 {
                continue;
            }
            let mut any = false;
            match a.input {
                Input::Eps => {}
                Input::Tok(t) => {
                    if self.output_for(a, t) == Some(t) {
                        cover.insert(t);
                        any = true;
                    }
                }
                Input::Class(c) => {
                    let set = &self.sets[c as usize];
                    *steps += set.word_count();
                    for t in set.iter() {
                        if self.output_for(a, t) == Some(t) {
                            cover.insert(t);
                            any = true;
                        }
                    }
                }
            }
            if any {
                targets.push(a.to);
            }
        }
        cover
    }

    /// The linear SUFFICIENT identity test of 3.1: the sub-automaton of faithful x:x arcs, walked as
    /// an automaton over the alphabet from the start, has every reachable state final and total.
    /// Steps are charged to `meter`; exhausting `BuildSteps` is a refusal, never a pass (C76).
    pub fn identity_test(&self, meter: &mut BuildMeter) -> Result<bool, BuildRefused> {
        let mut seen = vec![false; self.states.len()];
        let mut stack = vec![self.start];
        seen[self.start as usize] = true;
        while let Some(s) = stack.pop() {
            let mut steps = 1;
            let mut targets = Vec::new();
            let cover = self.identity_cover(s, &mut targets, &mut steps);
            meter.charge_steps(steps, "identity test")?;
            if !self.states[s as usize].is_final || !cover.covers_all() {
                return Ok(false);
            }
            for t in targets {
                if !seen[t as usize] {
                    seen[t as usize] = true;
                    stack.push(t);
                }
            }
        }
        Ok(true)
    }
}

impl Layer for NfaLayer {
    fn universe(&self) -> u32 {
        self.universe
    }

    fn has_edits(&self) -> bool {
        self.has_edits
    }

    fn start(&self, _: &mut LayerScratch) -> StateId {
        self.start
    }

    fn is_final(&self, _: &LayerScratch, s: StateId) -> bool {
        self.states[s as usize].is_final
    }

    fn arcs_from<'a>(
        &'a self,
        _: &mut LayerScratch,
        s: StateId,
        input: Option<Token>,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64 {
        let st = &self.states[s as usize];
        let mut steps = 0;
        match input {
            None => {
                for &ai in &st.eps {
                    steps += 1;
                    self.emit(&self.arcs[ai as usize], 0, f);
                }
            }
            Some(t) => {
                let lo = st.toks.partition_point(|&(k, _)| k < t);
                for &(k, ai) in &st.toks[lo..] {
                    if k != t {
                        break;
                    }
                    steps += 1;
                    self.emit(&self.arcs[ai as usize], t, f);
                }
                for &ai in &st.classes {
                    let a = &self.arcs[ai as usize];
                    let Input::Class(c) = a.input else { continue };
                    let set = &self.sets[c as usize];
                    steps += 1 + set.word_count();
                    if set.contains(t) {
                        self.emit(a, t, f);
                    }
                }
            }
        }
        steps
    }

    fn arcs_free<'a>(
        &'a self,
        _: &mut LayerScratch,
        s: StateId,
        f: &mut dyn FnMut(Arc<'a>),
    ) -> u64 {
        let st = &self.states[s as usize];
        let mut steps = 0;
        for &ai in &st.eps {
            steps += 1;
            self.emit(&self.arcs[ai as usize], 0, f);
        }
        for &(t, ai) in &st.toks {
            steps += 1;
            self.emit(&self.arcs[ai as usize], t, f);
        }
        for &ai in &st.classes {
            let a = &self.arcs[ai as usize];
            let Input::Class(c) = a.input else { continue };
            let set = &self.sets[c as usize];
            steps += 1 + set.word_count();
            match a.output {
                Output::Same | Output::Image(_) => {
                    for t in set.iter() {
                        self.emit(a, t, f);
                    }
                }
                // The output does not depend on the token read: one arc stands for the whole class.
                Output::Eps | Output::Tok(_) => {
                    // an empty class reads nothing: no arc
                    if set.iter().next().is_some() {
                        self.emit(a, 0, f)
                    }
                }
            }
        }
        steps
    }
}

/// Builds an [`NfaLayer`] under a [`BuildMeter`]: every state, arc, set and image is charged BEFORE
/// it is allocated (C50), and an arc identical to an earlier one except for its tags is stored once
/// with the union of the tag sets.
pub struct NfaBuilder<'m> {
    layer: NfaLayer,
    meter: &'m mut BuildMeter,
    name: String,
    dup: HashMap<(StateId, Input, Output, StateId, u32, u8), u32>,
}

const STATE_BUILD_BYTES: u64 = 64;
const ARC_BUILD_BYTES: u64 = 48;

impl<'m> NfaBuilder<'m> {
    pub fn new(name: &str, universe: u32, meter: &'m mut BuildMeter) -> Self {
        NfaBuilder {
            layer: NfaLayer {
                universe,
                start: 0,
                sets: Vec::new(),
                images: Vec::new(),
                states: Vec::new(),
                arcs: Vec::new(),
                has_edits: false,
            },
            meter,
            name: name.to_string(),
            dup: HashMap::new(),
        }
    }

    pub fn add_state(&mut self, is_final: bool) -> Result<StateId, BuildRefused> {
        self.meter.charge_bytes(STATE_BUILD_BYTES, &self.name)?;
        self.layer.states.push(NState {
            is_final,
            ..NState::default()
        });
        Ok((self.layer.states.len() - 1) as StateId)
    }

    pub fn set_start(&mut self, s: StateId) {
        self.layer.start = s;
    }

    pub fn add_set(&mut self, set: TokenSet) -> Result<u32, BuildRefused> {
        self.meter
            .charge_bytes(24 + 8 * set.word_count(), &self.name)?;
        self.layer.sets.push(set);
        Ok((self.layer.sets.len() - 1) as u32)
    }

    /// `image[t]` is the image of member `t`; non-members hold `Token::MAX`.
    pub fn add_image(&mut self, image: Vec<Token>) -> Result<u32, BuildRefused> {
        assert_eq!(image.len(), self.layer.universe as usize);
        self.meter
            .charge_bytes(24 + 4 * image.len() as u64, &self.name)?;
        self.layer.images.push(image);
        Ok((self.layer.images.len() - 1) as u32)
    }

    pub fn no_image() -> Token {
        NO_IMAGE
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_arc(
        &mut self,
        from: StateId,
        input: Input,
        output: Output,
        to: StateId,
        tags: &[Tag],
        weight: u32,
        edits: u8,
    ) -> Result<(), BuildRefused> {
        assert!(
            !(input == Input::Eps && matches!(output, Output::Same | Output::Image(_))),
            "an epsilon-input arc has no token to copy"
        );
        assert!(
            !matches!(output, Output::Image(_)) || matches!(input, Input::Class(_)),
            "an image function needs a class input"
        );
        let universe = self.layer.universe;
        for t in [
            match input {
                Input::Tok(t) => Some(t),
                _ => None,
            },
            match output {
                Output::Tok(t) => Some(t),
                _ => None,
            },
        ]
        .into_iter()
        .flatten()
        {
            assert!(
                t < universe,
                "layer {}: arc {from} -> {to} names token {t} outside the alphabet 0..{universe}",
                self.name
            );
        }
        let mut tags: Vec<Tag> = tags.to_vec();
        tags.sort();
        tags.dedup();
        let key = (from, input, output, to, weight, edits);
        if let Some(&ai) = self.dup.get(&key) {
            let merged: Vec<Tag> = {
                let mut m = self.layer.arcs[ai as usize].tags.to_vec();
                m.extend_from_slice(&tags);
                m.sort();
                m.dedup();
                m
            };
            let extra = 4 * merged
                .len()
                .saturating_sub(self.layer.arcs[ai as usize].tags.len())
                as u64;
            self.meter.charge_bytes(extra, &self.name)?;
            self.layer.arcs[ai as usize].tags = merged.into_boxed_slice();
            return Ok(());
        }
        self.meter
            .charge_bytes(ARC_BUILD_BYTES + 4 * tags.len() as u64 + 16, &self.name)?;
        let ai = self.layer.arcs.len() as u32;
        self.layer.arcs.push(TableArc {
            input,
            output,
            to,
            tags: tags.into_boxed_slice(),
            weight,
            edits,
        });
        self.dup.insert(key, ai);
        self.layer.has_edits |= edits > 0;
        let st = &mut self.layer.states[from as usize];
        match input {
            Input::Eps => st.eps.push(ai),
            Input::Tok(t) => {
                let at = st.toks.partition_point(|&(k, _)| k <= t);
                st.toks.insert(at, (t, ai));
            }
            Input::Class(_) => st.classes.push(ai),
        }
        Ok(())
    }

    pub fn finish(self) -> NfaLayer {
        self.layer
    }
}

/// The identity on `Sigma*`: one final state, one class arc.
pub fn identity_layer(universe: u32, meter: &mut BuildMeter) -> Result<NfaLayer, BuildRefused> {
    let mut b = NfaBuilder::new("identity", universe, meter);
    let s = b.add_state(true)?;
    let all = b.add_set(TokenSet::full(universe))?;
    b.add_arc(s, Input::Class(all), Output::Same, s, &[], 0, 0)?;
    Ok(b.finish())
}

/// Identity test and wrap (3.1, S.4 d). Every layer must contain the identity from its start. A net
/// that passes the sufficient test is used as it is. A net that fails it (every obligatory rewrite
/// rule) is used as it is when the stratum selector `sigma` runs it (`runs_net`), and is replaced by
/// the identity layer when the selector puts it in `Id` mode: the step between the two is the
/// selector's, not a free epsilon choice per layer, so it adds no start tuples.
pub fn prepare_layer(
    net: NfaLayer,
    runs_net: bool,
    meter: &mut BuildMeter,
) -> Result<NfaLayer, BuildRefused> {
    let has_identity = net.identity_test(meter)?;
    if has_identity || runs_net {
        Ok(net)
    } else {
        identity_layer(net.universe, meter)
    }
}
