//! foma/rewrite.c — literal Wave-2 (bug-for-bug) port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/rewrite.md
//! (per-file ids) plus the fomalib.h prototype ids for fsm_rewrite and
//! fsm_clear_contexts.
//!
//! The compiler works in a flattened 4-tape block encoding: each logical
//! position is 4 consecutive symbols — tape 1 the position class, tape 2
//! the rule-number marker, tape 3 the input symbol, tape 4 the output
//! symbol (see the header comment in rewrite.c and the sem rules).

use crate::constructions::{
    fsm_compact, fsm_complement, fsm_compose, fsm_concat, fsm_concat_n, fsm_contains,
    fsm_cross_product, fsm_flatten, fsm_intersect, fsm_kleene_star, fsm_minus,
    fsm_substitute_symbol, fsm_symbol, fsm_unflatten, fsm_union, fsm_universal,
};
use crate::extract::{fsm_lower, fsm_upper};
use crate::minimize::fsm_minimize;
use crate::options::FomaOptions;
use crate::regex::fsm_parse_regex;
use crate::sigma::{sigma_add, sigma_find, sigma_remove, sigma_sort, sigma_substitute};
use crate::structures::{fsm_copy, fsm_destroy, fsm_empty_set, fsm_empty_string, fsm_identity};
use crate::types::{
    ARROW_DOTTED, ARROW_LEFT, ARROW_LONGEST_MATCH, ARROW_OPTIONAL, ARROW_RIGHT,
    ARROW_SHORTEST_MATCH, Fsm, Fsmcontexts, OP_DOWNWARD_REPLACE, OP_LEFTWARD_REPLACE,
    OP_RIGHTWARD_REPLACE, OP_TWO_LEVEL_REPLACE, OP_UPWARD_REPLACE, RewriteSet,
};

// Lower(X) puts X on output tape (may also be represented by @ID@ on input tape)
// Upper(X) puts X on input tape
// Unrewritten(X) X on input tape, not rewritten (aligned with @O@ symbols)
// NotContain(X) MT does not contain MT configuration X

// Boundary: every MT word begins and ends with boundary, i.e. the @#@ symbol on the input tape, output tape, and relevant semantic symbols

/*

       [ @O@  ]  [ @I[@        ] [ @I@         ] [ @I]@        ] [ @I[]@       ]
       [ @0@  ]  [ @#0001@     ] [ @#0001@     ] [ @#0001@     ] [ @#0001@     ]
       [ @#@  ]  [ ANY|@0@     ] [ ANY|@0@     ] [ ANY|@0@     ] [ ANY|@0@     ]
       [ @ID@ ]  [ ANY|@ID|@0@ ] [ ANY|@ID|@0@ ] [ ANY|@ID|@0@ ] [ ANY|@ID|@0@ ]


*/
/* Special symbols used:
   @0@    Epsilon
   @O@    Outside rewrite
   @I@    Inside rewrite
   @I[@   Beginning of rewrite
   @I[]@  Beginning and end of rewrite
   @I]@   End of rewrite
   @ID@   Identity symbol (= repeat symbol on previous tape at this position)
   @#X@   X = rule number (one for each rule, starting with @#0001@)
*/

// [spec:foma:def:rewrite.rewrite-batch]
#[derive(Debug)]
pub struct RewriteBatch {
    /* C: struct rewrite_set *rewrite_set — assigned once in fsm_rewrite and
    never read anywhere. DEVIATION from C (aliases the caller's rule set;
    safe Rust cannot store the alias — always None here). */
    pub rewrite_set: Option<Box<RewriteSet>>,
    pub rulenames: Option<Box<Fsm>>,
    pub isyms: Option<Box<Fsm>>,
    pub any: Option<Box<Fsm>>,
    /// C: declared but never assigned anywhere — always NULL (see the
    /// rewrite-cleanup sem rule). Always None here.
    pub iopen: Option<Box<Fsm>>,
    /// C: declared but never assigned anywhere — always NULL. Always None.
    pub iclose: Option<Box<Fsm>>,
    pub itape: Option<Box<Fsm>>,
    pub any4tape: Option<Box<Fsm>>,
    pub epextend: Option<Box<Fsm>>,
    pub num_rules: i32,
    /// C: `char (*namestrings)[8]` — one sprintf'd "@#%04i@" row per rule.
    /// (The C rows overflow for rule numbers > 9999 — memory-unsafe,
    /// unreproducible; String rows here.)
    pub namestrings: Vec<String>,
}

/* C: char *specialsymbols[] = { ..., NULL } — the NULL terminator is
dropped; iteration is by length instead */
pub static SPECIALSYMBOLS: [&str; 8] =
    ["@0@", "@O@", "@I@", "@I[@", "@I[]@", "@I]@", "@ID@", "@#@"];

