//! foma/define.c — literal Wave-2 (bug-for-bug) port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/define.md.

use crate::constructions::fsm_count;
use crate::options::FomaOptions;
use crate::structures::fsm_destroy;
use crate::types::{DefinedFunctions, DefinedNetworks, Fsm};

// C: the non-static `g_defines` / `g_defines_f` registry globals defined at
// the top of define.c (extern'd by iface.c/foma.c) live on `Session` now
// (`session.defines` / `session.defines_f`, init'd by `Session::new`).

/// Outcome of adding a definition (`add_defined` / `add_defined_function`).
/// Replaces C's `0`/`1` status ints: the verbose CLI printer switches on the
/// variant instead of a magic number. (C also had a `-1` "name too long" code
/// from the fixed 40-byte name buffer; names are heap Strings now, so that
/// case no longer exists.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Defined {
    /// A fresh definition was stored (C returned 0), or the no-op when the
    /// supplied net is `None`.
    New,
    /// An existing definition of the same name (+ arity, for functions) was
    /// replaced in place (C returned 1).
    Redefined,
}

/// Outcome of removing a definition (`remove_defined`). Replaces C's `0`/`1`
/// success/absent status int.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Undefined {
    /// The definition was removed (C returned 0); also the undefine-all path.
    Removed,
    /// No definition of that name existed (C returned 1).
    NotFound,
}

/* Find a defined symbol from the symbol table */
/* Return the corresponding FSM                */
// [spec:foma:def:define.find-defined-fn]
// [spec:foma:sem:define.find-defined-fn]
// [spec:foma:def:fomalib.find-defined-fn]
// [spec:foma:sem:fomalib.find-defined-fn]
pub fn find_defined<'a>(def: &'a mut DefinedNetworks, string: &str) -> Option<&'a mut Fsm> {
    /* C: returns NULL when def == NULL (loop runs zero times); a &mut borrow
    is never NULL — NULL-able callers keep the check at the call site. The
    returned net is the registry's own copy (borrowed, not duplicated). */
    let mut d = Some(def);
    while let Some(node) = d {
        if node.name.as_deref() == Some(string) {
            return node.net.as_deref_mut();
        }
        d = node.next.as_deref_mut();
    }
    None
}

// [spec:foma:def:define.defined-networks-init-fn]
// [spec:foma:sem:define.defined-networks-init-fn]
// [spec:foma:def:fomalib.defined-networks-init-fn]
// [spec:foma:sem:fomalib.defined-networks-init-fn]
pub fn defined_networks_init() -> Box<DefinedNetworks> {
    /* calloc: Dummy first entry, so we can maintain the ptr */
    Box::new(DefinedNetworks {
        name: None,
        net: None,
        next: None,
    })
}

// [spec:foma:def:define.defined-functions-init-fn]
// [spec:foma:sem:define.defined-functions-init-fn]
// [spec:foma:def:fomalib.defined-functions-init-fn]
// [spec:foma:sem:fomalib.defined-functions-init-fn]
pub fn defined_functions_init() -> Box<DefinedFunctions> {
    /* calloc: Dummy first entry */
    Box::new(DefinedFunctions {
        name: None,
        regex: None,
        numargs: 0,
        next: None,
    })
}

/* Removes a defined network from the list.                */
/* Undefines all if None is passed as the string argument. */

