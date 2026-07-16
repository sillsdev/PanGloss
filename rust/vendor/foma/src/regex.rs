//! foma/regex.y + regex.l (the regex compiler).
//!
//! Wave-2 wiring rather than literal translation: foma's flex/bison grammar is
//! replaced by the `nfst-xre` parser, which produces a typed `XreExpr` AST. We
//! walk that AST and call the same construction functions the C grammar's
//! semantic actions would, so the OBSERVABLE net for a given regex matches the
//! C compositions bug-for-bug (see docs/port/rust-conventions.md).
//!
//! Mapping authority is foma/regex.y (production -> construction call) and
//! foma/regex.l (symbol handling, defined-symbol substitution, @"file" loads,
//! NAME(args) function application). Where nfst-xre's AST covers syntax the C
//! grammar lacks (merge ops, weights, .-u./.-l., @pl"…"), we return None with a
//! diagnostic. Where the C grammar covers syntax nfst-xre cannot lex (the
//! `_foo(` internal builtins, quantifiers ∀/∃, VAR logic, right/interleave
//! quotients, `.f` flag-eliminate, two-level `|||` replace), those simply never
//! reach us as AST nodes (they lex/parse to something else or error out).

use crate::options::FomaOptions;

use nfst_xre::{
    BinaryOp, ContextMark, MappingKind, MappingPair, MappingSide, ReadKind, ReplaceArrow,
    ReplaceRule, RestrContext, SpannedXre, SubstituteWhat, UnaryOp, XreExpr,
};

use crate::constructions::{
    fsm_complement, fsm_compose, fsm_concat, fsm_concat_m_n, fsm_concat_n, fsm_contains,
    fsm_contains_one, fsm_contains_opt_one, fsm_context_restrict, fsm_cross_product, fsm_follows,
    fsm_ignore, fsm_intersect, fsm_invert, fsm_kleene_plus, fsm_kleene_star, fsm_lenient_compose,
    fsm_minus, fsm_optionality, fsm_precedes, fsm_priority_union_lower, fsm_priority_union_upper,
    fsm_quotient_left, fsm_shuffle, fsm_substitute_symbol, fsm_symbol, fsm_term_negation,
    fsm_union,
};
use crate::define::{add_defined, find_defined, find_defined_function, remove_defined};
use crate::determinize::fsm_determinize;
use crate::extract::{fsm_lower, fsm_upper};
use crate::io::{file_to_mem, fsm_read_binary_file, fsm_read_spaced_text_file, fsm_read_text_file};
use crate::minimize::fsm_minimize;
use crate::reverse::fsm_reverse;
use crate::rewrite::fsm_rewrite;
use crate::structures::{fsm_copy, fsm_destroy, fsm_empty_string, fsm_identity, fsm_isempty};
use crate::types::{
    ARROW_DOTTED, ARROW_LEFT, ARROW_LEFT_TO_RIGHT, ARROW_LONGEST_MATCH, ARROW_OPTIONAL,
    ARROW_RIGHT, ARROW_RIGHT_TO_LEFT, ARROW_SHORTEST_MATCH, DefinedFunctions, DefinedNetworks, Fsm,
    Fsmcontexts, Fsmrules, OP_DOWNWARD_REPLACE, OP_IGNORE_ALL, OP_IGNORE_INTERNAL,
    OP_LEFTWARD_REPLACE, OP_RIGHTWARD_REPLACE, OP_UPWARD_REPLACE, RewriteSet,
};
use crate::utf8::replace_equal_len;

/* C: `#define MAX_PARSE_DEPTH 100` — the self-recursion guard for my_yyparse. */
const MAX_PARSE_DEPTH: i32 = 100;

/// The parse-scoped state that C kept in the file-static `g_parse_depth`
/// (regex.l, the self-recursion guard) and `g_internal_sym` (regex.y, the
/// running counter for the unique temporary symbol names function application
/// synthesizes). Both survive the nested `my_yyparse` reparse a function
/// application triggers, so one `&mut ParseState` is threaded through the whole
/// recursive walk. A fresh `ParseState` is created for each top-level parse.
struct ParseState {
    /* C: `int g_parse_depth = 0;` */
    depth: i32,
    /* C: `unsigned int g_internal_sym = 23482342;` */
    internal_sym: u32,
}

impl ParseState {
    fn new() -> ParseState {
        ParseState {
            depth: 0,
            internal_sym: 23482342,
        }
    }
}

// [spec:foma:def:fomalib.fsm-parse-regex-fn]
// [spec:foma:sem:fomalib.fsm-parse-regex-fn]
pub fn fsm_parse_regex(
    opts: &FomaOptions,
    regex: &str,
    defined_nets: Option<&mut DefinedNetworks>,
    defined_funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    /* C: strcpy a copy of `regex` with ";" appended, my_yyparse it at line 1,
    and on success return fsm_minimize(opts, current_parse). nfst-xre tolerates the
    optional trailing ";" itself, so no copy is needed. */
    let mut ps = ParseState::new();
    let current_parse = my_yyparse(opts, &mut ps, regex, defined_nets, defined_funcs)?;
    Some(fsm_minimize(opts, current_parse))
}