// [spec:foma:def:rewrite.fsm-rewrite-fn]
// [spec:foma:sem:rewrite.fsm-rewrite-fn]
// [spec:foma:def:fomalib.fsm-rewrite-fn]
// [spec:foma:sem:fomalib.fsm-rewrite-fn]
pub fn fsm_rewrite(opts: &FomaOptions, all_rules: &mut RewriteSet) -> Box<Fsm> {
    let mut num_rules: i32;
    let mut rule_number: i32;
    let mut dir: i32;
    let mut i: i32;
    /* Count parallel rules */
    num_rules = 0;
    {
        let mut ruleset = Some(&*all_rules);
        while let Some(rs) = ruleset {
            let mut rules = rs.rewrite_rules.as_deref();
            while let Some(r) = rules {
                num_rules += 1;
                rules = r.next.as_deref();
            }
            ruleset = rs.next.as_deref();
        }
    }

    /* rb = calloc(1, sizeof(struct rewrite_batch)) */
    let mut rb = RewriteBatch {
        rewrite_set: None, /* C: rb->rewrite_set = all_rules (never read) */
        rulenames: None,
        isyms: None,
        any: None,
        iopen: None,
        iclose: None,
        itape: None,
        any4tape: None,
        epextend: None,
        num_rules,
        namestrings: Vec::new(),
    };
    i = 0;
    while i < rb.num_rules {
        /* sprintf(rb->namestrings[i], "@#%04i@", i+1) */
        rb.namestrings.push(format!("@#{:04}@", i + 1));
        i += 1;
    }

    rb.isyms = Some(fsm_minimize(
        opts,
        fsm_union(
            opts,
            fsm_symbol("@I@"),
            fsm_union(
                opts,
                fsm_symbol("@I[]@"),
                fsm_union(opts, fsm_symbol("@I[@"), fsm_symbol("@I]@")),
            ),
        ),
    ));
    rb.rulenames = Some(fsm_empty_set());
    i = 1;
    while i <= num_rules {
        let sym = fsm_symbol(&rb.namestrings[(i - 1) as usize]);
        rb.rulenames = Some(fsm_minimize(
            opts,
            fsm_union(
                opts,
                rb.rulenames
                    .take()
                    .expect("rb.rulenames built at fsm_rewrite start"),
                sym,
            ),
        ));
        i += 1;
    }
    rb.any = Some(fsm_identity());
    /* rewrite_add_special_syms(rb, rb->ANY) — detach rb->ANY while rb is
    passed shared (borrowck) */
    let mut any = rb.any.take().expect("rb.any built at fsm_rewrite start");
    rewrite_add_special_syms(&rb, Some(&mut any));
    rb.any = Some(any);

    /* Add auxiliary symbols to all alphabets */
    {
        let mut ruleset = Some(&mut *all_rules);
        while let Some(rs) = ruleset {
            let mut rules = rs.rewrite_rules.as_deref_mut();
            while let Some(r) = rules {
                rewrite_add_special_syms(&rb, r.left.as_deref_mut());
                rewrite_add_special_syms(&rb, r.right.as_deref_mut());
                rewrite_add_special_syms(&rb, r.right2.as_deref_mut());
                rules = r.next.as_deref_mut();
            }
            let mut contexts = rs.rewrite_contexts.as_deref_mut();
            while let Some(c) = contexts {
                rewrite_add_special_syms(&rb, c.left.as_deref_mut());
                rewrite_add_special_syms(&rb, c.right.as_deref_mut());
                contexts = c.next.as_deref_mut();
            }
            ruleset = rs.next.as_deref_mut();
        }
    }
    /* Get cross-product of every rule, according to its type */
    let mut rule_cp = fsm_empty_set();
    rule_number = 1;
    {
        let mut ruleset = Some(&mut *all_rules);
        while let Some(rs) = ruleset {
            dir = rs.rule_direction;
            let _ = dir; /* C: dir is assigned here but only read in the next loop */
            let mut rules = rs.rewrite_rules.as_deref_mut();
            while let Some(r) = rules {
                let cp: Box<Fsm>;
                if r.right.is_none() {
                    /* T(x)-type rule */
                    let left_copy =
                        fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                    let mut cp_new = rewrite_cp_transducer(opts, &mut rb, left_copy, rule_number);
                    r.cross_product = Some(fsm_copy(&mut cp_new));
                    let left_copy =
                        fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                    r.right = Some(fsm_minimize(opts, fsm_lower(left_copy)));
                    let left_copy =
                        fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                    /* replace the center with its upper side (old center dropped) */
                    r.left = Some(fsm_minimize(opts, fsm_upper(left_copy)));
                    rewrite_add_special_syms(&rb, r.right.as_deref_mut());
                    rewrite_add_special_syms(&rb, r.left.as_deref_mut());
                    cp = cp_new;
                } else if r.right2.is_none() {
                    /* Regular rewrite rule */
                    let left_copy =
                        fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                    let right_copy = fsm_copy(
                        r.right
                            .as_deref_mut()
                            .expect("rule right built for this op"),
                    );
                    let mut cp_new = rewrite_cp(opts, &mut rb, left_copy, right_copy, rule_number);
                    r.cross_product = Some(fsm_copy(&mut cp_new));
                    cp = cp_new;
                } else {
                    /* A -> B ... C -type rule */
                    let left_copy =
                        fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                    let right_copy = fsm_copy(
                        r.right
                            .as_deref_mut()
                            .expect("rule right built for this op"),
                    );
                    let right2_copy = fsm_copy(
                        r.right2
                            .as_deref_mut()
                            .expect("rule right2 built for this op"),
                    );
                    let mut cp_new = rewrite_cp_markup(
                        opts,
                        &mut rb,
                        left_copy,
                        right_copy,
                        right2_copy,
                        rule_number,
                    );
                    r.cross_product = Some(fsm_copy(&mut cp_new));
                    cp = cp_new;
                }
                rule_cp = fsm_minimize(opts, fsm_union(opts, rule_cp, cp));
                rule_number += 1;
                rules = r.next.as_deref_mut();
            }
            ruleset = rs.next.as_deref_mut();
        }
    }

    /* Create Base language */
    let mut boundary = fsm_parse_regex(opts, "\"@O@\" \"@0@\" \"@#@\" \"@ID@\"", None, None)
        .expect("constant rewrite regex compiles");
    let any_copy = fsm_copy(
        rb.any
            .as_deref_mut()
            .expect("rb.any built at fsm_rewrite start"),
    );
    let outside = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_symbol("@O@"),
            fsm_concat(
                opts,
                fsm_symbol("@0@"),
                fsm_concat(opts, any_copy, fsm_symbol("@ID@")),
            ),
        ),
    );
    let mut base = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_copy(&mut boundary),
            fsm_concat(
                opts,
                fsm_kleene_star(opts, fsm_union(opts, rule_cp, outside)),
                fsm_copy(&mut boundary),
            ),
        ),
    );
    fsm_destroy(boundary);
    rule_number = 1;
    {
        let mut ruleset = Some(&mut *all_rules);
        while let Some(rs) = ruleset {
            dir = rs.rule_direction;
            /* Split the ruleset borrow: the rules loop below reads the
            contexts chain of the same node (disjoint fields) */
            let RewriteSet {
                rewrite_rules,
                rewrite_contexts,
                next,
                ..
            } = rs;
            /* Replace all context spec with Upper/Lower, depending on rule_direction */
            let mut contexts = rewrite_contexts.as_deref_mut();
            while let Some(c) = contexts {
                match dir {
                    OP_UPWARD_REPLACE => {
                        let left_copy = fsm_copy(
                            c.left
                                .as_deref_mut()
                                .expect("context left built for this op"),
                        );
                        c.cpleft = Some(rewrite_upper(opts, &mut rb, left_copy));
                        let right_copy = fsm_copy(
                            c.right
                                .as_deref_mut()
                                .expect("context right built for this op"),
                        );
                        c.cpright = Some(rewrite_upper(opts, &mut rb, right_copy));
                    }
                    OP_RIGHTWARD_REPLACE => {
                        let left_copy = fsm_copy(
                            c.left
                                .as_deref_mut()
                                .expect("context left built for this op"),
                        );
                        c.cpleft = Some(rewrite_lower(opts, &mut rb, left_copy));
                        let right_copy = fsm_copy(
                            c.right
                                .as_deref_mut()
                                .expect("context right built for this op"),
                        );
                        c.cpright = Some(rewrite_upper(opts, &mut rb, right_copy));
                    }
                    OP_LEFTWARD_REPLACE => {
                        let left_copy = fsm_copy(
                            c.left
                                .as_deref_mut()
                                .expect("context left built for this op"),
                        );
                        c.cpleft = Some(rewrite_upper(opts, &mut rb, left_copy));
                        let right_copy = fsm_copy(
                            c.right
                                .as_deref_mut()
                                .expect("context right built for this op"),
                        );
                        c.cpright = Some(rewrite_lower(opts, &mut rb, right_copy));
                    }
                    OP_DOWNWARD_REPLACE => {
                        let left_copy = fsm_copy(
                            c.left
                                .as_deref_mut()
                                .expect("context left built for this op"),
                        );
                        c.cpleft = Some(rewrite_lower(opts, &mut rb, left_copy));
                        let right_copy = fsm_copy(
                            c.right
                                .as_deref_mut()
                                .expect("context right built for this op"),
                        );
                        c.cpright = Some(rewrite_lower(opts, &mut rb, right_copy));
                    }
                    OP_TWO_LEVEL_REPLACE => {
                        let left_copy = fsm_copy(
                            c.left
                                .as_deref_mut()
                                .expect("context left built for this op"),
                        );
                        c.cpleft = Some(rewrite_two_level(opts, &mut rb, left_copy, 0));
                        let right_copy = fsm_copy(
                            c.right
                                .as_deref_mut()
                                .expect("context right built for this op"),
                        );
                        c.cpright = Some(rewrite_two_level(opts, &mut rb, right_copy, 1));
                    }
                    _ => {} /* C: switch has no default */
                }
                contexts = c.next.as_deref_mut();
            }
            let mut rules = rewrite_rules.as_deref_mut();
            while let Some(r) = rules {
                /* Just the rule center w/ number without CP() contests */
                /* Actually, maybe better to include CP(U,L) in this, very slow with e.g. a -> a || _ b^15 */
                if r.arrow_type & ARROW_DOTTED != 0 {
                    /* define EP Tape1of4("@O@") | [ Tape1of4("@I[@" "@I@"* "@I]@" | "@I[]@") & Tape3of4(~["@0@"*]) ] ; */
                    /* Additional constraint: 0->x is only allowed between EP _ EP */
                    /* The left and right sides can be checked separately */
                    /* ~[?* Center ~[EP ?*]] & ~[~[?* EP] Center ?*] */
                    let mut center = fsm_copy(
                        r.cross_product
                            .as_deref_mut()
                            .expect("rule cross_product built for this op"),
                    );
                    base = fsm_intersect(
                        opts,
                        fsm_intersect(
                            opts,
                            base,
                            fsm_complement(
                                opts,
                                fsm_concat(
                                    opts,
                                    rewrite_any_4tape(opts, &mut rb),
                                    fsm_concat(
                                        opts,
                                        fsm_copy(&mut center),
                                        fsm_complement(
                                            opts,
                                            fsm_concat(
                                                opts,
                                                rewrite_epextend(opts, &mut rb),
                                                rewrite_any_4tape(opts, &mut rb),
                                            ),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                        fsm_complement(
                            opts,
                            fsm_concat(
                                opts,
                                fsm_complement(
                                    opts,
                                    fsm_concat(
                                        opts,
                                        rewrite_any_4tape(opts, &mut rb),
                                        rewrite_epextend(opts, &mut rb),
                                    ),
                                ),
                                fsm_concat(
                                    opts,
                                    fsm_copy(&mut center),
                                    rewrite_any_4tape(opts, &mut rb),
                                ),
                            ),
                        ),
                    );
                    fsm_destroy(center);
                }
                if rewrite_contexts.is_some() {
                    let restriction = rewr_context_restrict(
                        opts,
                        &mut rb,
                        r.cross_product
                            .as_deref_mut()
                            .expect("rule cross_product built for this op"),
                        rewrite_contexts.as_deref_mut(),
                    );
                    base = fsm_intersect(opts, base, restriction);
                }
                /* Determine C (based on rule type) */
                let mut c = fsm_empty_set();
                if (r.arrow_type & ARROW_RIGHT) != 0 && (r.arrow_type & ARROW_OPTIONAL) == 0 {
                    let left_copy =
                        fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                    c = fsm_union(
                        opts,
                        c,
                        rewr_unrewritten(
                            opts,
                            &mut rb,
                            fsm_minimize(opts, fsm_minus(opts, left_copy, fsm_empty_string())),
                        ),
                    );
                }
                if (r.arrow_type & ARROW_LEFT) != 0 && (r.arrow_type & ARROW_OPTIONAL) == 0 {
                    let right_copy = fsm_copy(
                        r.right
                            .as_deref_mut()
                            .expect("rule right built for this op"),
                    );
                    c = fsm_union(
                        opts,
                        c,
                        rewr_unrewritten(
                            opts,
                            &mut rb,
                            fsm_minimize(opts, fsm_minus(opts, right_copy, fsm_empty_string())),
                        ),
                    );
                }
                if r.arrow_type & ARROW_LONGEST_MATCH != 0 {
                    if r.arrow_type & ARROW_RIGHT != 0 {
                        let left_copy =
                            fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                        let mut lang = rewrite_upper(opts, &mut rb, left_copy);
                        c = fsm_union(
                            opts,
                            c,
                            rewr_notleftmost(opts, &rb, &mut lang, rule_number, r.arrow_type),
                        );
                        let left_copy =
                            fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                        let mut lang = rewrite_upper(opts, &mut rb, left_copy);
                        c = fsm_union(
                            opts,
                            c,
                            rewr_notlongest(opts, &rb, &mut lang, rule_number, r.arrow_type),
                        );
                    }
                    if r.arrow_type & ARROW_LEFT != 0 {
                        let right_copy = fsm_copy(
                            r.right
                                .as_deref_mut()
                                .expect("rule right built for this op"),
                        );
                        let mut lang = rewrite_lower(opts, &mut rb, right_copy);
                        c = fsm_union(
                            opts,
                            c,
                            rewr_notleftmost(opts, &rb, &mut lang, rule_number, r.arrow_type),
                        );
                        let right_copy = fsm_copy(
                            r.right
                                .as_deref_mut()
                                .expect("rule right built for this op"),
                        );
                        let mut lang = rewrite_lower(opts, &mut rb, right_copy);
                        c = fsm_union(
                            opts,
                            c,
                            rewr_notlongest(opts, &rb, &mut lang, rule_number, r.arrow_type),
                        );
                    }
                }
                if r.arrow_type & ARROW_SHORTEST_MATCH != 0 {
                    if r.arrow_type & ARROW_RIGHT != 0 {
                        let left_copy =
                            fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                        let mut lang = rewrite_upper(opts, &mut rb, left_copy);
                        c = fsm_union(
                            opts,
                            c,
                            rewr_notleftmost(opts, &rb, &mut lang, rule_number, r.arrow_type),
                        );
                        let left_copy =
                            fsm_copy(r.left.as_deref_mut().expect("rule left built for this op"));
                        let mut lang = rewrite_upper(opts, &mut rb, left_copy);
                        c = fsm_union(opts, c, rewr_notshortest(opts, &rb, &mut lang, rule_number));
                    }
                    if r.arrow_type & ARROW_LEFT != 0 {
                        let right_copy = fsm_copy(
                            r.right
                                .as_deref_mut()
                                .expect("rule right built for this op"),
                        );
                        let mut lang = rewrite_lower(opts, &mut rb, right_copy);
                        c = fsm_union(
                            opts,
                            c,
                            rewr_notleftmost(opts, &rb, &mut lang, rule_number, r.arrow_type),
                        );
                        let right_copy = fsm_copy(
                            r.right
                                .as_deref_mut()
                                .expect("rule right built for this op"),
                        );
                        let mut lang = rewrite_lower(opts, &mut rb, right_copy);
                        c = fsm_union(opts, c, rewr_notshortest(opts, &rb, &mut lang, rule_number));
                    }
                }
                if rewrite_contexts.is_none() {
                    if r.arrow_type & ARROW_DOTTED != 0 && (r.arrow_type & ARROW_OPTIONAL) == 0 {
                        let epep = fsm_concat(
                            opts,
                            rewrite_epextend(opts, &mut rb),
                            rewrite_epextend(opts, &mut rb),
                        );
                        base = fsm_minus(opts, base, rewr_contains(opts, &mut rb, epep));
                    } else {
                        let c_copy = fsm_copy(&mut c);
                        base = fsm_minus(opts, base, rewr_contains(opts, &mut rb, c_copy));
                    }
                }
                let mut contexts = rewrite_contexts.as_deref_mut();
                while let Some(ctx) = contexts {
                    /* Constraints: running intersect w/ Base */
                    /* NotContain(LC [Unrewritten|LM|...] RC) */
                    if r.arrow_type & ARROW_DOTTED != 0 && (r.arrow_type & ARROW_OPTIONAL) == 0 {
                        /* Extend left and right */
                        let cpleft_copy = fsm_copy(
                            ctx.cpleft
                                .as_deref_mut()
                                .expect("context cpleft built for this op"),
                        );
                        let left_extend = fsm_minimize(
                            opts,
                            fsm_intersect(
                                opts,
                                fsm_concat(opts, rewrite_any_4tape(opts, &mut rb), cpleft_copy),
                                fsm_concat(
                                    opts,
                                    rewrite_any_4tape(opts, &mut rb),
                                    rewrite_epextend(opts, &mut rb),
                                ),
                            ),
                        );
                        let cpright_copy = fsm_copy(
                            ctx.cpright
                                .as_deref_mut()
                                .expect("context cpright built for this op"),
                        );
                        let right_extend = fsm_minimize(
                            opts,
                            fsm_intersect(
                                opts,
                                fsm_concat(
                                    opts,
                                    rewrite_epextend(opts, &mut rb),
                                    rewrite_any_4tape(opts, &mut rb),
                                ),
                                fsm_concat(opts, cpright_copy, rewrite_any_4tape(opts, &mut rb)),
                            ),
                        );
                        let extended =
                            fsm_minimize(opts, fsm_concat(opts, left_extend, right_extend));
                        base = fsm_minus(opts, base, rewr_contains(opts, &mut rb, extended));
                    } else {
                        let cpleft_copy = fsm_copy(
                            ctx.cpleft
                                .as_deref_mut()
                                .expect("context cpleft built for this op"),
                        );
                        let c_copy = fsm_copy(&mut c);
                        let cpright_copy = fsm_copy(
                            ctx.cpright
                                .as_deref_mut()
                                .expect("context cpright built for this op"),
                        );
                        let lcr =
                            fsm_concat(opts, cpleft_copy, fsm_concat(opts, c_copy, cpright_copy));
                        base = fsm_minus(opts, base, rewr_contains(opts, &mut rb, lcr));
                    }
                    contexts = ctx.next.as_deref_mut();
                }
                rule_number += 1;
                fsm_destroy(c);
                rules = r.next.as_deref_mut();
            }
            ruleset = next.as_deref_mut();
        }
    }
    base = fsm_minimize(
        opts,
        fsm_lower(fsm_compose(
            opts,
            base,
            fsm_parse_regex(opts, "[?:0]^4 [?:0 ?:0 ? ?]* [?:0]^4", None, None)
                .expect("constant rewrite regex compiles"),
        )),
    );
    base = fsm_unflatten(opts, base, "@0@", "@ID@");

    /* C: for (i = 0; specialsymbols[i] != NULL; i++) */
    let mut si: usize = 0;
    while si < SPECIALSYMBOLS.len() {
        sigma_remove(SPECIALSYMBOLS[si], &mut base.sigma);
        si += 1;
    }
    rule_number = 1;
    while rule_number <= num_rules {
        sigma_remove(&rb.namestrings[(rule_number - 1) as usize], &mut base.sigma);
        rule_number += 1;
    }

    fsm_compact(&mut base);
    sigma_sort(&mut base);
    rewrite_cleanup(rb);
    base
}

// [spec:foma:def:rewrite.rewrite-cleanup-fn]
// [spec:foma:sem:rewrite.rewrite-cleanup-fn]
pub fn rewrite_cleanup(rb: RewriteBatch) {
    if let Some(net) = rb.rulenames {
        fsm_destroy(net);
    }
    if let Some(net) = rb.isyms {
        fsm_destroy(net);
    }
    if let Some(net) = rb.any {
        fsm_destroy(net);
    }
    if let Some(net) = rb.iopen {
        fsm_destroy(net);
    }
    if let Some(net) = rb.iclose {
        fsm_destroy(net);
    }
    if let Some(net) = rb.itape {
        fsm_destroy(net);
    }
    if let Some(net) = rb.any4tape {
        fsm_destroy(net);
    }
    if let Some(net) = rb.epextend {
        fsm_destroy(net);
    }
    /* free(rb->namestrings); free(rb); */
    drop(rb.namestrings);
}

// [spec:foma:def:rewrite.rewr-notlongest-fn]
// [spec:foma:sem:rewrite.rewr-notlongest-fn]
pub fn rewr_notlongest(
    opts: &FomaOptions,
    rb: &RewriteBatch,
    lang: &mut Fsm,
    rule_number: i32,
    arrow_type: i32,
) -> Box<Fsm> {
    /* define NotLongest(X)  [Upper(X)/Lower(X) & Tape1of4(IOpen Tape1Sig* ["@O@" | IOpen] Tape1Sig*)] */
    let mut nl = fsm_parse_regex(opts,
        "[\"@I[@\"|\"@I[]@\"] [\"@I[@\"|\"@I[]@\"|\"@I]@\"|\"@I@\"|\"@O@\"]* [\"@O@\"|\"@I[@\"|\"@I[]@\"] [\"@I[@\"|\"@I[]@\"|\"@I]@\"|\"@I@\"|\"@O@\"]*",
        None,
        None,
    )
    .expect("constant rewrite regex compiles");
    nl = rewrite_tape_m_to_n_of_k(opts, nl, 1, 1, 4);
    let rulenum = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_identity(),
            fsm_concat(
                opts,
                fsm_symbol(&rb.namestrings[(rule_number - 1) as usize]),
                fsm_concat(
                    opts,
                    fsm_identity(),
                    fsm_concat(opts, fsm_identity(), fsm_universal()),
                ),
            ),
        ),
    );
    nl = fsm_intersect(opts, nl, rulenum);
    /* lang can't end in @0@ */
    let flt = if arrow_type & ARROW_RIGHT != 0 {
        fsm_parse_regex(opts, "[? ? ? ?]* [? ? [?-\"@0@\"] ?]", None, None)
            .expect("constant rewrite regex compiles")
    } else {
        fsm_parse_regex(opts, "[? ? ? ?]* [? ? ? [?-\"@0@\"]]", None, None)
            .expect("constant rewrite regex compiles")
    };
    fsm_minimize(
        opts,
        fsm_intersect(opts, fsm_intersect(opts, nl, fsm_copy(lang)), flt),
    )
}

// [spec:foma:def:rewrite.rewr-notshortest-fn]
// [spec:foma:sem:rewrite.rewr-notshortest-fn]
pub fn rewr_notshortest(
    opts: &FomaOptions,
    rb: &RewriteBatch,
    lang: &mut Fsm,
    rule_number: i32,
) -> Box<Fsm> {
    /* define NotShortest(X)   [Upper/Lower(X) & Tape1of4("@I[@" \IClose*)] */
    let mut ns = fsm_parse_regex(opts, "[\"@I[@\"] \\[\"@I]@\"]*", None, None)
        .expect("constant rewrite regex compiles");
    let rulenum = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_identity(),
            fsm_concat(
                opts,
                fsm_symbol(&rb.namestrings[(rule_number - 1) as usize]),
                fsm_concat(
                    opts,
                    fsm_identity(),
                    fsm_concat(opts, fsm_identity(), fsm_universal()),
                ),
            ),
        ),
    );
    ns = rewrite_tape_m_to_n_of_k(opts, ns, 1, 1, 4);
    ns = fsm_intersect(opts, ns, rulenum);
    fsm_minimize(opts, fsm_intersect(opts, ns, fsm_copy(lang)))
}