// [spec:foma:def:define.remove-defined-fn]
// [spec:foma:sem:define.remove-defined-fn]
// [spec:foma:def:fomalib.remove-defined-fn]
// [spec:foma:sem:fomalib.remove-defined-fn]
pub fn remove_defined(def: &mut DefinedNetworks, string: Option<&str>) -> Undefined {
    /* Undefine all */
    let Some(string) = string else {
        let mut d: Option<&mut DefinedNetworks> = Some(def);
        while let Some(node) = d {
            if let Some(net) = node.net.take() {
                fsm_destroy(net);
            }
            /* Clear name/net (already taken above); the nodes themselves stay
            in the list as in C, so the registry keeps its node count but reads
            as empty. */
            node.name = None;
            d = node.next.as_deref_mut();
        }
        return Undefined::Removed;
    };
    /* C scans once tracking d and d_prev; here: the same existence scan,
    then the head and non-head cases through fresh borrows */
    let mut exists = false;
    {
        let mut d = Some(&*def);
        while let Some(node) = d {
            if node.name.as_deref() == Some(string) {
                exists = true;
                break;
            }
            d = node.next.as_deref();
        }
    }
    if !exists {
        return Undefined::NotFound;
    }
    if def.name.as_deref() == Some(string) {
        /* d == def */
        if let Some(mut next) = def.next.take() {
            /* fsm_destroy(d->net) — C's fsm_destroy is a no-op on NULL */
            if let Some(net) = def.net.take() {
                fsm_destroy(net);
            }
            /* free(d->name) — dropped by the overwrite below */
            def.name = next.name.take();
            def.net = next.net.take();
            let d_next = next.next.take();
            /* free(d->next) — `next` dropped */
            def.next = d_next;
        } else {
            if let Some(net) = def.net.take() {
                fsm_destroy(net);
            }
            /* free(d->name) */
            def.next = None;
            def.name = None;
            def.net = None;
        }
    } else {
        let mut d_prev = &mut *def;
        // Existence was established above, so the match is always found before
        // `next` runs out (the `else { break }` is C's unreachable NULL tail).
        while d_prev.next.is_some() {
            let matched = d_prev
                .next
                .as_deref()
                .is_some_and(|d| d.name.as_deref() == Some(string));
            if matched {
                let Some(mut node) = d_prev.next.take() else {
                    break;
                };
                if let Some(net) = node.net.take() {
                    fsm_destroy(net);
                }
                /* free(d->name); d_prev->next = d->next; free(d) */
                d_prev.next = node.next.take();
                break;
            }
            let Some(nextnode) = d_prev.next.as_deref_mut() else {
                break;
            };
            d_prev = nextnode;
        }
    }
    Undefined::Removed
}

/* Finds defined regex "function" based on name, numargs */
/* Returns the corresponding regex                       */
// [spec:foma:def:define.find-defined-function-fn]
// [spec:foma:sem:define.find-defined-function-fn]
// [spec:foma:def:fomalib.find-defined-function-fn]
// [spec:foma:sem:fomalib.find-defined-function-fn]
pub fn find_defined_function<'a>(
    deff: &'a DefinedFunctions,
    name: &str,
    numargs: i32,
) -> Option<&'a str> {
    /* returns the stored regex string, borrowed from the list (not a copy) */
    let mut d = Some(deff);
    while let Some(node) = d {
        if node.name.as_deref() == Some(name) && node.numargs == numargs {
            return node.regex.as_deref();
        }
        d = node.next.as_deref();
    }
    None
}

/* Add a function to list of defined functions */
// [spec:foma:def:define.add-defined-function-fn]
// [spec:foma:sem:define.add-defined-function-fn]
// [spec:foma:def:fomalib.add-defined-function-fn]
// [spec:foma:sem:fomalib.add-defined-function-fn]
pub fn add_defined_function(
    opts: &FomaOptions,
    deff: &mut DefinedFunctions,
    name: &str,
    regex: &str,
    numargs: i32,
) -> Defined {
    let mut d = Some(&mut *deff);
    while let Some(node) = d {
        if node.name.as_deref() == Some(name) && node.numargs == numargs {
            /* free(d->regex); d->regex = strdup(regex) */
            node.regex = Some(regex.into());
            if opts.verbose {
                /* literal C message, including the unbalanced trailing ')' */
                tracing::info!("redefined {}@{})", name, numargs);
            }
            return Defined::Redefined;
        }
        d = node.next.as_deref_mut();
    }
    let d = if deff.name.is_none() {
        deff
    } else {
        let d = Box::new(DefinedFunctions {
            name: None,
            regex: None,
            numargs: 0,
            /* d->next = deff->next; deff->next = d */
            next: deff.next.take(),
        });
        /* deff->next = d — insert returns the just-stored node */
        deff.next.insert(d).as_mut()
    };
    d.name = Some(name.into()); /* strdup(name) */
    d.regex = Some(regex.into()); /* strdup(regex) */
    d.numargs = numargs;
    Defined::New
}

/* Add a network to list of defined networks. */
/* Always maintain head of list at same ptr.  */

