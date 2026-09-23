/// One stage of a supplied-root lookup; an overlay stats row's object index is its phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPhase {
    /// Searching the supplied-root trie for roots matching a shape.
    Search,
    /// A compounding rule's non-head compatibility check on a matched supplied root.
    Gate,
    /// Segmenting a matched supplied root and building its root `Word`.
    Materialize,
}

impl OverlayPhase {
    pub const ALL: [OverlayPhase; 3] = [Self::Search, Self::Gate, Self::Materialize];

    pub fn index(self) -> u32 {
        self as u32
    }

    /// Panics on an index no phase owns: an overlay row outside the three phases is a recorder bug.
    pub fn from_index(index: u32) -> Self {
        *Self::ALL
            .get(index as usize)
            .unwrap_or_else(|| panic!("overlay stats row {index} names no OverlayPhase"))
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Gate => "gate",
            Self::Materialize => "materialize",
        }
    }
}