// [spec:foma:def:rewrite.rewr-notleftmost-fn]
// [spec:foma:sem:rewrite.rewr-notleftmost-fn]
pub fn rewr_notleftmost(
    opts: &FomaOptions,
    rb: &RewriteBatch,
    lang: &mut Fsm,
    rule_number: i32,
    arrow_type: i32,
) -> Box<Fsm> {
    /* define Leftmost(X)   [Upper/Lower(X) & Tape1of4("@O@" Tape1Sig* IOpen Tape1Sig*) ] */
    let mut nl = fsm_parse_regex(
        opts,
        "\"@O@\" [\"@O@\"]* [\"@I[@\"|\"@I[]@\"] [\"@I[@\"|\"@I[]@\"|\"@I]@\"|\"@I@\"|\"@O@\"]*",
        None,
        None,
    )
    .expect("constant rewrite regex compiles");
    nl = rewrite_tape_m_to_n_of_k(opts, nl, 1, 1, 4);
    let rulenum = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_concat(
                opts,
                fsm_symbol("@O@"),
                fsm_concat(
                    opts,
                    fsm_identity(),
                    fsm_concat(opts, fsm_identity(), fsm_identity()),
                ),
            ),
            fsm_concat(
                opts,
                fsm_kleene_star(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_symbol("@O@"),
                        fsm_concat(
                            opts,
                            fsm_identity(),
                            fsm_concat(opts, fsm_identity(), fsm_identity()),
                        ),
                    ),
                ),
                fsm_concat(
                    opts,
                    fsm_union(opts, fsm_symbol("@I[@"), fsm_symbol("@I[]@")),
                    fsm_concat(
                        opts,
                        fsm_symbol(&rb.namestrings[(rule_number - 1) as usize]),
                        fsm_universal(),
                    ),
                ),
            ),
        ),
    );
    nl = fsm_intersect(opts, nl, rulenum);
    let flt = if arrow_type & ARROW_RIGHT != 0 {
        fsm_parse_regex(opts, "[? ? ? ?]* [? ? [?-\"@0@\"] ?]", None, None)
            .expect("constant rewrite regex compiles")
    } else {
        fsm_parse_regex(opts, "[? ? ? ?]* [? ? ? [?-\"@0@\"]]", None, None)
            .expect("constant rewrite regex compiles")
    };
    fsm_minimize(
        opts,
        fsm_intersect(opts, fsm_intersect(opts, nl, fsm_copy(lang)), flt),
    )
}

