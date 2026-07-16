//! foma/stack.c — literal Wave-2 (bug-for-bug) port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/stack.md
//! (per-file `stack.*` ids) plus the foma.h prototype ids (`foma.stack-*`)
//! carried at each Rust site.
//!
//! Representation (all observably equivalent to C):
//!  - The CLI network stack is a sentinel-terminated doubly-linked list of
//!    `struct stack_entry {number, ah, amedh, fsm, next, previous}` whose head
//!    is the file-static `struct stack_entry *main_stack`. The list always ends
//!    in a sentinel (number == -1, fsm == NULL, next == NULL). Real entries sit
//!    between head and sentinel: head == bottom, entry just before the sentinel
//!    == top (most recently pushed).
//!  - The file-static `main_stack` becomes `Session.stack_head`, the head's arena
//!    index. Threading `&mut Session` (see session.rs) replaces the former
//!    thread_local so independent sessions no longer share one stack.
//!  - malloc'd `stack_entry` nodes live in `Session.stack_arena` (a Vec). Node
//!    pointers (`main_stack`, `next`, `previous`, and every returned
//!    `struct stack_entry *`) become arena indices (`usize`); None ↔ NULL.
//!    DEVIATION from C: `free()` cannot release a slot that other indices may
//!    still name, so freed nodes are left in the arena (leaked, memory-safe).
//!    The arena also grows across stack_init cycles (each re-init pushes a fresh
//!    sentinel and abandons the old list, mirroring C's leak-on-reinit).
//!  - stack_get_ah / stack_get_med_ah: C returns `se->ah` / `se->amedh` (a
//!    borrowed handle pointer). DEVIATION from C: the cached handle lives inside
//!    an arena entry and cannot be handed out as a safe borrow, so the Rust twin
//!    lazily creates + caches the handle exactly as C does, then returns the
//!    owning entry's arena index (the handle is reachable as that entry's `ah` /
//!    `amedh` field). NULL top (empty stack) ↔ None.

use crate::apply::{apply_clear, apply_init};
use crate::constructions::fsm_count;
use crate::iface::print_stats;
use crate::options::FomaOptions;
use crate::session::Session;
use crate::spelling::{apply_med_clear, apply_med_init, apply_med_set_align_symbol};
use crate::structures::fsm_destroy;
use crate::types::{ApplyHandle, ApplyMedHandle, Fsm, StackEntry};

/// Outcome of an in-place stack reorder (`stack_turn` / `stack_rotate`):
/// whether there was anything on the stack to reorder. Replaces C's status
/// ints (`stack_turn` returned 0/1, `stack_rotate` -1/1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackReorder {
    /// The stack was empty — nothing to reorder.
    Empty,
    /// The stack was non-empty and the reorder applied (a single element is a
    /// trivial no-op that still reports Reordered, as in C).
    Reordered,
}

impl Session {
    /* -------------------------------------------------------------- */
    /* Arena / head helpers (pointer ops become index ops)            */
    /* -------------------------------------------------------------- */

    /// malloc(sizeof(struct stack_entry)) — push a node, return its index.
    fn arena_alloc(&mut self, entry: StackEntry) -> usize {
        self.stack_arena.push(entry);
        self.stack_arena.len() - 1
    }

    /// malloc a fresh sentinel node {number = -1, fsm/ah/amedh/next NULL}. Only
    /// the terminating sentinel carries these defaults; `previous` links it to
    /// whatever precedes it (None on a fresh empty stack).
    fn arena_alloc_sentinel(&mut self, previous: Option<usize>) -> usize {
        self.arena_alloc(StackEntry {
            number: -1,
            ah: None,
            amedh: None,
            fsm: None,
            next: None,
            previous,
        })
    }

    /// Walk from main_stack to the top real entry (the one whose `next` is the
    /// sentinel). Requires a non-empty stack: on an empty stack e_next(sentinel)
    /// is None → unwrap panics. DEVIATION from C: the C walk dereferences the
    /// sentinel's NULL `next` here (UB/crash); the port panics instead.
    fn walk_to_top(&self) -> usize {
        let mut stack_ptr = self.main_stack();
        // On a non-empty stack every non-sentinel entry has a `next`; on an empty
        // stack this reads the sentinel's None `next` and panics (DEVIATION pin).
        while self
            .e_next(stack_ptr)
            .map(|n| self.e_number(n))
            .expect("walk_to_top on empty stack (sentinel has no next)")
            != -1
        {
            stack_ptr = self
                .e_next(stack_ptr)
                .expect("non-sentinel entry always has a next");
        }
        stack_ptr
    }

    /// Read `main_stack`. DEVIATION from C: unwrap panics if the stack was never
    /// initialised (C would dereference a NULL/garbage global and crash).
    /// `Session::new()` always initialises it, so this holds by construction.
    fn main_stack(&self) -> usize {
        self.stack_head
            .expect("main_stack uninitialized; call stack_init() first")
    }