// [spec:foma:def:foma.my-yyparse-fn]
// [spec:foma:sem:foma.my-yyparse-fn]
fn my_yyparse(
    opts: &FomaOptions,
    ps: &mut ParseState,
    regex: &str,
    defined_nets: Option<&mut DefinedNetworks>,
    defined_funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    /* C: depth-limited reentrant driver. The C also saves/restores the global
    parser state (rewrite/contexts/rules/rewrite_rules) around the nested
    yyparse; this port builds those structures locally on the stack, so only
    the depth guard is observable. Returns the net the parse deposits in
    current_parse (unminimized — fsm_parse_regex/@re do the minimize). */
    if ps.depth >= MAX_PARSE_DEPTH {
        tracing::error!("Exceeded parser stack depth.  Self-recursive call?");
        return None;
    }
    ps.depth += 1;
    let result = my_yyparse_inner(opts, ps, regex, defined_nets, defined_funcs);
    ps.depth -= 1;
    result
}

fn my_yyparse_inner(
    opts: &FomaOptions,
    ps: &mut ParseState,
    regex: &str,
    defined_nets: Option<&mut DefinedNetworks>,
    defined_funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    let exprs = match nfst_xre::parse_all(regex) {
        Ok(e) => e,
        Err(e) => {
            /* C's my_yyparse returns non-zero on a syntax error; yyerror has
            already printed a "***...at '...'" diagnostic. */
            let msg = e
                .diagnostics
                .first()
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "syntax error".to_string());
            tracing::error!("Syntax error: {}", msg);
            return None;
        }
    };
    /* C grammar: `start: regex | regex start` with `regex: network END
    { current_parse = $1; }` — current_parse ends up as the LAST network
    parsed. */
    let last = match exprs.last() {
        Some(e) => e,
        None => {
            tracing::error!("Syntax error: empty regular expression");
            return None;
        }
    };
    build_net(opts, ps, &last.value, defined_nets, defined_funcs)
}