// [spec:foma:def:rewrite.rewr-unrewritten-fn]
// [spec:foma:sem:rewrite.rewr-unrewritten-fn]
pub fn rewr_unrewritten(opts: &FomaOptions, rb: &mut RewriteBatch, lang: Box<Fsm>) -> Box<Fsm> {
    /* define Unrewritten(X) [X .o. [0:"@O@" 0:"@0@" ? 0:"@ID@"]*].l; */
    let c = fsm_minimize(
        opts,
        fsm_kleene_star(
            opts,
            fsm_concat(
                opts,
                fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@O@")),
                fsm_concat(
                    opts,
                    fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@0@")),
                    fsm_concat(
                        opts,
                        fsm_copy(
                            rb.any
                                .as_deref_mut()
                                .expect("rb.any built at fsm_rewrite start"),
                        ),
                        fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@ID@")),
                    ),
                ),
            ),
        ),
    );
    fsm_minimize(opts, fsm_lower(fsm_compose(opts, lang, c)))
}

// [spec:foma:def:rewrite.rewr-contains-fn]
// [spec:foma:sem:rewrite.rewr-contains-fn]
pub fn rewr_contains(opts: &FomaOptions, rb: &mut RewriteBatch, lang: Box<Fsm>) -> Box<Fsm> {
    /* define NotContain(X) ~[[Tape1Sig Tape2Sig Tape3Sig Tape4Sig]* X ?*];
    (NO complement is taken despite the name — callers subtract) */
    let first = rewrite_any_4tape(opts, rb);
    let second = rewrite_any_4tape(opts, rb);
    fsm_minimize(
        opts,
        fsm_concat(opts, first, fsm_concat(opts, lang, second)),
    )
}

// [spec:foma:def:rewrite.rewrite-tape-m-to-n-of-k-fn]
// [spec:foma:sem:rewrite.rewrite-tape-m-to-n-of-k-fn]
pub fn rewrite_tape_m_to_n_of_k(
    opts: &FomaOptions,
    lang: Box<Fsm>,
    m: i32,
    n: i32,
    k: i32,
) -> Box<Fsm> {
    /* [X .o. [0:?^(m-1) ?^(n-m+1) 0:?^(k-n)]*].l */
    fsm_minimize(
        opts,
        fsm_lower(fsm_compose(
            opts,
            lang,
            fsm_kleene_star(
                opts,
                fsm_concat(
                    opts,
                    fsm_concat_n(
                        opts,
                        fsm_cross_product(opts, fsm_empty_string(), fsm_identity()),
                        m - 1,
                    ),
                    fsm_concat(
                        opts,
                        fsm_concat_n(opts, fsm_identity(), n - m + 1),
                        fsm_concat_n(
                            opts,
                            fsm_cross_product(opts, fsm_empty_string(), fsm_identity()),
                            k - n,
                        ),
                    ),
                ),
            ),
        )),
    )
}

// [spec:foma:def:rewrite.rewrite-two-level-fn]
// [spec:foma:sem:rewrite.rewrite-two-level-fn]
pub fn rewrite_two_level(
    opts: &FomaOptions,
    rb: &mut RewriteBatch,
    lang: Box<Fsm>,
    rightside: i32,
) -> Box<Fsm> {
    let mut lang = lang;
    let lower = rewrite_lower(opts, rb, fsm_minimize(opts, fsm_lower(fsm_copy(&mut lang))));
    let upper = rewrite_upper(opts, rb, fsm_minimize(opts, fsm_upper(lang)));
    if rightside == 1 {
        fsm_minimize(
            opts,
            fsm_intersect(
                opts,
                fsm_concat(opts, lower, rewrite_any_4tape(opts, rb)),
                fsm_concat(opts, upper, rewrite_any_4tape(opts, rb)),
            ),
        )
    } else {
        fsm_minimize(
            opts,
            fsm_intersect(
                opts,
                fsm_concat(opts, rewrite_any_4tape(opts, rb), lower),
                fsm_concat(opts, rewrite_any_4tape(opts, rb), upper),
            ),
        )
    }
}

// [spec:foma:def:rewrite.rewrite-lower-fn]
// [spec:foma:sem:rewrite.rewrite-lower-fn]
pub fn rewrite_lower(opts: &FomaOptions, rb: &mut RewriteBatch, lower: Box<Fsm>) -> Box<Fsm> {
    /*
       Lower:

       [ @O@      | ISyms    | ISyms    ]*
       [ @0@      | Rulenums | Rulenums ]
       [ <R>,@#@  | @0@,R    |  R       ]
       [ @ID@     | <R>      | @0@      ]

       R = any real symbol
       <R> = any real symbol, not inserted

    */

    let one = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@O@")),
            fsm_concat(
                opts,
                fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@0@")),
                fsm_concat(
                    opts,
                    fsm_union(
                        opts,
                        fsm_symbol("@#@"),
                        fsm_copy(
                            rb.any
                                .as_deref_mut()
                                .expect("rb.any built at fsm_rewrite start"),
                        ),
                    ),
                    fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@ID@")),
                ),
            ),
        ),
    );

    let two = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_cross_product(
                opts,
                fsm_empty_string(),
                fsm_copy(
                    rb.isyms
                        .as_deref_mut()
                        .expect("rb.isyms built at fsm_rewrite start"),
                ),
            ),
            fsm_concat(
                opts,
                fsm_cross_product(
                    opts,
                    fsm_empty_string(),
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                ),
                fsm_concat(
                    opts,
                    fsm_cross_product(
                        opts,
                        fsm_empty_string(),
                        fsm_union(
                            opts,
                            fsm_copy(
                                rb.any
                                    .as_deref_mut()
                                    .expect("rb.any built at fsm_rewrite start"),
                            ),
                            fsm_symbol("@0@"),
                        ),
                    ),
                    fsm_copy(
                        rb.any
                            .as_deref_mut()
                            .expect("rb.any built at fsm_rewrite start"),
                    ),
                ),
            ),
        ),
    );

    let three = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_cross_product(
                opts,
                fsm_empty_string(),
                fsm_copy(
                    rb.isyms
                        .as_deref_mut()
                        .expect("rb.isyms built at fsm_rewrite start"),
                ),
            ),
            fsm_concat(
                opts,
                fsm_cross_product(
                    opts,
                    fsm_empty_string(),
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                ),
                fsm_concat(
                    opts,
                    fsm_cross_product(
                        opts,
                        fsm_empty_string(),
                        fsm_copy(
                            rb.any
                                .as_deref_mut()
                                .expect("rb.any built at fsm_rewrite start"),
                        ),
                    ),
                    fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@0@")),
                ),
            ),
        ),
    );

    let filter = fsm_minimize(
        opts,
        fsm_kleene_star(opts, fsm_union(opts, one, fsm_union(opts, two, three))),
    );
    fsm_minimize(opts, fsm_lower(fsm_compose(opts, lower, filter)))
}