    fn set_main_stack(&mut self, i: usize) {
        self.stack_head = Some(i);
    }

    fn e_number(&self, i: usize) -> i32 {
        self.stack_arena[i].number
    }
    fn e_next(&self, i: usize) -> Option<usize> {
        self.stack_arena[i].next
    }
    fn e_previous(&self, i: usize) -> Option<usize> {
        self.stack_arena[i].previous
    }
    fn set_next(&mut self, i: usize, v: Option<usize>) {
        self.stack_arena[i].next = v;
    }
    fn set_previous(&mut self, i: usize, v: Option<usize>) {
        self.stack_arena[i].previous = v;
    }
    fn take_fsm(&mut self, i: usize) -> Option<Box<Fsm>> {
        self.stack_arena[i].fsm.take()
    }
    fn take_ah(&mut self, i: usize) -> Option<Box<ApplyHandle>> {
        self.stack_arena[i].ah.take()
    }
    fn take_amedh(&mut self, i: usize) -> Option<Box<ApplyMedHandle>> {
        self.stack_arena[i].amedh.take()
    }

    /* -------------------------------------------------------------- */
    /* Entry-field accessors for the iface layer                      */
    /* -------------------------------------------------------------- */
    // DEVIATION from C: in C the caller dereferences a `struct stack_entry *`
    // directly (e.g. `stack_find_top()->fsm`, `apply_down(stack_get_ah(), ...)`).
    // Here those pointers are arena indices (see module notes) and the fsm/ah/
    // amedh live inside the private `stack_arena`, so they cannot be handed out
    // as a `&mut`. These closure accessors let iface.c's twin operate on the
    // entry-owned fsm / apply handle / med handle by index. No C counterpart, no
    // spec ids — plumbing, like the private helpers above.

    /// Run `f` on the fsm owned by the entry at `index` (C: `entry->fsm`).
    pub fn stack_entry_fsm<R>(&mut self, index: usize, f: impl FnOnce(&mut Fsm) -> R) -> R {
        f(self.stack_arena[index]
            .fsm
            .as_deref_mut()
            .expect("stack entry has no fsm"))
    }