/// Walk one AST node to a network, mirroring the regex.y semantic action for
/// the corresponding production.
fn build_net(
    opts: &FomaOptions,
    ps: &mut ParseState,
    expr: &XreExpr,
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    match expr {
        // ──────────────── atoms ────────────────
        XreExpr::Symbol(s) => {
            /* regex.l NONRESERVED path: substitute a defined net when the
            symbol names one; otherwise it is a literal single symbol. (`0`
            and `?` arrive as Epsilon/Any; a Symbol("0")/Symbol("?") means the
            user escaped it as %0/%?, so it is NOT special-cased here.) */
            if let Some(n) = nets.as_deref_mut() {
                if let Some(found) = find_defined(n, s) {
                    return Some(fsm_copy(found));
                }
            }
            Some(fsm_symbol(s))
        }
        XreExpr::Curly(s) => {
            /* regex.l {BRACED}: nfst-xre delivers the braces' interior, which
            is exactly fsm_explode's payload */
            Some(crate::constructions::fsm_explode(s))
        }
        XreExpr::Epsilon => Some(fsm_empty_string()),
        XreExpr::Any => Some(fsm_identity()),
        XreExpr::BoundaryMarker => Some(fsm_symbol(".#.")),

        // ──────────────── label combinators ────────────────
        XreExpr::Pair { upper, lower } => {
            /* `:` HIGH_CROSS_PRODUCT: fsm_cross_product(upper, lower). */
            let u = build_net(
                opts,
                ps,
                &upper.value,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?;
            let l = build_net(
                opts,
                ps,
                &lower.value,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?;
            Some(fsm_cross_product(opts, u, l))
        }
        XreExpr::Weighted { .. } => {
            tracing::error!("Syntax error: weights (::w) are not supported");
            None
        }
        XreExpr::ContainmentWithWeight { .. } => {
            tracing::error!("Syntax error: weighted containment ($::w) is not supported");
            None
        }

        XreExpr::ReadFile { kind, path } => build_read_file(opts, ps, *kind, path, nets, funcs),

        XreExpr::FunctionCall { name, args } => function_apply(opts, ps, name, args, nets, funcs),

        // ──────────────── grouping ────────────────
        XreExpr::Group(inner) => build_net(opts, ps, &inner.value, nets, funcs),
        XreExpr::Optional(inner) => {
            /* regex.y LPAREN network RPAREN: fsm_optionality (no quantifier can
            reach us, so count_quantifiers() is always 0 here). */
            let n = build_net(opts, ps, &inner.value, nets, funcs)?;
            Some(fsm_optionality(opts, n))
        }
        XreExpr::BracketedDotted(_) => {
            /* `[. E .]` outside a replacement mapping is a syntax error in the C
            grammar (LDOT/RDOT only appear inside rule productions). */
            tracing::error!("Syntax error: [. .] is only valid as a replacement mapping side");
            None
        }

        // ──────────────── unary ────────────────
        XreExpr::Unary(op, inner) => build_unary(opts, ps, *op, &inner.value, nets, funcs),

        // ──────────────── binary ────────────────
        XreExpr::Binary(op, l, r) => build_binary(opts, ps, *op, &l.value, &r.value, nets, funcs),

        // ──────────────── iteration ────────────────
        XreExpr::RepeatN(inner, n) => {
            /* NCONCAT: fsm_concat_n(net, n). */
            let net = build_net(opts, ps, &inner.value, nets, funcs)?;
            Some(fsm_concat_n(opts, net, *n as i32))
        }
        XreExpr::RepeatNPlus(inner, n) => {
            /* MORENCONCAT (`^>N`): concat(concat_n(copy,n), kleene_plus(copy)). */
            let mut net = build_net(opts, ps, &inner.value, nets, funcs)?;
            let res = fsm_concat(
                opts,
                fsm_concat_n(opts, fsm_copy(&mut net), *n as i32),
                fsm_kleene_plus(opts, fsm_copy(&mut net)),
            );
            fsm_destroy(net);
            Some(res)
        }
        XreExpr::RepeatNMinus(inner, n) => {
            /* LESSNCONCAT (`^<N`): fsm_concat_m_n(net, 0, n-1). */
            let net = build_net(opts, ps, &inner.value, nets, funcs)?;
            Some(fsm_concat_m_n(opts, net, 0, *n as i32 - 1))
        }
        XreExpr::RepeatNToK(inner, n, k) => {
            /* MNCONCAT (`^N,K`): fsm_concat_m_n(net, n, k). */
            let net = build_net(opts, ps, &inner.value, nets, funcs)?;
            Some(fsm_concat_m_n(opts, net, *n as i32, *k as i32))
        }

        // ──────────────── replace / restriction / substitute ────────────────
        XreExpr::Replace { arrow, rules } => build_replace(opts, ps, *arrow, rules, nets, funcs),
        XreExpr::Restriction { body, contexts } => {
            build_restriction(opts, ps, &body.value, contexts, nets, funcs)
        }
        XreExpr::Substitute { haystack, what } => {
            build_substitute(opts, ps, &haystack.value, what, nets, funcs)
        }
    }
}

fn build_unary(
    opts: &FomaOptions,
    ps: &mut ParseState,
    op: UnaryOp,
    inner: &XreExpr,
    nets: Option<&mut DefinedNetworks>,
    funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    let net = build_net(opts, ps, inner, nets, funcs)?;
    Some(match op {
        /* network9 KLEENE_STAR: fsm_kleene_star(fsm_minimize(net)) */
        UnaryOp::Star => fsm_kleene_star(opts, fsm_minimize(opts, net)),
        UnaryOp::Plus => fsm_kleene_plus(opts, net),
        /* network9 REVERSE: fsm_determinize(fsm_reverse(net)) */
        UnaryOp::Reverse => fsm_determinize(fsm_reverse(net)),
        UnaryOp::Invert => fsm_invert(net),
        UnaryOp::UpperProject => fsm_upper(net),
        UnaryOp::LowerProject => fsm_lower(net),
        UnaryOp::Complement => fsm_complement(opts, net),
        UnaryOp::TermComplement => fsm_term_negation(opts, net),
        UnaryOp::Containment => fsm_contains(opts, net),
        UnaryOp::ContainmentOnce => fsm_contains_one(opts, net),
        UnaryOp::ContainmentOpt => fsm_contains_opt_one(opts, net),
    })
}

fn build_binary(
    opts: &FomaOptions,
    ps: &mut ParseState,
    op: BinaryOp,
    left: &XreExpr,
    right: &XreExpr,
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    let l = build_net(opts, ps, left, nets.as_deref_mut(), funcs.as_deref_mut())?;
    let r = build_net(opts, ps, right, nets, funcs)?;
    match op {
        BinaryOp::Concatenate => Some(fsm_concat(opts, l, r)),
        BinaryOp::Compose => Some(fsm_compose(opts, l, r)),
        BinaryOp::LenientCompose => Some(fsm_lenient_compose(opts, l, r)),
        BinaryOp::CrossProduct => Some(fsm_cross_product(opts, l, r)),
        BinaryOp::Union => Some(fsm_union(opts, l, r)),
        BinaryOp::Intersect => Some(fsm_intersect(opts, l, r)),
        BinaryOp::Subtract => Some(fsm_minus(opts, l, r)),
        BinaryOp::UpperPriorityUnion => Some(fsm_priority_union_upper(opts, l, r)),
        BinaryOp::LowerPriorityUnion => Some(fsm_priority_union_lower(opts, l, r)),
        BinaryOp::Ignoring => Some(fsm_ignore(opts, l, r, OP_IGNORE_ALL)),
        BinaryOp::IgnoreInternally => Some(fsm_ignore(opts, l, r, OP_IGNORE_INTERNAL)),
        BinaryOp::LeftQuotient => Some(fsm_quotient_left(opts, l, r)),
        BinaryOp::Shuffle => Some(fsm_shuffle(opts, l, r)),
        /* PRECEDES/FOLLOWS borrow (do not consume) their operands. */
        BinaryOp::Before => {
            let mut l = l;
            let mut r = r;
            let res = fsm_precedes(opts, &mut l, &mut r);
            fsm_destroy(l);
            fsm_destroy(r);
            Some(res)
        }
        BinaryOp::After => {
            let mut l = l;
            let mut r = r;
            let res = fsm_follows(opts, &mut l, &mut r);
            fsm_destroy(l);
            fsm_destroy(r);
            Some(res)
        }
        /* Operators nfst-xre can lex but the foma grammar has no production
        for: merges, upper/lower minus. */
        BinaryOp::MergeRight
        | BinaryOp::MergeLeft
        | BinaryOp::UpperSubtract
        | BinaryOp::LowerSubtract => {
            tracing::error!("Syntax error: operator not supported by foma regex grammar");
            fsm_destroy(l);
            fsm_destroy(r);
            None
        }
    }
}

fn build_read_file(
    opts: &FomaOptions,
    ps: &mut ParseState,
    kind: ReadKind,
    path: &str,
    nets: Option<&mut DefinedNetworks>,
    funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    match kind {
        /* regex.l @"…"/@bin"…": fsm_read_binary_file */
        ReadKind::Binary => match fsm_read_binary_file(path).ok() {
            Some(n) => Some(n),
            None => {
                tracing::error!("Error reading binary file '{}'", path);
                None
            }
        },
        ReadKind::Text => match fsm_read_text_file(path) {
            Some(n) => Some(n),
            None => {
                tracing::error!("Error reading text file '{}'", path);
                None
            }
        },
        ReadKind::Spaced => match fsm_read_spaced_text_file(path) {
            Some(n) => Some(n),
            None => {
                tracing::error!("Error reading spaced text file '{}'", path);
                None
            }
        },
        /* regex.l @re"…": file_to_mem then fsm_parse_regex_string (parse +
        minimize). */
        ReadKind::Regex => {
            let bytes = match file_to_mem(path).ok() {
                Some(b) => b,
                None => {
                    tracing::error!("Error reading regex file '{}'", path);
                    return None;
                }
            };
            let s = match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(_) => {
                    tracing::error!("Error: regex file '{}' is not valid UTF-8", path);
                    return None;
                }
            };
            Some(fsm_minimize(opts, my_yyparse(opts, ps, &s, nets, funcs)?))
        }
        ReadKind::Prolog => {
            tracing::error!("Syntax error: @pl\"…\" prolog files are not supported");
            None
        }
    }
}

// ───────────────────────── function application ─────────────────────────

/// regex.y function_apply: look up the function body regex by (name, numargs),
/// substitute each `@ARGUMENTNN@` with a unique temporary symbol, temporarily
/// define each argument net under that symbol, reparse the substituted regex,
/// then remove the temporaries.
fn function_apply(
    opts: &FomaOptions,
    ps: &mut ParseState,
    name: &str,
    args: &[SpannedXre],
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    let numargs = args.len() as i32;
    let body = match funcs.as_deref() {
        Some(f) => match find_defined_function(f, name, numargs) {
            Some(s) => s.to_string(),
            None => {
                tracing::error!("function {}@{}) not defined!", name, numargs);
                return None;
            }
        },
        None => {
            tracing::error!("function {}@{}) not defined!", name, numargs);
            return None;
        }
    };

    /* Build each argument net (C had these already built during the parse of
    the NAME(args) call). */
    let mut arg_nets: Vec<Box<Fsm>> = Vec::new();
    for a in args {
        arg_nets.push(build_net(
            opts,
            ps,
            &a.value,
            nets.as_deref_mut(),
            funcs.as_deref_mut(),
        )?);
    }

    let mut regex_bytes = body.into_bytes();
    let mut created: Vec<String> = Vec::new();
    for (i, argnet) in arg_nets.into_iter().enumerate() {
        let gsym = ps.internal_sym;
        /* C: sprintf(repstr, "%012X", g_internal_sym);
              sprintf(oldstr, "@ARGUMENT%02i@", i+1); — both 12 bytes wide, so
        streqrep's equal-length in-place replacement is valid. */
        let repstr = format!("{:012X}", gsym);
        let oldstr = format!("@ARGUMENT{:02}@", i + 1);
        replace_equal_len(&mut regex_bytes, oldstr.as_bytes(), repstr.as_bytes());
        match nets.as_deref_mut() {
            Some(n) => {
                add_defined(n, Some(argnet), &repstr);
            }
            None => {
                /* No registry to hold the temporary (only the internal
                None-table callers hit this; they never use functions). */
                fsm_destroy(argnet);
            }
        }
        created.push(repstr);
        ps.internal_sym = gsym.wrapping_add(1);
    }

    let regex_str = match String::from_utf8(regex_bytes) {
        Ok(s) => s,
        Err(_) => {
            if let Some(n) = nets.as_deref_mut() {
                for r in &created {
                    remove_defined(n, Some(r));
                }
            }
            tracing::error!("function {} produced a non-UTF-8 expansion", name);
            return None;
        }
    };

    let result = my_yyparse(opts, ps, &regex_str, nets.as_deref_mut(), funcs);

    if let Some(n) = nets {
        for r in &created {
            remove_defined(n, Some(r));
        }
    }
    result
}

// ───────────────────────── replacement rules ─────────────────────────

fn arrow_to_type(arrow: ReplaceArrow) -> i32 {
    match arrow {
        ReplaceArrow::Right => ARROW_RIGHT,
        ReplaceArrow::OptionalRight => ARROW_RIGHT | ARROW_OPTIONAL,
        ReplaceArrow::Left => ARROW_LEFT,
        ReplaceArrow::OptionalLeft => ARROW_LEFT | ARROW_OPTIONAL,
        ReplaceArrow::LeftRight => ARROW_LEFT | ARROW_RIGHT,
        ReplaceArrow::OptionalLeftRight => ARROW_LEFT | ARROW_RIGHT | ARROW_OPTIONAL,
        ReplaceArrow::LtrLongest => ARROW_RIGHT | ARROW_LONGEST_MATCH | ARROW_LEFT_TO_RIGHT,
        ReplaceArrow::LtrShortest => ARROW_RIGHT | ARROW_SHORTEST_MATCH | ARROW_LEFT_TO_RIGHT,
        ReplaceArrow::RtlLongest => ARROW_RIGHT | ARROW_LONGEST_MATCH | ARROW_RIGHT_TO_LEFT,
        ReplaceArrow::RtlShortest => ARROW_RIGHT | ARROW_SHORTEST_MATCH | ARROW_RIGHT_TO_LEFT,
    }
}

fn mark_to_dir(mark: ContextMark) -> i32 {
    match mark {
        ContextMark::UpperUpper => OP_UPWARD_REPLACE,
        ContextMark::LowerUpper => OP_RIGHTWARD_REPLACE,
        ContextMark::UpperLower => OP_LEFTWARD_REPLACE,
        ContextMark::LowerLower => OP_DOWNWARD_REPLACE,
    }
}

fn build_replace(
    opts: &FomaOptions,
    ps: &mut ParseState,
    arrow: ReplaceArrow,
    rules: &[ReplaceRule],
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    let arrow_type = arrow_to_type(arrow);

    /* Each ReplaceRule (a `,,`-separated block) becomes one rewrite_set node;
    each MappingPair inside becomes one (or two, for dotted) Fsmrules node.
    Rule/set ordering is observably irrelevant (the sets are unioned /
    intersected / subtracted and all internal rule markers are erased at the
    end), so we build in source order. */
    let mut set_nodes: Vec<RewriteSet> = Vec::new();
    for rule in rules {
        let mut rule_nodes: Vec<Fsmrules> = Vec::new();
        for mapping in &rule.mappings {
            build_mapping(
                opts,
                ps,
                mapping,
                arrow_type,
                &mut rule_nodes,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?;
        }
        let (contexts_chain, direction) = match &rule.contexts {
            Some(rc) => {
                let dir = mark_to_dir(rc.mark);
                let mut ctx_nodes: Vec<Fsmcontexts> = Vec::new();
                for item in &rc.items {
                    /* regex.y add_context_pair: a missing side stores
                    fsm_empty_string() (never NULL). */
                    let left = match &item.left {
                        Some(e) => build_net(
                            opts,
                            ps,
                            &e.value,
                            nets.as_deref_mut(),
                            funcs.as_deref_mut(),
                        )?,
                        None => fsm_empty_string(),
                    };
                    let right = match &item.right {
                        Some(e) => build_net(
                            opts,
                            ps,
                            &e.value,
                            nets.as_deref_mut(),
                            funcs.as_deref_mut(),
                        )?,
                        None => fsm_empty_string(),
                    };
                    ctx_nodes.push(Fsmcontexts {
                        left: Some(left),
                        right: Some(right),
                        next: None,
                        cpleft: None,
                        cpright: None,
                    });
                }
                (link_fsmcontexts(ctx_nodes), dir)
            }
            None => (None, 0),
        };
        set_nodes.push(RewriteSet {
            rewrite_rules: link_fsmrules(rule_nodes),
            rewrite_contexts: contexts_chain,
            next: None,
            rule_direction: direction,
        });
    }

    let mut head = link_rewritesets(set_nodes)?;
    /* networkA: fsm_rewrite(rewrite_rules); clear_rewrite_ruleset(...). */
    let net = fsm_rewrite(opts, &mut head);
    /* clear_rewrite_ruleset — the owned chain drops here. */
    drop(head);
    Some(net)
}

fn build_restriction(
    opts: &FomaOptions,
    ps: &mut ParseState,
    body: &XreExpr,
    contexts: &[RestrContext],
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    /* n0 CRESTRICT n0: fsm_context_restrict(body, contexts). */
    let x = build_net(opts, ps, body, nets.as_deref_mut(), funcs.as_deref_mut())?;
    let mut ctx_nodes: Vec<Fsmcontexts> = Vec::new();
    for item in contexts {
        let left = match &item.left {
            Some(e) => build_net(
                opts,
                ps,
                &e.value,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?,
            None => fsm_empty_string(),
        };
        let right = match &item.right {
            Some(e) => build_net(
                opts,
                ps,
                &e.value,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?,
            None => fsm_empty_string(),
        };
        ctx_nodes.push(Fsmcontexts {
            left: Some(left),
            right: Some(right),
            next: None,
            cpleft: None,
            cpright: None,
        });
    }
    Some(fsm_context_restrict(opts, x, link_fsmcontexts(ctx_nodes)))
}

fn build_substitute(
    opts: &FomaOptions,
    ps: &mut ParseState,
    haystack: &XreExpr,
    what: &SubstituteWhat,
    nets: Option<&mut DefinedNetworks>,
    funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    let net = build_net(opts, ps, haystack, nets, funcs)?;
    match what {
        /* sub1 sub2: fsm_substitute_symbol(net, subval1, subval2) — exactly one
        symbol to one symbol. */
        SubstituteWhat::Symbol {
            needle,
            replacement,
        } => {
            if replacement.len() != 1 {
                tracing::error!(
                    "Syntax error: substitution replaces a symbol with exactly one symbol"
                );
                fsm_destroy(net);
                return None;
            }
            Some(fsm_substitute_symbol(net, needle, &replacement[0]))
        }
        SubstituteWhat::Pair { .. } => {
            tracing::error!("Syntax error: pair substitution (a:b) is not supported");
            fsm_destroy(net);
            None
        }
    }
}

fn build_mapping(
    opts: &FomaOptions,
    ps: &mut ParseState,
    m: &MappingPair,
    arrow_type: i32,
    out: &mut Vec<Fsmrules>,
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<()> {
    match &m.upper {
        MappingSide::Expr(e) => {
            let upper = build_net(
                opts,
                ps,
                &e.value,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?;
            let (r, r2) = build_rhs(opts, ps, &m.kind, nets.as_deref_mut(), funcs.as_deref_mut())?;
            add_rule(opts, out, upper, r, r2, arrow_type);
        }
        MappingSide::Dotted(Some(e)) => {
            /* LDOT n0 RDOT ARROW ...: add_rule with ARROW_DOTTED. */
            let upper = build_net(
                opts,
                ps,
                &e.value,
                nets.as_deref_mut(),
                funcs.as_deref_mut(),
            )?;
            let (r, r2) = build_rhs(opts, ps, &m.kind, nets.as_deref_mut(), funcs.as_deref_mut())?;
            add_rule(opts, out, upper, r, r2, arrow_type | ARROW_DOTTED);
        }
        MappingSide::Dotted(None) => {
            /* LDOT RDOT ARROW n0: add_eprule with ARROW_DOTTED. */
            let (r, r2) = build_rhs(opts, ps, &m.kind, nets, funcs)?;
            add_eprule(out, r, r2, arrow_type | ARROW_DOTTED);
        }
    }
    Some(())
}

/// The right-hand side(s) of a mapping: (right, right2).
type RhsPair = (Option<Box<Fsm>>, Option<Box<Fsm>>);

fn build_rhs(
    opts: &FomaOptions,
    ps: &mut ParseState,
    kind: &MappingKind,
    mut nets: Option<&mut DefinedNetworks>,
    mut funcs: Option<&mut DefinedFunctions>,
) -> Option<RhsPair> {
    match kind {
        MappingKind::Plain { lower } => {
            let r = build_side(opts, ps, lower, nets.as_deref_mut(), funcs.as_deref_mut())?;
            Some((Some(r), None))
        }
        MappingKind::Markup { pre, post } => {
            /* n0 ARROW [n0] TRIPLE_DOT [n0]: right = pre|0, right2 = post|0. */
            let r = match pre {
                Some(s) => build_side(opts, ps, s, nets.as_deref_mut(), funcs.as_deref_mut())?,
                None => fsm_empty_string(),
            };
            let r2 = match post {
                Some(s) => build_side(opts, ps, s, nets, funcs)?,
                None => fsm_empty_string(),
            };
            Some((Some(r), Some(r2)))
        }
    }
}

fn build_side(
    opts: &FomaOptions,
    ps: &mut ParseState,
    s: &MappingSide,
    nets: Option<&mut DefinedNetworks>,
    funcs: Option<&mut DefinedFunctions>,
) -> Option<Box<Fsm>> {
    match s {
        MappingSide::Expr(e) => build_net(opts, ps, &e.value, nets, funcs),
        MappingSide::Dotted(Some(e)) => build_net(opts, ps, &e.value, nets, funcs),
        MappingSide::Dotted(None) => Some(fsm_empty_string()),
    }
}

/// regex.y add_rule: build the Fsmrules node(s) for one mapping. For dotted
/// rules the main rule has ARROW_DOTTED stripped (and its LHS loses the empty
/// string); an extra rule keeping ARROW_DOTTED is emitted only when the LHS
/// could match the empty string.
fn add_rule(
    opts: &FomaOptions,
    out: &mut Vec<Fsmrules>,
    l: Box<Fsm>,
    r: Option<Box<Fsm>>,
    r2: Option<Box<Fsm>>,
    ty: i32,
) {
    if (ty & ARROW_DOTTED) == 0 {
        out.push(Fsmrules {
            left: Some(l),
            right: r,
            right2: r2,
            cross_product: None,
            next: None,
            arrow_type: ty,
            dotted: 0,
        });
        return;
    }

    let mut l = l;
    let main_left = fsm_minus(opts, fsm_copy(&mut l), fsm_empty_string());
    let mut main = Fsmrules {
        left: Some(main_left),
        right: r,
        right2: r2,
        cross_product: None,
        next: None,
        arrow_type: ty - ARROW_DOTTED,
        dotted: 0,
    };

    /* test = L ∩ [] : add the empty-[..] rule only if non-empty. */
    let mut test = fsm_intersect(opts, l, fsm_empty_string());
    if !fsm_isempty(opts, &mut test) {
        let test_right = main.right.as_deref_mut().map(fsm_copy);
        let test_right2 = main.right2.as_deref_mut().map(fsm_copy);
        out.push(Fsmrules {
            left: Some(test),
            right: test_right,
            right2: test_right2,
            cross_product: None,
            next: None,
            arrow_type: ty,
            dotted: 0,
        });
    } else {
        fsm_destroy(test);
    }
    out.push(main);
}

/// regex.y add_eprule: `[..] -> R (... R2)` — LHS is the empty string, and the
/// arrow_type keeps ARROW_DOTTED (unlike add_rule's main rule).
fn add_eprule(out: &mut Vec<Fsmrules>, r: Option<Box<Fsm>>, r2: Option<Box<Fsm>>, ty: i32) {
    out.push(Fsmrules {
        left: Some(fsm_empty_string()),
        right: r,
        right2: r2,
        cross_product: None,
        next: None,
        arrow_type: ty,
        dotted: 0,
    });
}

fn link_fsmrules(mut nodes: Vec<Fsmrules>) -> Option<Box<Fsmrules>> {
    let mut head: Option<Box<Fsmrules>> = None;
    while let Some(mut node) = nodes.pop() {
        node.next = head;
        head = Some(Box::new(node));
    }
    head
}

fn link_fsmcontexts(mut nodes: Vec<Fsmcontexts>) -> Option<Box<Fsmcontexts>> {
    let mut head: Option<Box<Fsmcontexts>> = None;
    while let Some(mut node) = nodes.pop() {
        node.next = head;
        head = Some(Box::new(node));
    }
    head
}

fn link_rewritesets(mut nodes: Vec<RewriteSet>) -> Option<Box<RewriteSet>> {
    let mut head: Option<Box<RewriteSet>> = None;
    while let Some(mut node) = nodes.pop() {
        node.next = head;
        head = Some(Box::new(node));
    }
    head
}

#[cfg(test)]
mod tests {
    use crate::constructions::{fsm_count, fsm_equivalent};
    use crate::define::{
        Defined, add_defined, add_defined_function, defined_functions_init, defined_networks_init,
    };
    use crate::options::FomaOptions;
    use crate::topsort::fsm_topsort;
    use crate::types::Fsm;

    /// C foma's `print size` numbers are produced downstream of
    /// fsm_parse_regex: the CLI regex command runs fsm_topsort (which sets
    /// pathcount) and stack_add runs fsm_count — mirror that pipeline here.
    fn counted(net: Box<Fsm>) -> (i32, i32, i64) {
        let mut net = fsm_topsort(net);
        fsm_count(&mut net);
        (net.statecount, net.arccount, net.pathcount)
    }

    /// The internal regexes that rewrite.rs feeds to fsm_parse_regex and then
    /// `.unwrap()`s. If nfst-xre cannot parse any of these, the rewrite
    /// compiler would panic at runtime — so guard the parse here.
    const REWRITE_INTERNAL_REGEXES: &[&str] = &[
        r#""@O@" "@0@" "@#@" "@ID@""#,
        r#"[?:0]^4 [?:0 ?:0 ? ?]* [?:0]^4"#,
        r#"["@I[@"|"@I[]@"] ["@I[@"|"@I[]@"|"@I]@"|"@I@"|"@O@"]* ["@O@"|"@I[@"|"@I[]@"] ["@I[@"|"@I[]@"|"@I]@"|"@I@"|"@O@"]*"#,
        r#"[? ? ? ?]* [? ? [?-"@0@"] ?]"#,
        r#"[? ? ? ?]* [? ? ? [?-"@0@"]]"#,
        r#"["@I[@"] \["@I]@"]*"#,
        r#""@O@" ["@O@"]* ["@I[@"|"@I[]@"] ["@I[@"|"@I[]@"|"@I]@"|"@I@"|"@O@"]*"#,
        r#"~[[? ?]* "@0@" "@0@" [? ?]*]"#,
        r#"[? ? | "@UNK@" "@UNK@":"@ID@" ]*"#,
        r#"["@I[]@" ? ? ? | "@I[@" ? ? ? ["@I@" ? ? ?]* "@I]@" ? [?-"@0@"] ? ] ["@I]@" ? "@0@" ?]* | 0"#,
        r#"~[[? ? "@0@" ?]*]"#,
    ];

    #[test]
    fn rewrite_internal_regexes_parse() {
        for src in REWRITE_INTERNAL_REGEXES {
            let r = nfst_xre::parse_all(src);
            assert!(
                r.is_ok(),
                "nfst-xre failed to parse {:?}: {:?}",
                src,
                r.err()
            );
        }
    }

    #[test]
    fn end_to_end_compiles() {
        let opts = &FomaOptions::default();
        /* Exercise the full walk + construction pipeline (no defined tables). */
        let cases = [
            "cat",
            "c a t",
            "a | b",
            "a & b",
            "a - b",
            "a*",
            "a+",
            "a:b",
            "[a b]*",
            "a .o. b",
            "~a",
            "$a",
            "a^3",
            "a^{2,4}",
            "\\a",
            "a .x. b",
            ".#. a .#.",
            "{cat}",
            "a b c ;",
        ];
        for src in cases {
            let net = super::fsm_parse_regex(opts, src, None, None);
            assert!(net.is_some(), "failed to compile regex: {:?}", src);
        }
    }

    #[test]
    fn replace_and_restriction_compile() {
        let opts = &FomaOptions::default();
        /* The rewrite batch API and context-restriction paths. */
        assert!(super::fsm_parse_regex(opts, "a -> b", None, None).is_some());
        assert!(super::fsm_parse_regex(opts, "a -> b || c _ d", None, None).is_some());
        assert!(super::fsm_parse_regex(opts, "a -> b, c -> d", None, None).is_some());
        assert!(super::fsm_parse_regex(opts, "[. a .] -> b", None, None).is_some());
        assert!(super::fsm_parse_regex(opts, "a => b _ c", None, None).is_some());
        assert!(super::fsm_parse_regex(opts, "a @-> b || _ c", None, None).is_some());
    }

    // [spec:foma:sem:foma.my-yyparse-fn/test]
    // [spec:foma:sem:fomalib.fsm-parse-regex-fn/test]
    #[test]
    fn syntax_error_returns_none() {
        let opts = &FomaOptions::default();
        /* C: yyparse returns non-zero, my_yyparse propagates it, and
        fsm_parse_regex returns NULL. */
        assert!(super::fsm_parse_regex(opts, "[ a b", None, None).is_none());
        assert!(super::fsm_parse_regex(opts, "a |", None, None).is_none());
    }

    // fsm_parse_regex returns fsm_minimize(current_parse): the shapes below
    // are C foma's `print size` for the same regexes (2 states/1 arc/1 path;
    // 2/2/2; 3/3/2 — the last only after minimization).
    // [spec:foma:sem:fomalib.fsm-parse-regex-fn/test]
    #[test]
    fn parse_success_yields_minimized_c_shapes() {
        let opts = &FomaOptions::default();
        let expect = [
            ("a", 2, 1, 1i64),
            ("a|b", 2, 2, 2),
            ("[a b]|[a c]", 3, 3, 2),
        ];
        for (src, states, arcs, paths) in expect {
            let net = super::fsm_parse_regex(opts, src, None, None).unwrap();
            assert_eq!(counted(net), (states, arcs, paths), "shape of {:?}", src);
        }
    }

    // Grammar start rule `start: regex | regex start`: current_parse is the
    // LAST `;`-terminated network in the string.
    // [spec:foma:sem:foma.my-yyparse-fn/test]
    // [spec:foma:sem:fomalib.fsm-parse-regex-fn/test]
    #[test]
    fn semicolon_separated_regexes_keep_last() {
        let opts = &FomaOptions::default();
        let net = super::fsm_parse_regex(opts, "a b ; x y z ;", None, None).unwrap();
        let expected = super::fsm_parse_regex(opts, "x y z", None, None).unwrap();
        assert!(fsm_equivalent(opts, net, expected));
        let net = super::fsm_parse_regex(opts, "a ; b", None, None).unwrap();
        let expected = super::fsm_parse_regex(opts, "b", None, None).unwrap();
        assert!(fsm_equivalent(opts, net, expected));
    }

    // regex.l NONRESERVED: a symbol naming a defined net is substituted with
    // a copy of that net (via the defined_nets table); without the table the
    // same name is a literal single symbol.
    // [spec:foma:sem:foma.my-yyparse-fn/test]
    // [spec:foma:sem:fomalib.fsm-parse-regex-fn/test]
    #[test]
    fn defined_net_names_substitute_from_the_table() {
        let opts = &FomaOptions::default();
        let mut nets = defined_networks_init();
        let def = super::fsm_parse_regex(opts, "x y", None, None).unwrap();
        assert_eq!(add_defined(&mut nets, Some(def), "Foo"), Defined::New);
        // With the table: "Foo" compiles to the defined net [x y]
        // (C foma: 3 states, 2 arcs, 1 path).
        let net = super::fsm_parse_regex(opts, "Foo", Some(&mut nets), None).unwrap();
        assert_eq!(counted(net), (3, 2, 1));
        let net = super::fsm_parse_regex(opts, "Foo", Some(&mut nets), None).unwrap();
        let expected = super::fsm_parse_regex(opts, "x y", None, None).unwrap();
        assert!(fsm_equivalent(opts, net, expected));
        // Without the table: "Foo" is one literal (multichar) symbol.
        let net = super::fsm_parse_regex(opts, "Foo", None, None).unwrap();
        assert_eq!(counted(net), (2, 1, 1));
    }

    // regex.y function_apply: F(a) expands the stored body regex with the
    // argument bound to a temporary defined symbol and reparses it.
    // [spec:foma:sem:foma.my-yyparse-fn/test]
    // [spec:foma:sem:fomalib.fsm-parse-regex-fn/test]
    #[test]
    fn defined_function_application_expands_body() {
        let opts = &FomaOptions::default();
        let mut nets = defined_networks_init();
        let mut funcs = defined_functions_init();
        // define F(X) [X X];  — stored body references @ARGUMENT01@.
        add_defined_function(opts, &mut funcs, "F", "[@ARGUMENT01@ @ARGUMENT01@]", 1);
        let net = super::fsm_parse_regex(opts, "F(a)", Some(&mut nets), Some(&mut funcs)).unwrap();
        // C foma: regex F(a); => 3 states, 2 arcs, 1 path (== [a a]).
        assert_eq!(counted(net), (3, 2, 1));
        let net = super::fsm_parse_regex(opts, "F(a)", Some(&mut nets), Some(&mut funcs)).unwrap();
        let expected = super::fsm_parse_regex(opts, "a a", None, None).unwrap();
        assert!(fsm_equivalent(opts, net, expected));
        // Undefined functions fail.
        assert!(super::fsm_parse_regex(opts, "G(a)", Some(&mut nets), Some(&mut funcs)).is_none());
    }

    // my_yyparse's g_parse_depth guard: at MAX_PARSE_DEPTH (100) nested
    // reparses it prints "Exceeded parser stack depth.  Self-recursive call?"
    // and fails instead of recursing forever.
    // [spec:foma:sem:foma.my-yyparse-fn/test]
    #[test]
    fn parse_depth_guard_stops_self_recursive_defines() {
        let opts = &FomaOptions::default();
        let mut nets = defined_networks_init();
        let mut funcs = defined_functions_init();
        // define F(X) F(X); — every application reparses another F(...) call.
        add_defined_function(opts, &mut funcs, "F", "F(@ARGUMENT01@)", 1);
        assert!(super::fsm_parse_regex(opts, "F(a)", Some(&mut nets), Some(&mut funcs)).is_none());
        // Each top-level parse gets a fresh depth counter, so the parser still
        // works after the guard trips.
        assert!(super::fsm_parse_regex(opts, "a", None, None).is_some());
    }
}