// [spec:foma:def:define.add-defined-fn]
// [spec:foma:sem:define.add-defined-fn]
// [spec:foma:def:fomalib.add-defined-fn]
// [spec:foma:sem:fomalib.add-defined-fn]
pub fn add_defined(def: &mut DefinedNetworks, net: Option<Box<Fsm>>, string: &str) -> Defined {
    let mut net = match net {
        None => return Defined::New,
        Some(net) => net,
    };

    fsm_count(&mut net);
    let mut d = Some(&mut *def);
    while let Some(node) = d {
        if node.name.as_deref() == Some(string) {
            /* fsm_destroy(d->net) — C's fsm_destroy is a no-op on NULL */
            if let Some(old) = node.net.take() {
                fsm_destroy(old);
            }
            /* free(d->name) */
            node.net = Some(net);
            node.name = Some(string.into()); /* strdup(string) */
            return Defined::Redefined;
        }
        d = node.next.as_deref_mut();
    }
    let d = if def.name.is_none() {
        def
    } else {
        let d = Box::new(DefinedNetworks {
            name: None,
            net: None,
            /* d->next = def->next; def->next = d */
            next: def.next.take(),
        });
        /* def->next = d — insert returns the just-stored node */
        def.next.insert(d).as_mut()
    };
    d.name = Some(string.into()); /* strdup(string) */
    d.net = Some(net);
    Defined::New
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constructions::fsm_symbol;

    /* Names in list order (dummy head included), skipping cleared nodes. */
    fn net_names(def: &DefinedNetworks) -> Vec<String> {
        let mut v = Vec::new();
        let mut d = Some(def);
        while let Some(n) = d {
            if let Some(name) = &n.name {
                v.push(name.to_string());
            }
            d = n.next.as_deref();
        }
        v
    }

    /* Total node count (including cleared/dummy nodes). */
    fn node_count(def: &DefinedNetworks) -> usize {
        let mut c = 0;
        let mut d = Some(def);
        while let Some(n) = d {
            c += 1;
            d = n.next.as_deref();
        }
        c
    }

    fn has_sym(net: &Fsm, sym: &str) -> bool {
        net.sigma.iter().any(|node| node.symbol == sym)
    }

    // [spec:foma:sem:define.defined-networks-init-fn/test]
    // [spec:foma:sem:fomalib.defined-networks-init-fn/test]
    // [spec:foma:sem:define.defined-functions-init-fn/test]
    // [spec:foma:sem:fomalib.defined-functions-init-fn/test]
    #[test]
    fn defined_init_dummy_head() {
        let dn = defined_networks_init();
        assert!(dn.name.is_none() && dn.net.is_none() && dn.next.is_none());
        let df = defined_functions_init();
        assert!(df.name.is_none() && df.regex.is_none() && df.next.is_none());
        assert_eq!(df.numargs, 0);
    }

    // [spec:foma:sem:define.add-defined-fn/test]
    // [spec:foma:sem:fomalib.add-defined-fn/test]
    // [spec:foma:sem:define.find-defined-fn/test]
    // [spec:foma:sem:fomalib.find-defined-fn/test]
    #[test]
    fn add_find_defined_nets() {
        let mut def = defined_networks_init();
        /* First add fills the dummy head; subsequent adds splice in
        immediately after the head, so a,b,c inserts as [a, c, b]. */
        assert_eq!(
            add_defined(&mut def, Some(fsm_symbol("A")), "a"),
            Defined::New
        );
        assert_eq!(
            add_defined(&mut def, Some(fsm_symbol("B")), "b"),
            Defined::New
        );
        assert_eq!(
            add_defined(&mut def, Some(fsm_symbol("C")), "c"),
            Defined::New
        );
        assert_eq!(net_names(&def), vec!["a", "c", "b"]);

        /* find_defined returns the registry's own net (borrowed). */
        assert!(has_sym(find_defined(&mut def, "b").unwrap(), "B"));
        assert!(find_defined(&mut def, "zzz").is_none());

        /* Redefinition replaces the net in place. */
        assert_eq!(
            add_defined(&mut def, Some(fsm_symbol("A2")), "a"),
            Defined::Redefined
        );
        assert!(has_sym(find_defined(&mut def, "a").unwrap(), "A2"));
        assert_eq!(
            net_names(&def),
            vec!["a", "c", "b"],
            "redefinition adds no node"
        );

        /* net == None is a no-op reported as New. */
        assert_eq!(add_defined(&mut def, None, "q"), Defined::New);
        assert!(find_defined(&mut def, "q").is_none());

        /* Names are heap Strings with no length cap: a long name is stored as-is. */
        let long = "x".repeat(300);
        assert_eq!(
            add_defined(&mut def, Some(fsm_symbol("Z")), &long),
            Defined::New
        );
        assert!(find_defined(&mut def, &long).is_some());
    }

    // [spec:foma:sem:define.remove-defined-fn/test]
    // [spec:foma:sem:fomalib.remove-defined-fn/test]
    #[test]
    fn remove_defined_cases() {
        let mut def = defined_networks_init();
        add_defined(&mut def, Some(fsm_symbol("A")), "a");
        add_defined(&mut def, Some(fsm_symbol("B")), "b");
        add_defined(&mut def, Some(fsm_symbol("C")), "c");
        /* list order: a, c, b */

        /* Remove a non-head node (predecessor splices it out). */
        assert_eq!(remove_defined(&mut def, Some("c")), Undefined::Removed);
        assert_eq!(net_names(&def), vec!["a", "b"]);
        /* Removing an absent name reports NotFound. */
        assert_eq!(remove_defined(&mut def, Some("zzz")), Undefined::NotFound);
        /* Remove the head when it has a successor: successor moves into head. */
        assert_eq!(remove_defined(&mut def, Some("a")), Undefined::Removed);
        assert_eq!(net_names(&def), vec!["b"]);
        /* Remove the head when it is the only node: back to empty dummy. */
        assert_eq!(remove_defined(&mut def, Some("b")), Undefined::Removed);
        assert!(net_names(&def).is_empty());
        assert!(def.name.is_none() && def.net.is_none() && def.next.is_none());

        /* Undefine-all quirk: nodes remain in the list (count unchanged) but
        their name/net payloads are cleared, so the registry reads as empty. */
        let mut def2 = defined_networks_init();
        add_defined(&mut def2, Some(fsm_symbol("A")), "a");
        add_defined(&mut def2, Some(fsm_symbol("B")), "b");
        assert_eq!(node_count(&def2), 2);
        assert_eq!(remove_defined(&mut def2, None), Undefined::Removed);
        assert_eq!(node_count(&def2), 2, "nodes remain after undefine-all");
        assert!(net_names(&def2).is_empty(), "payloads cleared");
        assert!(find_defined(&mut def2, "a").is_none());
    }

    // [spec:foma:sem:define.add-defined-function-fn/test]
    // [spec:foma:sem:fomalib.add-defined-function-fn/test]
    // [spec:foma:sem:define.find-defined-function-fn/test]
    // [spec:foma:sem:fomalib.find-defined-function-fn/test]
    #[test]
    fn add_find_defined_functions() {
        let opts = &FomaOptions::default();
        let mut deff = defined_functions_init();
        /* (name, numargs) is the key: same name, different arity is a new node. */
        assert_eq!(
            add_defined_function(opts, &mut deff, "@f", "a b", 2),
            Defined::New
        );
        assert_eq!(
            add_defined_function(opts, &mut deff, "@f", "c d", 1),
            Defined::New
        );
        assert_eq!(find_defined_function(&deff, "@f", 2), Some("a b"));
        assert_eq!(find_defined_function(&deff, "@f", 1), Some("c d"));
        /* Arity mismatch / unknown name are not found. */
        assert_eq!(find_defined_function(&deff, "@f", 3), None);
        assert_eq!(find_defined_function(&deff, "@g", 2), None);

        /* Redefinition (same name+numargs) replaces the regex and reports
        Redefined. Drive the g_verbose "redefined %s@%i)" message path (stderr,
        not asserted here). */
        assert_eq!(
            add_defined_function(opts, &mut deff, "@f", "x y", 2),
            Defined::Redefined
        );
        assert_eq!(find_defined_function(&deff, "@f", 2), Some("x y"));
        /* The arity-1 overload is untouched. */
        assert_eq!(find_defined_function(&deff, "@f", 1), Some("c d"));
    }
}