// [spec:foma:def:rewrite.rewrite-any-4tape-fn]
// [spec:foma:sem:rewrite.rewrite-any-4tape-fn]
pub fn rewrite_any_4tape(opts: &FomaOptions, rb: &mut RewriteBatch) -> Box<Fsm> {
    /*
      Upper:

      [ @O@      | ISyms      ]*
      [ @0@      | Rulenums   ]
      [ <R>,@#@  | @0@,R      ]
      [ @ID@     | R,@ID@,@0@ ]

      R = any real symbol
      <R> = any real symbol, not inserted
    */
    if rb.any4tape.is_none() {
        rb.any4tape = Some(fsm_minimize(
            opts,
            fsm_kleene_star(
                opts,
                fsm_union(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_symbol("@O@"),
                        fsm_concat(
                            opts,
                            fsm_symbol("@0@"),
                            fsm_concat(
                                opts,
                                fsm_union(
                                    opts,
                                    fsm_copy(
                                        rb.any
                                            .as_deref_mut()
                                            .expect("rb.any built at fsm_rewrite start"),
                                    ),
                                    fsm_symbol("@#@"),
                                ),
                                fsm_symbol("@ID@"),
                            ),
                        ),
                    ),
                    fsm_concat(
                        opts,
                        fsm_copy(
                            rb.isyms
                                .as_deref_mut()
                                .expect("rb.isyms built at fsm_rewrite start"),
                        ),
                        fsm_concat(
                            opts,
                            fsm_copy(
                                rb.rulenames
                                    .as_deref_mut()
                                    .expect("rb.rulenames built at fsm_rewrite start"),
                            ),
                            fsm_concat(
                                opts,
                                fsm_union(
                                    opts,
                                    fsm_copy(
                                        rb.any
                                            .as_deref_mut()
                                            .expect("rb.any built at fsm_rewrite start"),
                                    ),
                                    fsm_symbol("@0@"),
                                ),
                                fsm_union(
                                    opts,
                                    fsm_copy(
                                        rb.any
                                            .as_deref_mut()
                                            .expect("rb.any built at fsm_rewrite start"),
                                    ),
                                    fsm_union(opts, fsm_symbol("@ID@"), fsm_symbol("@0@")),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ));
    }
    fsm_copy(
        rb.any4tape
            .as_deref_mut()
            .expect("rb.any4tape built before use"),
    )
}

// [spec:foma:def:rewrite.rewrite-upper-fn]
// [spec:foma:sem:rewrite.rewrite-upper-fn]
pub fn rewrite_upper(opts: &FomaOptions, rb: &mut RewriteBatch, upper: Box<Fsm>) -> Box<Fsm> {
    /*
      Upper:

      [ @O@      | ISyms    | ISyms      ]*
      [ @0@      | Rulenums | Rulenums   ]
      [ <R>,@#@  | @0@      | <R>        ]
      [ @ID@     |  R       | R,@ID@,@0@ ]

      R = any real symbol
      <R> = any real symbol, not inserted
    */

    let one = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@O@")),
            fsm_concat(
                opts,
                fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@0@")),
                fsm_concat(
                    opts,
                    fsm_union(
                        opts,
                        fsm_symbol("@#@"),
                        fsm_copy(
                            rb.any
                                .as_deref_mut()
                                .expect("rb.any built at fsm_rewrite start"),
                        ),
                    ),
                    fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@ID@")),
                ),
            ),
        ),
    );

    let two = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_cross_product(
                opts,
                fsm_empty_string(),
                fsm_copy(
                    rb.isyms
                        .as_deref_mut()
                        .expect("rb.isyms built at fsm_rewrite start"),
                ),
            ),
            fsm_concat(
                opts,
                fsm_cross_product(
                    opts,
                    fsm_empty_string(),
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                ),
                fsm_concat(
                    opts,
                    fsm_cross_product(opts, fsm_empty_string(), fsm_symbol("@0@")),
                    fsm_cross_product(
                        opts,
                        fsm_empty_string(),
                        fsm_copy(
                            rb.any
                                .as_deref_mut()
                                .expect("rb.any built at fsm_rewrite start"),
                        ),
                    ),
                ),
            ),
        ),
    );

    let three = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_cross_product(
                opts,
                fsm_empty_string(),
                fsm_copy(
                    rb.isyms
                        .as_deref_mut()
                        .expect("rb.isyms built at fsm_rewrite start"),
                ),
            ),
            fsm_concat(
                opts,
                fsm_cross_product(
                    opts,
                    fsm_empty_string(),
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                ),
                fsm_concat(
                    opts,
                    fsm_copy(
                        rb.any
                            .as_deref_mut()
                            .expect("rb.any built at fsm_rewrite start"),
                    ),
                    fsm_cross_product(
                        opts,
                        fsm_empty_string(),
                        fsm_union(
                            opts,
                            fsm_union(
                                opts,
                                fsm_symbol("@0@"),
                                fsm_copy(
                                    rb.any
                                        .as_deref_mut()
                                        .expect("rb.any built at fsm_rewrite start"),
                                ),
                            ),
                            fsm_symbol("@ID@"),
                        ),
                    ),
                ),
            ),
        ),
    );

    let filter = fsm_minimize(
        opts,
        fsm_kleene_star(opts, fsm_union(opts, one, fsm_union(opts, two, three))),
    );
    fsm_minimize(opts, fsm_lower(fsm_compose(opts, upper, filter)))
}

// [spec:foma:def:rewrite.rewrite-align-fn]
// [spec:foma:sem:rewrite.rewrite-align-fn]
pub fn rewrite_align(opts: &FomaOptions, upper: Box<Fsm>, lower: Box<Fsm>) -> Box<Fsm> {
    /* `[[`[[Tape1of2(upper "@0@"*) & Tape2of2(lower "@0@"*) & ~[[? ?]* "@0@" "@0@" [? ?]*]], %@%_IDENTITY%_SYMBOL%_%@,%@UNK%@] .o. [? ?|"@UNK@" "@UNK@":"@ID@"]*].l, %@UNK%@,%@%_IDENTITY%_SYMBOL%_%@] */
    let first = fsm_minimize(
        opts,
        rewrite_tape_m_to_n_of_k(
            opts,
            fsm_concat(opts, upper, fsm_kleene_star(opts, fsm_symbol("@0@"))),
            1,
            1,
            2,
        ),
    );
    let second = fsm_minimize(
        opts,
        rewrite_tape_m_to_n_of_k(
            opts,
            fsm_concat(opts, lower, fsm_kleene_star(opts, fsm_symbol("@0@"))),
            2,
            2,
            2,
        ),
    );
    let third = fsm_minimize(
        opts,
        fsm_parse_regex(opts, "~[[? ?]* \"@0@\" \"@0@\" [? ?]*]", None, None)
            .expect("constant rewrite regex compiles"),
    );

    let mut align = fsm_minimize(
        opts,
        fsm_intersect(opts, third, fsm_intersect(opts, first, second)),
    );
    align = fsm_minimize(
        opts,
        fsm_substitute_symbol(align, "@_IDENTITY_SYMBOL_@", "@UNK@"),
    );
    let mut align2 = fsm_minimize(
        opts,
        fsm_lower(fsm_compose(
            opts,
            align,
            fsm_parse_regex(opts, "[? ? | \"@UNK@\" \"@UNK@\":\"@ID@\" ]*", None, None)
                .expect("constant rewrite regex compiles"),
        )),
    );
    align2 = fsm_minimize(
        opts,
        fsm_substitute_symbol(align2, "@UNK@", "@_IDENTITY_SYMBOL_@"),
    );
    align2
}

// [spec:foma:def:rewrite.rewrite-align-markup-fn]
// [spec:foma:sem:rewrite.rewrite-align-markup-fn]
pub fn rewrite_align_markup(
    opts: &FomaOptions,
    upper: Box<Fsm>,
    lower1: Box<Fsm>,
    lower2: Box<Fsm>,
) -> Box<Fsm> {
    /* [Tape1of2("@0@"*) & Tape2of2(lower1)] [Tape1of2(upper) & Tape2of2("@ID@"*)] [ Tape1of2(lower1) & Tape2of2("@0@"*)] */
    /* + make sure IDENTITY and UNKNOWN are taken care of */
    let first = fsm_minimize(
        opts,
        rewrite_tape_m_to_n_of_k(opts, fsm_kleene_star(opts, fsm_symbol("@0@")), 1, 1, 2),
    );
    let second = fsm_minimize(opts, rewrite_tape_m_to_n_of_k(opts, lower1, 2, 2, 2));
    let third = fsm_minimize(opts, rewrite_tape_m_to_n_of_k(opts, upper, 1, 1, 2));
    let fourth = fsm_minimize(
        opts,
        rewrite_tape_m_to_n_of_k(opts, fsm_kleene_star(opts, fsm_symbol("@ID@")), 2, 2, 2),
    );
    let fifth = fsm_minimize(
        opts,
        rewrite_tape_m_to_n_of_k(opts, fsm_kleene_star(opts, fsm_symbol("@0@")), 1, 1, 2),
    );
    let sixth = fsm_minimize(opts, rewrite_tape_m_to_n_of_k(opts, lower2, 2, 2, 2));
    let mut align = fsm_minimize(
        opts,
        fsm_concat(
            opts,
            fsm_intersect(opts, first, second),
            fsm_concat(
                opts,
                fsm_intersect(opts, third, fourth),
                fsm_intersect(opts, fifth, sixth),
            ),
        ),
    );
    align = fsm_minimize(
        opts,
        fsm_substitute_symbol(align, "@_IDENTITY_SYMBOL_@", "@UNK@"),
    );
    let mut align2 = fsm_minimize(
        opts,
        fsm_lower(fsm_compose(
            opts,
            align,
            fsm_parse_regex(opts, "[? ? | \"@UNK@\" \"@UNK@\":\"@ID@\" ]*", None, None)
                .expect("constant rewrite regex compiles"),
        )),
    );
    align2 = fsm_minimize(
        opts,
        fsm_substitute_symbol(align2, "@UNK@", "@_IDENTITY_SYMBOL_@"),
    );
    align2
}