    /// Like `stack_entry_fsm`, but also lends the session options — for library
    /// calls needing both (`f(&opts, &mut fsm)`), which a `&session.opts` borrow
    /// inside a `stack_entry_fsm` closure would otherwise conflict with.
    pub fn stack_entry_fsm_with_opts<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&FomaOptions, &mut Fsm) -> R,
    ) -> R {
        f(
            &self.opts,
            self.stack_arena[index]
                .fsm
                .as_deref_mut()
                .expect("stack entry has no fsm"),
        )
    }

    /// Run `f` on the apply handle owned by the entry at `index` (C: `entry->ah`).
    pub fn stack_entry_ah<R>(&mut self, index: usize, f: impl FnOnce(&mut ApplyHandle) -> R) -> R {
        f(self.stack_arena[index]
            .ah
            .as_deref_mut()
            .expect("stack entry has no ah"))
    }

    /// Like `stack_entry_ah`, but also lends the session options (see
    /// `stack_entry_fsm_with_opts`).
    pub fn stack_entry_ah_with_opts<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&FomaOptions, &mut ApplyHandle) -> R,
    ) -> R {
        f(
            &self.opts,
            self.stack_arena[index]
                .ah
                .as_deref_mut()
                .expect("stack entry has no ah"),
        )
    }

    /// Like `stack_entry_amedh`, but also lends the session options (see
    /// `stack_entry_fsm_with_opts`).
    pub fn stack_entry_amedh_with_opts<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&FomaOptions, &mut ApplyMedHandle) -> R,
    ) -> R {
        f(
            &self.opts,
            self.stack_arena[index]
                .amedh
                .as_deref_mut()
                .expect("stack entry has no amedh"),
        )
    }

    /// Read the `next` pointer of the entry at `index` (C: `entry->next`), so the
    /// iface layer can walk the list (e.g. iface_save_stack). Sentinel/NULL ↔ None.
    pub fn stack_entry_next(&self, index: usize) -> Option<usize> {
        self.e_next(index)
    }

    /// Run `f` on the med handle owned by the entry at `index` (C: `entry->amedh`).
    pub fn stack_entry_amedh<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut ApplyMedHandle) -> R,
    ) -> R {
        f(self.stack_arena[index]
            .amedh
            .as_deref_mut()
            .expect("stack entry has no amedh"))
    }

    /* -------------------------------------------------------------- */

    // [spec:foma:def:stack.stack-size-fn]
    // [spec:foma:sem:stack.stack-size-fn]
    // [spec:foma:def:foma.stack-size-fn]
    // [spec:foma:sem:foma.stack-size-fn]
    pub fn stack_size(&self) -> i32 {
        let mut i = 0;
        let mut stack_ptr = self.main_stack();
        while let Some(next) = self.e_next(stack_ptr) {
            stack_ptr = next;
            i += 1;
        }
        i
    }

    // [spec:foma:def:stack.stack-init-fn]
    // [spec:foma:sem:stack.stack-init-fn+1]
    // [spec:foma:def:foma.stack-init-fn]
    // [spec:foma:sem:foma.stack-init-fn+1]
    pub fn stack_init(&mut self) {
        // malloc a fresh sentinel {number = -1, fsm = NULL, next = NULL,
        // previous = NULL} (ah/amedh left uninitialized in C; None here — never
        // read on the sentinel). Does not free any previous list (leaks, as in C).
        let idx = self.arena_alloc_sentinel(None);
        self.set_main_stack(idx);
    }

    // [spec:foma:def:stack.stack-add-fn]
    // [spec:foma:sem:stack.stack-add-fn]
    // [spec:foma:def:foma.stack-add-fn]
    // [spec:foma:sem:foma.stack-add-fn]
    pub fn stack_add(&mut self, mut fsm: Box<Fsm>) -> i32 {
        let mut i = 0;
        let mut stack_ptr_previous: Option<usize> = None;

        fsm_count(&mut fsm);
        if fsm.name.is_empty() {
            // sprintf(fsm->name, "%X", rand()) — uppercase hex of rand() into the
            // fixed 40-byte name buffer (%X of a 32-bit value is <= 8 chars).
            fsm.name = format!("{:X}", self.lcg.rand() as u32).into();
        }
        let mut stack_ptr = self.main_stack();
        while self.e_number(stack_ptr) != -1 {
            stack_ptr_previous = Some(stack_ptr);
            stack_ptr = self
                .e_next(stack_ptr)
                .expect("non-sentinel entry always has a next");
            i += 1;
        }
        // Allocate the fresh sentinel that becomes stack_ptr->next; its number =
        // -1, fsm = NULL, next = NULL, previous = stack_ptr.
        let new_sentinel = self.arena_alloc_sentinel(Some(stack_ptr));
        // Convert the old sentinel (stack_ptr) into the new top entry, in C order.
        self.stack_arena[stack_ptr].next = Some(new_sentinel);
        self.stack_arena[stack_ptr].fsm = Some(fsm);
        self.stack_arena[stack_ptr].ah = None;
        self.stack_arena[stack_ptr].amedh = None;
        self.stack_arena[stack_ptr].number = i;
        self.stack_arena[stack_ptr].previous = stack_ptr_previous;
        if self.opts.verbose {
            print_stats(
                self.stack_arena[stack_ptr]
                    .fsm
                    .as_deref()
                    .expect("fsm just set on this entry above"),
            );
        }
        self.e_number(stack_ptr)
    }

    // [spec:foma:def:stack.stack-get-med-ah-fn]
    // [spec:foma:sem:stack.stack-get-med-ah-fn]
    // [spec:foma:def:foma.stack-get-med-ah-fn]
    // [spec:foma:sem:foma.stack-get-med-ah-fn]
    pub fn stack_get_med_ah(&mut self) -> Option<usize> {
        let se = self.stack_find_top()?;
        if self.stack_arena[se].amedh.is_none() {
            // se->amedh = apply_med_init(se->fsm);
            let mut amedh = apply_med_init(
                self.stack_arena[se]
                    .fsm
                    .as_deref()
                    .expect("top real entry always carries an fsm"),
            );
            apply_med_set_align_symbol(&mut amedh, "-");
            self.stack_arena[se].amedh = Some(amedh);
        }
        // C: return se->amedh; here the owning entry index (see module notes).
        Some(se)
    }

    // [spec:foma:def:stack.stack-get-ah-fn]
    // [spec:foma:sem:stack.stack-get-ah-fn]
    // [spec:foma:def:foma.stack-get-ah-fn]
    // [spec:foma:sem:foma.stack-get-ah-fn]
    pub fn stack_get_ah(&mut self) -> Option<usize> {
        let se = self.stack_find_top()?;
        if self.stack_arena[se].ah.is_none() {
            // se->ah = apply_init(se->fsm);
            let ah = apply_init(
                self.stack_arena[se]
                    .fsm
                    .as_deref()
                    .expect("top real entry always carries an fsm"),
            );
            self.stack_arena[se].ah = Some(ah);
        }
        // C: return se->ah; here the owning entry index (see module notes).
        Some(se)
    }

    // [spec:foma:def:stack.stack-pop-fn]
    // [spec:foma:sem:stack.stack-pop-fn]
    // [spec:foma:def:foma.stack-pop-fn]
    // [spec:foma:sem:foma.stack-pop-fn]
    pub fn stack_pop(&mut self) -> Option<Box<Fsm>> {
        if self.stack_size() == 1 {
            // fsm = main_stack->fsm; main_stack->fsm = NULL; stack_clear();
            let fsm = self.take_fsm(self.main_stack());
            self.stack_clear();
            return fsm;
        }
        // Walk to the top entry (its next is the sentinel). No empty-stack guard:
        // walk_to_top panics on an empty stack (DEVIATION: C's UB null-deref).
        let stack_ptr = self.walk_to_top();
        // (stack_ptr->previous)->next = stack_ptr->next;
        // (stack_ptr->next)->previous = stack_ptr->previous;
        // Size >= 2 here (the size == 1 fast path returned above): the top entry
        // has a real predecessor and its `next` is the sentinel — both present.
        let prev = self
            .e_previous(stack_ptr)
            .expect("top entry of a >=2 stack has a previous");
        let nxt = self
            .e_next(stack_ptr)
            .expect("top entry always has a next (the sentinel)");
        self.set_next(prev, Some(nxt));
        self.set_previous(nxt, Some(prev));
        let fsm = self.take_fsm(stack_ptr);
        let ah = self.take_ah(stack_ptr);
        if let Some(ah) = ah {
            apply_clear(ah);
        }
        let amedh = self.take_amedh(stack_ptr);
        if amedh.is_some() {
            apply_med_clear(amedh);
        }
        // stack_ptr->fsm = NULL (done by take_fsm); free(stack_ptr): slot leaked.
        fsm
    }

    // [spec:foma:def:stack.stack-isempty-fn]
    // [spec:foma:sem:stack.stack-isempty-fn]
    // [spec:foma:def:foma.stack-isempty-fn]
    // [spec:foma:sem:foma.stack-isempty-fn]
    pub fn stack_isempty(&self) -> bool {
        self.e_next(self.main_stack()).is_none()
    }

    // [spec:foma:def:stack.stack-turn-fn]
    // [spec:foma:sem:stack.stack-turn-fn+1]
    // [spec:foma:def:foma.stack-turn-fn]
    // [spec:foma:sem:foma.stack-turn-fn+1]
    pub fn stack_turn(&mut self) -> StackReorder {
        // Wave 4 fix: the C reversal's final previous-link fix-up loop never
        // advanced its cursor, so on any stack of >= 2 entries it spun forever
        // (dead code — "turn stack" reaches iface_turn → stack_rotate, never here).
        // Implement the evident intent: reverse the order of the real entries in
        // place, relinking next/previous correctly and leaving the sentinel at the
        // tail. Each entry travels with its own fsm/ah/amedh/number (numbers are
        // not renumbered, matching the C code's evident intent), so afterwards the
        // former top is the new bottom and the former bottom is the new top.
        if self.stack_isempty() {
            tracing::info!("Stack is empty.");
            return StackReorder::Empty;
        }
        if self.stack_size() == 1 {
            return StackReorder::Reordered;
        }

        // Collect the real entries bottom -> top, stopping at the sentinel.
        let mut entries = Vec::new();
        let mut stack_ptr = self.main_stack();
        while self.e_number(stack_ptr) != -1 {
            entries.push(stack_ptr);
            stack_ptr = self
                .e_next(stack_ptr)
                .expect("non-sentinel entry always has a next");
        }
        let sentinel = stack_ptr;

        // Relink in reversed order: new head = old top, ..., new top = old bottom.
        entries.reverse();
        self.set_main_stack(entries[0]);
        self.set_previous(entries[0], None);
        for pair in entries.windows(2) {
            self.set_next(pair[0], Some(pair[1]));
            self.set_previous(pair[1], Some(pair[0]));
        }
        let new_top = *entries
            .last()
            .expect("size >= 2 here, so entries is non-empty");
        self.set_next(new_top, Some(sentinel));
        self.set_previous(sentinel, Some(new_top));
        StackReorder::Reordered
    }

    // [spec:foma:def:stack.stack-find-top-fn]
    // [spec:foma:sem:stack.stack-find-top-fn]
    // [spec:foma:def:foma.stack-find-top-fn]
    // [spec:foma:sem:foma.stack-find-top-fn]
    pub fn stack_find_top(&self) -> Option<usize> {
        if self.e_number(self.main_stack()) == -1 {
            return None;
        }
        Some(self.walk_to_top())
    }

    // [spec:foma:def:stack.stack-find-bottom-fn]
    // [spec:foma:sem:stack.stack-find-bottom-fn]
    // [spec:foma:def:foma.stack-find-bottom-fn]
    // [spec:foma:sem:foma.stack-find-bottom-fn]
    pub fn stack_find_bottom(&self) -> Option<usize> {
        if self.e_number(self.main_stack()) == -1 {
            return None;
        }
        Some(self.main_stack())
    }

    // [spec:foma:def:stack.stack-find-second-fn]
    // [spec:foma:sem:stack.stack-find-second-fn]
    // [spec:foma:def:foma.stack-find-second-fn]
    // [spec:foma:sem:foma.stack-find-second-fn]
    pub fn stack_find_second(&self) -> Option<usize> {
        // C's empty-stack guard is commented out, so walk_to_top runs uncondition-
        // ally and panics on an empty stack (DEVIATION: C's UB null-deref of the
        // sentinel).
        self.e_previous(self.walk_to_top())
    }

    // [spec:foma:def:stack.stack-clear-fn]
    // [spec:foma:sem:stack.stack-clear-fn+1]
    // [spec:foma:def:foma.stack-clear-fn]
    // [spec:foma:sem:foma.stack-clear-fn+1]
    pub fn stack_clear(&mut self) {
        let mut stack_ptr = self.main_stack();
        while let Some(next) = self.e_next(stack_ptr) {
            let ah = self.take_ah(stack_ptr);
            if let Some(ah) = ah {
                apply_clear(ah);
            }
            let amedh = self.take_amedh(stack_ptr);
            if amedh.is_some() {
                apply_med_clear(amedh);
            }
            self.set_main_stack(next);
            let fsm = self.take_fsm(stack_ptr);
            if let Some(fsm) = fsm {
                // fsm_destroy(NULL) is a safe no-op in C; the None case is the guard.
                fsm_destroy(fsm);
            }
            // free(stack_ptr): slot leaked (memory-safe).
            stack_ptr = self.main_stack();
        }
        // free(stack_ptr): trailing sentinel — slot leaked.
        self.stack_init()
    }

    // [spec:foma:def:stack.stack-rotate-fn]
    // [spec:foma:sem:stack.stack-rotate-fn+1]
    // [spec:foma:def:foma.stack-rotate-fn]
    // [spec:foma:sem:foma.stack-rotate-fn+1]
    pub fn stack_rotate(&mut self) -> StackReorder {
        /* Top element of stack to bottom */
        if self.stack_isempty() {
            tracing::info!("Stack is empty.");
            return StackReorder::Empty;
        }
        if self.stack_size() == 1 {
            return StackReorder::Reordered;
        }
        let stack_ptr = self
            .stack_find_top()
            .expect("non-empty stack has a top (guarded above)");
        let ms = self.main_stack();
        // [spec:foma:sem:stack.stack-rotate-fn+1] swap the cached apply/med handles
        // (ah/amedh) together with the fsm, so each handle stays bound to its own
        // net. C swapped only ->fsm, leaving cached handles pointing at the other
        // entry's former net — subsequent apply/med ran against the wrong net.
        let temp_fsm = self.stack_arena[ms].fsm.take();
        self.stack_arena[ms].fsm = self.stack_arena[stack_ptr].fsm.take();
        self.stack_arena[stack_ptr].fsm = temp_fsm;
        let temp_ah = self.stack_arena[ms].ah.take();
        self.stack_arena[ms].ah = self.stack_arena[stack_ptr].ah.take();
        self.stack_arena[stack_ptr].ah = temp_ah;
        let temp_amedh = self.stack_arena[ms].amedh.take();
        self.stack_arena[ms].amedh = self.stack_arena[stack_ptr].amedh.take();
        self.stack_arena[stack_ptr].amedh = temp_amedh;
        StackReorder::Reordered
    }

    // [spec:foma:def:stack.stack-print-fn]
    // [spec:foma:sem:stack.stack-print-fn+1]
    // [spec:foma:def:foma.stack-print-fn]
    // [spec:foma:sem:foma.stack-print-fn+1]
    pub fn stack_print(&self) {
        // No-op stub: reads/writes no state, prints nothing.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructions::fsm_symbol;

    /// Push a symbol net with a caller-chosen name (fsm_symbol leaves name "").
    fn add_named(session: &mut Session, sym: &str, name: &str) -> i32 {
        let mut f = fsm_symbol(sym);
        f.name = name.into();
        session.stack_add(f)
    }

    fn top_fsm_name(session: &mut Session) -> String {
        let top = session.stack_find_top().unwrap();
        session.stack_entry_fsm(top, |f| f.name.clone()).to_string()
    }

    fn bottom_fsm_name(session: &mut Session) -> String {
        let bottom = session.stack_find_bottom().unwrap();
        session
            .stack_entry_fsm(bottom, |f| f.name.clone())
            .to_string()
    }

    // [spec:foma:sem:stack.stack-init-fn+1/test]
    // [spec:foma:sem:foma.stack-init-fn+1/test]
    #[test]
    fn stack_init_creates_fresh_empty_sentinel() {
        let mut session = Session::new();
        // Head is the sentinel: number == -1, no next, no previous, no fsm.
        let head = session.main_stack();
        assert_eq!(session.e_number(head), -1);
        assert_eq!(session.e_next(head), None);
        assert_eq!(session.e_previous(head), None);
        assert!(session.stack_isempty());
        assert_eq!(session.stack_size(), 0);
        // Re-init on a populated stack abandons the old list (leak, as in C)
        // and starts empty again.
        add_named(&mut session, "a", "old");
        session.stack_init();
        assert_eq!(session.stack_size(), 0);
        assert!(session.stack_isempty());
    }

    // [spec:foma:sem:stack.stack-isempty-fn/test]
    // [spec:foma:sem:foma.stack-isempty-fn/test]
    #[test]
    fn stack_isempty_is_1_iff_no_real_entries() {
        let mut session = Session::new();
        assert!(session.stack_isempty());
        add_named(&mut session, "a", "x");
        assert!(!(session.stack_isempty()));
        session.stack_clear();
        assert!(session.stack_isempty());
    }

    // [spec:foma:sem:stack.stack-size-fn/test]
    // [spec:foma:sem:foma.stack-size-fn/test]
    #[test]
    fn stack_size_counts_real_entries() {
        let mut session = Session::new();
        assert_eq!(session.stack_size(), 0);
        add_named(&mut session, "a", "one");
        assert_eq!(session.stack_size(), 1);
        add_named(&mut session, "b", "two");
        assert_eq!(session.stack_size(), 2);
        session.stack_pop();
        assert_eq!(session.stack_size(), 1);
    }

    // [spec:foma:sem:stack.stack-add-fn/test]
    // [spec:foma:sem:foma.stack-add-fn/test]
    #[test]
    fn stack_add_appends_numbers_names_and_counts() {
        let mut session = Session::new();
        // Return value is the new entry's number == stack size before the push.
        assert_eq!(add_named(&mut session, "a", "first"), 0);
        assert_eq!(add_named(&mut session, "b", "second"), 1);
        // Entries append at the tail: head == bottom == first pushed,
        // entry before the sentinel == top == most recently pushed.
        assert_eq!(bottom_fsm_name(&mut session), "first");
        assert_eq!(top_fsm_name(&mut session), "second");
        assert_eq!(session.e_number(session.stack_find_bottom().unwrap()), 0);
        assert_eq!(session.e_number(session.stack_find_top().unwrap()), 1);
        // An empty fsm->name gets sprintf(name, "%X", rand()): nonempty
        // uppercase hex.
        assert_eq!(session.stack_add(fsm_symbol("c")), 2);
        let name = top_fsm_name(&mut session);
        assert!(!name.is_empty());
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
        );
        // fsm_count ran on the pushed net: single-symbol net is
        // "2 states, 1 arc, 1 path" (C foma `print size`), 1 final.
        let top = session.stack_find_top().unwrap();
        session.stack_entry_fsm(top, |f| {
            assert_eq!(f.statecount, 2);
            assert_eq!(f.arccount, 1);
            assert_eq!(f.finalcount, 1);
            assert_eq!(f.pathcount, 1);
        });
    }

    // [spec:foma:sem:stack.stack-pop-fn/test]
    // [spec:foma:sem:foma.stack-pop-fn/test]
    #[test]
    fn stack_add_pop_is_lifo() {
        let mut session = Session::new();
        add_named(&mut session, "a", "first");
        add_named(&mut session, "b", "second");
        add_named(&mut session, "c", "third");
        // Pop returns the most recently pushed net and unlinks the top entry.
        assert_eq!(session.stack_pop().unwrap().name, "third");
        assert_eq!(session.stack_size(), 2);
        assert_eq!(session.e_number(session.stack_find_top().unwrap()), 1);
        assert_eq!(session.stack_pop().unwrap().name, "second");
        // Size-1 fast path: fsm is saved, stack_clear() re-inits empty.
        assert_eq!(session.stack_pop().unwrap().name, "first");
        assert!(session.stack_isempty());
        assert_eq!(session.stack_size(), 0);
    }

    // DEVIATION pin: C dereferences the sentinel's NULL `next` on an empty
    // stack (UB/crash); the port panics.
    // [spec:foma:sem:stack.stack-pop-fn/test]
    // [spec:foma:sem:foma.stack-pop-fn/test]
    #[test]
    #[should_panic]
    fn stack_pop_on_empty_stack_panics() {
        let mut session = Session::new();
        session.stack_pop();
    }

    // [spec:foma:sem:stack.stack-find-top-fn/test]
    // [spec:foma:sem:foma.stack-find-top-fn/test]
    // [spec:foma:sem:stack.stack-find-bottom-fn/test]
    // [spec:foma:sem:foma.stack-find-bottom-fn/test]
    // [spec:foma:sem:stack.stack-find-second-fn/test]
    // [spec:foma:sem:foma.stack-find-second-fn/test]
    #[test]
    fn stack_find_top_bottom_second_on_multi_entry_stacks() {
        let mut session = Session::new();
        // Empty stack: top and bottom are NULL (None).
        assert_eq!(session.stack_find_top(), None);
        assert_eq!(session.stack_find_bottom(), None);
        add_named(&mut session, "a", "only");
        // One entry: top == bottom == head; second (top->previous) is NULL.
        assert_eq!(session.stack_find_top(), session.stack_find_bottom());
        assert_eq!(session.stack_find_second(), None);
        add_named(&mut session, "b", "mid");
        add_named(&mut session, "c", "newest");
        let top = session.stack_find_top().unwrap();
        let bottom = session.stack_find_bottom().unwrap();
        let second = session.stack_find_second().unwrap();
        assert_eq!(session.e_number(top), 2);
        assert_eq!(bottom, session.main_stack());
        assert_eq!(session.e_number(bottom), 0);
        // Second-from-top is the top entry's `previous`.
        assert_eq!(session.e_number(second), 1);
        assert_eq!(session.stack_entry_fsm(second, |f| f.name.clone()), "mid");
    }

    // DEVIATION pin: C's empty-stack guard is commented out, so it walks
    // through the sentinel's NULL `next` (UB/crash); the port panics.
    // [spec:foma:sem:stack.stack-find-second-fn/test]
    // [spec:foma:sem:foma.stack-find-second-fn/test]
    #[test]
    #[should_panic]
    fn stack_find_second_on_empty_stack_panics() {
        let session = Session::new();
        session.stack_find_second();
    }

    // [spec:foma:sem:stack.stack-get-ah-fn/test]
    // [spec:foma:sem:foma.stack-get-ah-fn/test]
    #[test]
    fn stack_get_ah_lazily_creates_then_caches() {
        let mut session = Session::new();
        // Empty stack: NULL (None).
        assert_eq!(session.stack_get_ah(), None);
        add_named(&mut session, "a", "net");
        let se = session.stack_get_ah().unwrap();
        assert_eq!(se, session.stack_find_top().unwrap());
        // Mark the handle; a second call must return the SAME cached handle
        // (not a fresh apply_init), so the mark survives.
        session.stack_entry_ah(se, |ah| ah.ptr = 424_242);
        let se2 = session.stack_get_ah().unwrap();
        assert_eq!(se2, se);
        assert_eq!(session.stack_entry_ah(se2, |ah| ah.ptr), 424_242);
    }

    // [spec:foma:sem:stack.stack-get-med-ah-fn/test]
    // [spec:foma:sem:foma.stack-get-med-ah-fn/test]
    #[test]
    fn stack_get_med_ah_lazily_creates_sets_align_then_caches() {
        let mut session = Session::new();
        assert_eq!(session.stack_get_med_ah(), None);
        add_named(&mut session, "a", "net");
        let se = session.stack_get_med_ah().unwrap();
        assert_eq!(se, session.stack_find_top().unwrap());
        // apply_med_set_align_symbol(amedh, "-") ran on creation.
        assert_eq!(
            session
                .stack_entry_amedh(se, |m| m.align_symbol.clone())
                .as_deref(),
            Some("-")
        );
        // Cached: the marked handle is returned again, not re-created.
        session.stack_entry_amedh(se, |m| m.med_limit = 77);
        let se2 = session.stack_get_med_ah().unwrap();
        assert_eq!(se2, se);
        assert_eq!(session.stack_entry_amedh(se2, |m| m.med_limit), 77);
    }

    // [spec:foma:sem:stack.stack-rotate-fn+1/test]
    // [spec:foma:sem:foma.stack-rotate-fn+1/test]
    #[test]
    fn stack_rotate_swaps_top_and_bottom_fsms_with_their_handles() {
        let mut session = Session::new();
        // Empty: logs "Stack is empty." and returns -1.
        assert_eq!(session.stack_rotate(), StackReorder::Empty);
        add_named(&mut session, "a", "bottomnet");
        // Size 1: returns 1, no change.
        assert_eq!(session.stack_rotate(), StackReorder::Reordered);
        assert_eq!(top_fsm_name(&mut session), "bottomnet");
        add_named(&mut session, "b", "midnet");
        add_named(&mut session, "c", "topnet");
        // Cache an apply handle on the top entry (holding topnet) and mark it.
        let top = session.stack_get_ah().unwrap();
        session.stack_entry_ah(top, |ah| ah.ptr = 313_131);
        assert_eq!(session.stack_rotate(), StackReorder::Reordered);
        // The fsm pointers of bottom and top are exchanged; the middle entry is
        // untouched (for size > 2 this is a swap, not a rotate).
        assert_eq!(bottom_fsm_name(&mut session), "topnet");
        assert_eq!(top_fsm_name(&mut session), "bottomnet");
        let second = session.stack_find_second().unwrap();
        assert_eq!(
            session.stack_entry_fsm(second, |f| f.name.clone()),
            "midnet"
        );
        // Numbers are NOT swapped...
        assert_eq!(session.e_number(session.stack_find_bottom().unwrap()), 0);
        assert_eq!(session.e_number(session.stack_find_top().unwrap()), 2);
        // ...but the cached apply handle now travels WITH its net: the handle
        // built for topnet moved to the bottom entry alongside topnet's fsm, so
        // apply still runs against the transducer it was created for (the C
        // stale-handle quirk is fixed).
        let bottom = session.stack_find_bottom().unwrap();
        assert_eq!(session.stack_entry_ah(bottom, |ah| ah.ptr), 313_131);
        assert_eq!(top, session.stack_find_top().unwrap());
    }

    // [spec:foma:sem:stack.stack-print-fn+1/test]
    // [spec:foma:sem:foma.stack-print-fn+1/test]
    #[test]
    fn stack_print_is_a_noop() {
        let mut session = Session::new();
        session.stack_print();
        add_named(&mut session, "a", "x");
        session.stack_print();
        assert_eq!(session.stack_size(), 1);
    }

    // [spec:foma:sem:stack.stack-clear-fn+1/test]
    // [spec:foma:sem:foma.stack-clear-fn+1/test]
    #[test]
    fn stack_clear_destroys_all_entries_and_reinits() {
        let mut session = Session::new();
        add_named(&mut session, "a", "one");
        add_named(&mut session, "b", "two");
        // Cache handles on the top entry so clear exercises apply_clear /
        // apply_med_clear paths.
        session.stack_get_ah().unwrap();
        session.stack_get_med_ah().unwrap();
        session.stack_clear();
        assert!(session.stack_isempty());
        assert_eq!(session.stack_size(), 0);
        assert_eq!(session.e_number(session.main_stack()), -1);
    }

    // [spec:foma:sem:stack.stack-turn-fn+1/test]
    // [spec:foma:sem:foma.stack-turn-fn+1/test]
    #[test]
    fn stack_turn_reverses_the_stack() {
        let mut session = Session::new();
        // Empty: logs "Stack is empty." and returns 0.
        assert_eq!(session.stack_turn(), StackReorder::Empty);
        add_named(&mut session, "a", "solo");
        // Size 1: returns 1 with no change.
        assert_eq!(session.stack_turn(), StackReorder::Reordered);
        assert_eq!(session.stack_size(), 1);
        assert_eq!(top_fsm_name(&mut session), "solo");

        // Wave 4 fix: a real reversal of a 3-entry stack. Push first (bottom),
        // second, third (top).
        let mut session = Session::new();
        add_named(&mut session, "a", "first");
        add_named(&mut session, "b", "second");
        add_named(&mut session, "c", "third");
        assert_eq!(session.stack_turn(), StackReorder::Reordered);
        // Former top is now the bottom, former bottom is now the top, the
        // middle is unchanged; the stack keeps all three entries.
        assert_eq!(session.stack_size(), 3);
        assert_eq!(bottom_fsm_name(&mut session), "third");
        assert_eq!(top_fsm_name(&mut session), "first");
        let second = session.stack_find_second().unwrap();
        assert_eq!(
            session.stack_entry_fsm(second, |f| f.name.clone()),
            "second"
        );
        // Entries travel with their own number (not renumbered): the new bottom
        // carries the former top's number 2, the new top the former bottom's 0.
        assert_eq!(session.e_number(session.stack_find_bottom().unwrap()), 2);
        assert_eq!(session.e_number(session.stack_find_top().unwrap()), 0);
        // Forward (next) order from the bottom is fully relinked...
        let bottom = session.stack_find_bottom().unwrap();
        let mid = session.e_next(bottom).unwrap();
        let top = session.e_next(mid).unwrap();
        assert_eq!(top, session.stack_find_top().unwrap());
        // ...and the previous links mirror it: bottom has no previous, and each
        // forward hop is matched by a backward hop.
        assert_eq!(session.e_previous(bottom), None);
        assert_eq!(session.e_previous(mid), Some(bottom));
        assert_eq!(session.e_previous(top), Some(mid));
        // Reversal is an involution: turning again restores the original order.
        assert_eq!(session.stack_turn(), StackReorder::Reordered);
        assert_eq!(bottom_fsm_name(&mut session), "first");
        assert_eq!(top_fsm_name(&mut session), "third");
    }
}
