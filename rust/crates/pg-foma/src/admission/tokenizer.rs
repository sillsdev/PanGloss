//! Splits a word into the net symbols a rule table reads.

use super::acceptor::OTHER;
use foma::types::{Fsm, IDENTITY};
use foma::utf8::is_combining;

struct TrieNode {
    symbol: u16,
    next: Vec<(char, u32)>,
}

/// Splits text into net symbols the way foma's apply does: longest symbol first, and a combining mark turns the token into `OTHER`.
pub(super) struct Tokenizer {
    nodes: Vec<TrieNode>,
}

impl Tokenizer {
    pub(super) fn new(net: &Fsm) -> Self {
        let mut nodes = vec![TrieNode {
            symbol: 0,
            next: Vec::new(),
        }];
        for entry in net.sigma.iter().filter(|s| s.number > IDENTITY) {
            let mut at = 0usize;
            for c in entry.symbol.chars() {
                at = match nodes[at].next.iter().find(|(k, _)| *k == c) {
                    Some((_, child)) => *child as usize,
                    None => {
                        nodes.push(TrieNode {
                            symbol: 0,
                            next: Vec::new(),
                        });
                        let child = nodes.len() - 1;
                        nodes[at].next.push((c, child as u32));
                        child
                    }
                };
            }
            nodes[at].symbol = entry.number as u16;
        }
        Self { nodes }
    }

    /// The longest symbol at the start of `text` and its length in bytes; symbol 0 when none matches.
    fn longest(&self, text: &str) -> (u16, usize) {
        let mut best = (0u16, 0usize);
        let mut at = 0usize;
        for (index, c) in text.char_indices() {
            let Some((_, child)) = self.nodes[at].next.iter().find(|(k, _)| *k == c) else {
                break;
            };
            at = *child as usize;
            if self.nodes[at].symbol != 0 {
                best = (self.nodes[at].symbol, index + c.len_utf8());
            }
        }
        best
    }

    pub(super) fn tokens(&self, text: &str) -> Vec<u16> {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0usize;
        while at < text.len() {
            let (mut symbol, mut used) = self.longest(&text[at..]);
            if symbol == 0 {
                symbol = OTHER;
                used = text[at..].chars().next().map_or(1, char::len_utf8);
            }
            loop {
                let mark = is_combining(&bytes[at + used..]);
                if mark == 0 {
                    break;
                }
                symbol = OTHER;
                used += mark as usize;
            }
            out.push(symbol);
            at += used;
        }
        out
    }
}