// [spec:foma:def:rewrite.rewrite-itape-fn]
// [spec:foma:sem:rewrite.rewrite-itape-fn]
pub fn rewrite_itape(opts: &FomaOptions, rb: &mut RewriteBatch) -> Box<Fsm> {
    if rb.itape.is_none() {
        rb.itape = Some(
            fsm_parse_regex(opts,
                "[\"@I[]@\" ? ? ? | \"@I[@\" ? ? ? [\"@I@\" ? ? ?]* \"@I]@\" ? [?-\"@0@\"] ? ] [\"@I]@\" ? \"@0@\" ?]* | 0",
                None,
                None,
            )
            .expect("constant rewrite regex compiles"),
        );
    }
    fsm_copy(rb.itape.as_deref_mut().expect("rb.itape built before use"))
}

// [spec:foma:def:rewrite.rewrite-cp-markup-fn]
// [spec:foma:sem:rewrite.rewrite-cp-markup-fn]
pub fn rewrite_cp_markup(
    opts: &FomaOptions,
    rb: &mut RewriteBatch,
    upper: Box<Fsm>,
    lower1: Box<Fsm>,
    lower2: Box<Fsm>,
    rule_number: i32,
) -> Box<Fsm> {
    /* Same as rewrite_cp, could be consolidated */
    /* define CP(X,Y) Tape23of3(Align2(X,Y)) & [ "@I[@"  ? ? ["@I@" ? ?]* "@I]@" ? ? | "@I[]@" ? ? | 0 ] */
    let mut aligned = rewrite_align_markup(opts, upper, lower1, lower2);
    aligned = rewrite_tape_m_to_n_of_k(opts, aligned, 3, 4, 4);
    let threetape = fsm_minimize(opts, fsm_intersect(opts, aligned, rewrite_itape(opts, rb)));
    let rulenumtape = rewrite_tape_m_to_n_of_k(
        opts,
        fsm_minimize(
            opts,
            fsm_kleene_star(
                opts,
                fsm_symbol(&rb.namestrings[(rule_number - 1) as usize]),
            ),
        ),
        2,
        2,
        4,
    );
    fsm_minimize(opts, fsm_intersect(opts, threetape, rulenumtape))
}

// [spec:foma:def:rewrite.rewrite-cp-transducer-fn]
// [spec:foma:sem:rewrite.rewrite-cp-transducer-fn]
pub fn rewrite_cp_transducer(
    opts: &FomaOptions,
    rb: &mut RewriteBatch,
    t: Box<Fsm>,
    rule_number: i32,
) -> Box<Fsm> {
    /* C: fsm_flatten's NULL return is a dead branch (see the fsm-flatten
    sem rule) — unwrap here */
    let mut aligned = fsm_flatten(opts, t, fsm_symbol("@0@"))
        .expect("flatten of internally-built transducer succeeds");
    aligned = rewrite_tape_m_to_n_of_k(opts, aligned, 3, 4, 4);
    let threetape = fsm_minimize(opts, fsm_intersect(opts, aligned, rewrite_itape(opts, rb)));
    let rulenumtape = rewrite_tape_m_to_n_of_k(
        opts,
        fsm_minimize(
            opts,
            fsm_kleene_star(
                opts,
                fsm_symbol(&rb.namestrings[(rule_number - 1) as usize]),
            ),
        ),
        2,
        2,
        4,
    );
    fsm_minimize(opts, fsm_intersect(opts, threetape, rulenumtape))
}

// [spec:foma:def:rewrite.rewrite-cp-fn]
// [spec:foma:sem:rewrite.rewrite-cp-fn]
pub fn rewrite_cp(
    opts: &FomaOptions,
    rb: &mut RewriteBatch,
    upper: Box<Fsm>,
    lower: Box<Fsm>,
    rule_number: i32,
) -> Box<Fsm> {
    /* define CP(X,Y) Tape23of3(Align2(X,Y)) & [ "@I[@"  ? ? ["@I@" ? ?]* "@I]@" ? ? | "@I[]@" ? ? | 0 ] */
    let mut aligned = rewrite_align(opts, upper, lower);
    aligned = rewrite_tape_m_to_n_of_k(opts, aligned, 3, 4, 4);
    let threetape = fsm_minimize(opts, fsm_intersect(opts, aligned, rewrite_itape(opts, rb)));
    let rulenumtape = rewrite_tape_m_to_n_of_k(
        opts,
        fsm_minimize(
            opts,
            fsm_kleene_star(
                opts,
                fsm_symbol(&rb.namestrings[(rule_number - 1) as usize]),
            ),
        ),
        2,
        2,
        4,
    );
    fsm_minimize(opts, fsm_intersect(opts, threetape, rulenumtape))
}

// [spec:foma:def:rewrite.rewrite-add-special-syms-fn]
// [spec:foma:sem:rewrite.rewrite-add-special-syms-fn]
pub fn rewrite_add_special_syms(rb: &RewriteBatch, net: Option<&mut Fsm>) {
    let net = match net {
        Some(net) => net,
        None => return,
    };
    /* We convert boundaries to our internal rep; the substituted number is
    unused (C discarded the int too). This is because sigma merging
    (fsm_merge_sigma(opts, )) is handled in a special way for .#., which we
    don't want here. */
    let _ = sigma_substitute(".#.", "@#@", &mut net.sigma);

    /* C: for (i = 0; specialsymbols[i] != NULL; i++) */
    let mut i: usize = 0;
    while i < SPECIALSYMBOLS.len() {
        if sigma_find(SPECIALSYMBOLS[i], &net.sigma).is_none() {
            sigma_add(SPECIALSYMBOLS[i], &mut net.sigma);
        }
        i += 1;
    }
    let mut i: i32 = 1;
    while i <= rb.num_rules {
        sigma_add(&rb.namestrings[(i - 1) as usize], &mut net.sigma);
        i += 1;
    }
    sigma_sort(net);
}

// [spec:foma:def:rewrite.fsm-clear-contexts-fn]
// [spec:foma:sem:rewrite.fsm-clear-contexts-fn]
// [spec:foma:def:fomalib.fsm-clear-contexts-fn]
// [spec:foma:sem:fomalib.fsm-clear-contexts-fn]
pub fn fsm_clear_contexts(contexts: Option<Box<Fsmcontexts>>) {
    let mut c = contexts;
    while let Some(mut node) = c {
        /* fsm_destroy tolerates NULL in C; the Option checks stand in */
        if let Some(net) = node.left.take() {
            fsm_destroy(net);
        }
        if let Some(net) = node.right.take() {
            fsm_destroy(net);
        }
        if let Some(net) = node.cpleft.take() {
            fsm_destroy(net);
        }
        if let Some(net) = node.cpright.take() {
            fsm_destroy(net);
        }
        let cp = node.next.take();
        /* free(c) */
        drop(node);
        c = cp;
    }
}

// [spec:foma:def:rewrite.rewr-context-restrict-fn]
// [spec:foma:sem:rewrite.rewr-context-restrict-fn]
pub fn rewr_context_restrict(
    opts: &FomaOptions,
    rb: &mut RewriteBatch,
    x: &mut Fsm,
    lr: Option<&mut Fsmcontexts>,
) -> Box<Fsm> {
    let mut var = fsm_symbol("@VARX@");
    //Notvar = fsm_minimize(fsm_kleene_star(fsm_term_negation(fsm_symbol("@VARX@"))));
    let mut notvar = fsm_minus(
        opts,
        rewrite_any_4tape(opts, rb),
        fsm_contains(opts, fsm_symbol("@VARX@")),
    );
    /* We add the variable symbol to all alphabets to avoid ? matching it */
    /* which would cause extra nondeterminism */

    let mut newx = fsm_copy(x);
    if sigma_find("@VARX@", &newx.sigma).is_none() {
        sigma_add("@VARX@", &mut newx.sigma);
        sigma_sort(&mut newx);
    }
    let mut union_p = fsm_empty_set();

    let mut pairs = lr;
    while let Some(p) = pairs {
        let left = if p.left.is_none() {
            fsm_empty_string()
        } else {
            let mut l = fsm_copy(
                p.cpleft
                    .as_deref_mut()
                    .expect("context cpleft built for this op"),
            );
            sigma_add("@VARX@", &mut l.sigma);
            sigma_sort(&mut l);
            l
        };
        let right = if p.right.is_none() {
            fsm_empty_string()
        } else {
            let mut r = fsm_copy(
                p.cpright
                    .as_deref_mut()
                    .expect("context cpright built for this op"),
            );
            sigma_add("@VARX@", &mut r.sigma);
            sigma_sort(&mut r);
            r
        };
        union_p = fsm_union(
            opts,
            fsm_concat(
                opts,
                left,
                fsm_concat(
                    opts,
                    fsm_copy(&mut var),
                    fsm_concat(
                        opts,
                        fsm_copy(&mut notvar),
                        fsm_concat(opts, fsm_copy(&mut var), right),
                    ),
                ),
            ),
            union_p,
        );
        pairs = p.next.as_deref_mut();
    }
    let union_l = fsm_concat(
        opts,
        fsm_copy(&mut notvar),
        fsm_concat(
            opts,
            fsm_copy(&mut var),
            fsm_concat(
                opts,
                fsm_copy(&mut newx),
                fsm_concat(opts, fsm_copy(&mut var), fsm_copy(&mut notvar)),
            ),
        ),
    );
    let mut result = fsm_minus(
        opts,
        union_l,
        fsm_concat(
            opts,
            fsm_copy(&mut notvar),
            fsm_concat(opts, fsm_copy(&mut union_p), fsm_copy(&mut notvar)),
        ),
    );

    if sigma_find("@VARX@", &result.sigma).is_some() {
        result = fsm_complement(
            opts,
            fsm_substitute_symbol(result, "@VARX@", "@_EPSILON_SYMBOL_@"),
        );
    } else {
        result = fsm_complement(opts, result);
    }
    fsm_destroy(union_p);
    fsm_destroy(var);
    fsm_destroy(notvar);
    fsm_destroy(newx);
    result
}

// [spec:foma:def:rewrite.rewrite-epextend-fn]
// [spec:foma:sem:rewrite.rewrite-epextend-fn]
pub fn rewrite_epextend(opts: &FomaOptions, rb: &mut RewriteBatch) -> Box<Fsm> {
    /* 1.  @O@   @0@     [ANY|@#@] @ID@           */
    /* 2.  @I[]@ @#Rule@ [ANY]     [@ID@|@0@|ANY] */
    /* 3a. @I[@  @#Rule@ [ANY]     [@ID@|@0@|ANY] */
    /* 3b. @I@   @#Rule@ [ANY]     [@ID@|@0@|ANY] */
    /* 3c. @I]@  @#Rule@ [ANY]     [@ID@|@0@|ANY] */
    /* 3.  [3a|3b|3c] & ~[[? ? "@0@" ?]*]         */

    /* TODO lower version as well */

    if rb.epextend.is_none() {
        let one = fsm_minimize(
            opts,
            fsm_concat(
                opts,
                fsm_symbol("@O@"),
                fsm_concat(
                    opts,
                    fsm_symbol("@0@"),
                    fsm_concat(
                        opts,
                        fsm_union(
                            opts,
                            fsm_copy(
                                rb.any
                                    .as_deref_mut()
                                    .expect("rb.any built at fsm_rewrite start"),
                            ),
                            fsm_symbol("@#@"),
                        ),
                        fsm_symbol("@ID@"),
                    ),
                ),
            ),
        );
        let two = fsm_minimize(
            opts,
            fsm_concat(
                opts,
                fsm_symbol("@I[]@"),
                fsm_concat(
                    opts,
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                    fsm_concat(
                        opts,
                        fsm_copy(
                            rb.any
                                .as_deref_mut()
                                .expect("rb.any built at fsm_rewrite start"),
                        ),
                        fsm_union(
                            opts,
                            fsm_symbol("@0@"),
                            fsm_union(
                                opts,
                                fsm_symbol("@ID@"),
                                fsm_copy(
                                    rb.any
                                        .as_deref_mut()
                                        .expect("rb.any built at fsm_rewrite start"),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let allzeroupper = fsm_parse_regex(opts, "~[[? ? \"@0@\" ?]*]", None, None)
            .expect("constant rewrite regex compiles");
        let threea = fsm_minimize(
            opts,
            fsm_concat(
                opts,
                fsm_symbol("@I[@"),
                fsm_concat(
                    opts,
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                    fsm_concat(
                        opts,
                        fsm_union(
                            opts,
                            fsm_copy(
                                rb.any
                                    .as_deref_mut()
                                    .expect("rb.any built at fsm_rewrite start"),
                            ),
                            fsm_symbol("@0@"),
                        ),
                        fsm_union(
                            opts,
                            fsm_symbol("@0@"),
                            fsm_union(
                                opts,
                                fsm_symbol("@ID@"),
                                fsm_copy(
                                    rb.any
                                        .as_deref_mut()
                                        .expect("rb.any built at fsm_rewrite start"),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let threeb = fsm_minimize(
            opts,
            fsm_kleene_star(
                opts,
                fsm_concat(
                    opts,
                    fsm_symbol("@I@"),
                    fsm_concat(
                        opts,
                        fsm_copy(
                            rb.rulenames
                                .as_deref_mut()
                                .expect("rb.rulenames built at fsm_rewrite start"),
                        ),
                        fsm_concat(
                            opts,
                            fsm_union(
                                opts,
                                fsm_copy(
                                    rb.any
                                        .as_deref_mut()
                                        .expect("rb.any built at fsm_rewrite start"),
                                ),
                                fsm_symbol("@0@"),
                            ),
                            fsm_union(
                                opts,
                                fsm_symbol("@0@"),
                                fsm_union(
                                    opts,
                                    fsm_symbol("@ID@"),
                                    fsm_copy(
                                        rb.any
                                            .as_deref_mut()
                                            .expect("rb.any built at fsm_rewrite start"),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let threec = fsm_minimize(
            opts,
            fsm_concat(
                opts,
                fsm_symbol("@I]@"),
                fsm_concat(
                    opts,
                    fsm_copy(
                        rb.rulenames
                            .as_deref_mut()
                            .expect("rb.rulenames built at fsm_rewrite start"),
                    ),
                    fsm_concat(
                        opts,
                        fsm_union(
                            opts,
                            fsm_copy(
                                rb.any
                                    .as_deref_mut()
                                    .expect("rb.any built at fsm_rewrite start"),
                            ),
                            fsm_symbol("@0@"),
                        ),
                        fsm_union(
                            opts,
                            fsm_symbol("@0@"),
                            fsm_union(
                                opts,
                                fsm_symbol("@ID@"),
                                fsm_copy(
                                    rb.any
                                        .as_deref_mut()
                                        .expect("rb.any built at fsm_rewrite start"),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        );
        let three = fsm_intersect(
            opts,
            allzeroupper,
            fsm_concat(opts, threea, fsm_concat(opts, threeb, threec)),
        );
        rb.epextend = Some(fsm_minimize(
            opts,
            fsm_union(opts, fsm_union(opts, one, two), three),
        ));
    }
    fsm_copy(
        rb.epextend
            .as_deref_mut()
            .expect("rb.epextend built before use"),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        RewriteBatch, fsm_clear_contexts, fsm_rewrite, rewr_contains, rewrite_add_special_syms,
        rewrite_align,
    };
    use crate::apply::{apply_down, apply_init, apply_up, apply_words};
    use crate::constructions::{fsm_intersect, fsm_symbol, fsm_union};
    use crate::minimize::fsm_minimize;
    use crate::options::FomaOptions;
    use crate::regex::fsm_parse_regex;
    use crate::structures::{fsm_copy, fsm_empty_set, fsm_empty_string, fsm_identity, fsm_isempty};
    use crate::types::{ARROW_RIGHT, Fsm, Fsmcontexts, Fsmrules, OP_TWO_LEVEL_REPLACE, RewriteSet};

    /* All lower-side outputs of `word` (apply down), sorted+deduped. */
    fn down_all(net: &Fsm, word: &str) -> Vec<String> {
        let mut h = apply_init(net);
        let mut v = Vec::new();
        let mut r = apply_down(&mut h, Some(word));
        while let Some(s) = r {
            v.push(s);
            r = apply_down(&mut h, None);
        }
        v.sort();
        v.dedup();
        v
    }

    /* All upper-side inputs mapping to `word` (apply up). */
    fn up_all(net: &Fsm, word: &str) -> Vec<String> {
        let mut h = apply_init(net);
        let mut v = Vec::new();
        let mut r = apply_up(&mut h, Some(word));
        while let Some(s) = r {
            v.push(s);
            r = apply_up(&mut h, None);
        }
        v.sort();
        v.dedup();
        v
    }

    /* Whole (finite) accepted language via apply_words. */
    fn all_words(net: &Fsm) -> Vec<String> {
        let mut h = apply_init(net);
        let mut v = Vec::new();
        let mut r = apply_words(&mut h);
        while let Some(s) = r {
            v.push(s);
            r = apply_words(&mut h);
        }
        v.sort();
        v.dedup();
        v
    }

    /* A minimal rewrite_batch (num_rules == 0) for direct-call tests, built the
    same way fsm_rewrite bootstraps rb. */
    fn mini_rb() -> RewriteBatch {
        let opts = &FomaOptions::default();
        let mut rb = RewriteBatch {
            rewrite_set: None,
            rulenames: None,
            isyms: None,
            any: None,
            iopen: None,
            iclose: None,
            itape: None,
            any4tape: None,
            epextend: None,
            num_rules: 0,
            namestrings: Vec::new(),
        };
        rb.isyms = Some(fsm_minimize(
            opts,
            fsm_union(
                opts,
                fsm_symbol("@I@"),
                fsm_union(
                    opts,
                    fsm_symbol("@I[]@"),
                    fsm_union(opts, fsm_symbol("@I[@"), fsm_symbol("@I]@")),
                ),
            ),
        ));
        rb.rulenames = Some(fsm_empty_set());
        let mut any = fsm_identity();
        rewrite_add_special_syms(&rb, Some(&mut any));
        rb.any = Some(any);
        rb
    }

    // Simple obligatory replacement `a -> b`. Drives the whole compiler:
    // rewrite_cp -> rewrite_align -> rewrite_tape_m_to_n_of_k / rewrite_itape;
    // Base uses rewrite_any_4tape and Outside; the ARROW_RIGHT obligatory
    // constraint is C = rewr_unrewritten(left), subtracted via rewr_contains;
    // rule sigmas get rewrite_add_special_syms; rb torn down by rewrite_cleanup.
    // [spec:foma:sem:rewrite.fsm-rewrite-fn/test]
    // [spec:foma:sem:fomalib.fsm-rewrite-fn/test]
    // [spec:foma:sem:rewrite.rewrite-cp-fn/test]
    // [spec:foma:sem:rewrite.rewrite-itape-fn/test]
    // [spec:foma:sem:rewrite.rewrite-any-4tape-fn/test]
    // [spec:foma:sem:rewrite.rewrite-tape-m-to-n-of-k-fn/test]
    // [spec:foma:sem:rewrite.rewr-unrewritten-fn/test]
    // [spec:foma:sem:rewrite.rewrite-add-special-syms-fn/test]
    // [spec:foma:sem:rewrite.rewrite-cleanup-fn/test]
    #[test]
    fn rewrite_simple_replacement() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a -> b", None, None).unwrap();
        assert_eq!(down_all(&net, "aaa"), vec!["bbb"]);
        assert_eq!(down_all(&net, "bab"), vec!["bbb"]);
        assert_eq!(down_all(&net, ""), vec![""]);
        /* Every {a,b}^3 upper string maps down to "bbb". */
        assert_eq!(up_all(&net, "bbb").len(), 8);
        /* No upper string produces a lower "a" (a is obligatorily rewritten). */
        assert_eq!(up_all(&net, "bab"), Vec::<String>::new());
    }

    // Optional replacement `a (->) b`: both a and b are valid outputs (the
    // obligatory unrewritten constraint is skipped for ARROW_OPTIONAL).
    // [spec:foma:sem:rewrite.fsm-rewrite-fn/test]
    // [spec:foma:sem:fomalib.fsm-rewrite-fn/test]
    #[test]
    fn rewrite_optional() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a (->) b", None, None).unwrap();
        assert_eq!(down_all(&net, "a"), vec!["a", "b"]);
    }

    // Contextual rule with an upper context `a -> b || l _ r` (OP_UPWARD_REPLACE
    // -> rewrite_upper for both context sides; rewr_context_restrict).
    // [spec:foma:sem:rewrite.rewrite-upper-fn/test]
    // [spec:foma:sem:rewrite.rewr-context-restrict-fn/test]
    #[test]
    fn rewrite_contextual_upper() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a -> b || l _ r", None, None).unwrap();
        assert_eq!(down_all(&net, "lar"), vec!["lbr"]);
        assert_eq!(down_all(&net, "aaa"), vec!["aaa"]);
        assert_eq!(down_all(&net, "lara"), vec!["lbra"]);
    }

    // Contextual rule with a lower context `a -> b \/ l _ r` (OP_DOWNWARD_REPLACE
    // -> rewrite_lower for both context sides).
    // [spec:foma:sem:rewrite.rewrite-lower-fn/test]
    #[test]
    fn rewrite_contextual_lower() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a -> b \\/ l _ r", None, None).unwrap();
        assert_eq!(down_all(&net, "lar"), vec!["lbr"]);
        assert_eq!(down_all(&net, "aar"), vec!["aar"]);
    }

    // Longest-match `a+ @-> x`: the whole run collapses to one x; rewr_notleftmost
    // and rewr_notlongest supply the longest-match violation constraints.
    // [spec:foma:sem:rewrite.rewr-notleftmost-fn/test]
    // [spec:foma:sem:rewrite.rewr-notlongest-fn/test]
    #[test]
    fn rewrite_longest_match() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a+ @-> x", None, None).unwrap();
        assert_eq!(down_all(&net, "aaa"), vec!["x"]);
        assert_eq!(down_all(&net, "aaba"), vec!["xbx"]);
    }

    // Shortest-match `a+ @> x`: each single a becomes x; rewr_notshortest.
    // [spec:foma:sem:rewrite.rewr-notshortest-fn/test]
    #[test]
    fn rewrite_shortest_match() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a+ @> x", None, None).unwrap();
        assert_eq!(down_all(&net, "aaa"), vec!["xxx"]);
    }

    // Epenthesis `[..] -> x`: inserts x at every position (dotted rule ->
    // rewrite_epextend flanking constraint).
    // [spec:foma:sem:rewrite.rewrite-epextend-fn/test]
    #[test]
    fn rewrite_epenthesis() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "[..] -> x", None, None).unwrap();
        assert_eq!(down_all(&net, ""), vec!["x"]);
        assert_eq!(down_all(&net, "ab"), vec!["xaxbx"]);
    }

    // Markup rule `a -> b ... c`: a is kept, b inserted before, c after
    // (rewrite_cp_markup -> rewrite_align_markup).
    // [spec:foma:sem:rewrite.rewrite-cp-markup-fn/test]
    // [spec:foma:sem:rewrite.rewrite-align-markup-fn/test]
    #[test]
    fn rewrite_markup() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a -> b ... c", None, None).unwrap();
        assert_eq!(down_all(&net, "a"), vec!["bac"]);
        assert_eq!(down_all(&net, "aa"), vec!["bacbac"]);
    }

    // Transducer-center rule (rules->right == NULL): the center a:b is flattened
    // by rewrite_cp_transducer, then split into upper=a / lower=b and compiled
    // like `a -> b`. Not reachable from the regex grammar, so built by hand.
    // [spec:foma:sem:rewrite.rewrite-cp-transducer-fn/test]
    #[test]
    fn rewrite_transducer_center() {
        let opts = &FomaOptions::default();
        let center = fsm_parse_regex(opts, "a:b", None, None).unwrap();
        let rule = Box::new(Fsmrules {
            left: Some(center),
            right: None,
            right2: None,
            cross_product: None,
            next: None,
            arrow_type: ARROW_RIGHT,
            dotted: 0,
        });
        let mut rs = RewriteSet {
            rewrite_rules: Some(rule),
            rewrite_contexts: None,
            next: None,
            rule_direction: 0,
        };
        let net = fsm_rewrite(opts, &mut rs);
        assert_eq!(down_all(&net, "a"), vec!["b"]);
        assert_eq!(down_all(&net, "aa"), vec!["bb"]);
        assert_eq!(down_all(&net, "b"), vec!["b"]);
        assert_eq!(up_all(&net, "b"), vec!["a", "b"]);
    }

    // Two-level context (rule_direction == OP_TWO_LEVEL_REPLACE): the contexts
    // are compiled by rewrite_two_level. Not reachable from the regex grammar,
    // so built by hand with identity contexts l / r, which behave like a normal
    // `a -> b || l _ r`.
    // [spec:foma:sem:rewrite.rewrite-two-level-fn/test]
    #[test]
    fn rewrite_two_level_context() {
        let opts = &FomaOptions::default();
        let rule = Box::new(Fsmrules {
            left: Some(fsm_symbol("a")),
            right: Some(fsm_symbol("b")),
            right2: None,
            cross_product: None,
            next: None,
            arrow_type: ARROW_RIGHT,
            dotted: 0,
        });
        let ctx = Box::new(Fsmcontexts {
            left: Some(fsm_symbol("l")),
            right: Some(fsm_symbol("r")),
            next: None,
            cpleft: None,
            cpright: None,
        });
        let mut rs = RewriteSet {
            rewrite_rules: Some(rule),
            rewrite_contexts: Some(ctx),
            next: None,
            rule_direction: OP_TWO_LEVEL_REPLACE,
        };
        let net = fsm_rewrite(opts, &mut rs);
        assert_eq!(down_all(&net, "lar"), vec!["lbr"]);
        assert_eq!(down_all(&net, "aar"), vec!["aar"]);
    }

    // rewrite_align on a tiny pair (a, b): flattened 2-tape alternating encoding
    // of the pair — the single flat word "a" "b".
    // [spec:foma:sem:rewrite.rewrite-align-fn/test]
    #[test]
    fn rewrite_align_tiny_pair() {
        let opts = &FomaOptions::default();
        let net = rewrite_align(opts, fsm_symbol("a"), fsm_symbol("b"));
        assert_eq!(all_words(&net), vec!["ab"]);
    }

    // rewr_contains builds Any4 . lang . Any4 with NO complement (despite the
    // "NotContain" comment): for a nonempty lang the empty string is NOT in the
    // result, whereas the complement would contain it.
    // [spec:foma:sem:rewrite.rewr-contains-fn/test]
    #[test]
    fn rewr_contains_no_complement() {
        let opts = &FomaOptions::default();
        let mut rb = mini_rb();
        let mut net = rewr_contains(opts, &mut rb, fsm_symbol("a"));
        assert!(
            !(fsm_isempty(opts, &mut net)),
            "containment language is non-empty"
        );
        let mut probe = fsm_intersect(opts, fsm_copy(&mut net), fsm_empty_string());
        assert!(
            fsm_isempty(opts, &mut probe),
            "empty string not accepted -> no complement was taken"
        );
    }

    // fsm_clear_contexts: None is a no-op; a non-empty list is consumed (its
    // nets destroyed) without panic.
    // [spec:foma:sem:rewrite.fsm-clear-contexts-fn/test]
    // [spec:foma:sem:fomalib.fsm-clear-contexts-fn/test]
    #[test]
    fn clear_contexts() {
        fsm_clear_contexts(None);
        let ctx = Box::new(Fsmcontexts {
            left: Some(fsm_symbol("l")),
            right: Some(fsm_symbol("r")),
            next: Some(Box::new(Fsmcontexts {
                left: Some(fsm_symbol("x")),
                right: None,
                next: None,
                cpleft: Some(fsm_symbol("c")),
                cpright: None,
            })),
            cpleft: None,
            cpright: None,
        });
        fsm_clear_contexts(Some(ctx));
    }
}
