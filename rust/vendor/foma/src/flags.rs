//! foma/flags.c — literal Wave-2 (bug-for-bug) port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/flags.md.

use crate::constructions::{
    add_fsm_arc, fsm_complement, fsm_compose, fsm_concat, fsm_contains, fsm_intersect,
    fsm_optionality, fsm_symbol, fsm_union, fsm_universal,
};
use crate::minimize::fsm_minimize;
use crate::options::FomaOptions;
use crate::sigma::{sigma_cleanup, sigma_max, sigma_remove_num, sigma_sort};
use crate::structures::{fsm_copy, fsm_empty_set};
use crate::topsort::fsm_topsort;
use crate::types::{EPSILON, Fsm, FsmState, NO, UNK};
use crate::types::{
    FLAG_CLEAR, FLAG_DISALLOW, FLAG_EQUAL, FLAG_NEGATIVE, FLAG_POSITIVE, FLAG_REQUIRE, FLAG_UNIFY,
};
use smol_str::SmolStr;

/* File-local constants (C #defines; distinct from apply.c's FAIL/SUCCEED) */
const FAIL: i32 = 1;
const SUCCEED: i32 = 2;
const NONE: i32 = 3;

// [spec:foma:def:flags.flags]
pub struct Flags {
    pub r#type: i32,
    pub name: Option<SmolStr>,
    pub value: Option<SmolStr>,
    pub next: Option<Box<Flags>>,
}

/* We eliminate all flags by creating a list of them and building a regex filter   */
/* that successively removes unwanted paths.  NB: flag_eliminate() called with the */
/* second argument NULL eliminates all flags.                                      */
/* The regexes we build for each flag symbol are of the format:                    */
/* ~[?* FAIL ~$SUCCEED THISFLAG ?*] for U,P,D                                      */
/* or                                                                              */
/* ~[(?* FAIL) ~$SUCCEED THISFLAG ?*] for the R flag                               */
/* The function flag_build() determines, depending on the flag at hand for each    */
/* of the other flags occurring in the network if it belongs in FAIL, SUCCEED,     */
/* or neither.                                                                     */
/* The languages FAIL, SUCCEED is then the union of all symbols that cause         */
/* compatibility or incompatibility.                                               */
/* We intersect all these filters, creating a large filter that we compose both on */
/* the upper side of the network and the lower side:                               */
/* RESULT = FILTER .o. ORIGINAL .o. FILTER                                         */
/* We can't simply intersect the language with FILTER because the lower side flags */
/* are independent of the upper side ones, and the network may be a transducer.    */
/* Finally, we replace the affected arcs with EPSILON arcs, and call               */
/* sigma_cleanup() to purge the symbols not occurring on arcs.                     */

///
///Eliminate a flag from a network. If called with name = NULL, eliminate all flags.
///
// [spec:foma:def:flags.flag-eliminate-fn]
// [spec:foma:sem:flags.flag-eliminate-fn+1]
// [spec:foma:def:fomalib.flag-eliminate-fn]
// [spec:foma:sem:fomalib.flag-eliminate-fn+1]
pub fn flag_eliminate(opts: &FomaOptions, net: Box<Fsm>, name: Option<&str>) -> Box<Fsm> {
    let mut filter: Option<Box<Fsm>> = None;

    if net.pathcount == 0 {
        if opts.verbose {
            tracing::warn!("Skipping flag elimination since there are no paths in network.");
        }
        return net;
    }

    let flags = flag_extract(&net);
    /* Check that flag actually exists in net */
    if let Some(name) = name {
        let mut found = 0;
        let mut f = flags.as_deref();
        while let Some(fl) = f {
            /* strcmp(name, f->name) — C would segfault on a NULL f->name */
            if fl.name.as_deref() == Some(name) {
                found = 1;
            }
            f = fl.next.as_deref();
        }
        if found == 0 {
            if opts.verbose {
                tracing::warn!("Flag attribute '{}' does not occur in the network.", name);
            }
            return net;
        }
    }

    let mut flag = 0;

    let mut f = flags.as_deref();
    while let Some(fl) = f {
        let mut succeed_flags: Option<Box<Fsm>> = None;
        let mut fail_flags: Option<Box<Fsm>> = None;
        let mut self_: Option<Box<Fsm>> = None;

        /* Wave 4 fix: the C ORed the type mask (`f->type | U|R|D|E`), which is
        always nonzero, so the intended restriction to U/R/D/E flags was a
        no-op and the body ran for every type. Use `&` to actually restrict it.
        Observable language is unchanged: flag_build classifies pairs only when
        f's type is U, R, or D, so the P/N/C/E iterations the bug allowed never
        built a filter anyway. */
        if (name.is_none() || fl.name.as_deref() == name)
            && (fl.r#type & (FLAG_UNIFY | FLAG_REQUIRE | FLAG_DISALLOW | FLAG_EQUAL)) != 0
        {
            succeed_flags = Some(fsm_empty_set());
            fail_flags = Some(fsm_empty_set());
            self_ = Some(flag_create_symbol(
                fl.r#type,
                fl.name.as_deref().expect("flag list node has a name"),
                fl.value.as_deref(),
            ));

            let mut ff = flags.as_deref();
            flag = 0;
            while let Some(ffl) = ff {
                let fstatus = flag_build(
                    fl.r#type,
                    fl.name.as_deref().expect("flag list node has a name"),
                    fl.value.as_deref(),
                    ffl.r#type,
                    ffl.name.as_deref().expect("flag list node has a name"),
                    ffl.value.as_deref(),
                );
                if fstatus == FAIL {
                    fail_flags = Some(fsm_minimize(
                        opts,
                        fsm_union(
                            opts,
                            fail_flags
                                .take()
                                .expect("fail_flags populated when flag was set"),
                            flag_create_symbol(
                                ffl.r#type,
                                ffl.name.as_deref().expect("flag list node has a name"),
                                ffl.value.as_deref(),
                            ),
                        ),
                    ));
                    flag = 1;
                }
                if fstatus == SUCCEED {
                    succeed_flags = Some(fsm_minimize(
                        opts,
                        fsm_union(
                            opts,
                            succeed_flags
                                .take()
                                .expect("succeed_flags populated when flag was set"),
                            flag_create_symbol(
                                ffl.r#type,
                                ffl.name.as_deref().expect("flag list node has a name"),
                                ffl.value.as_deref(),
                            ),
                        ),
                    ));
                    flag = 1;
                }
                ff = ffl.next.as_deref();
            }
        }

        if flag != 0 {
            let newfilter = if fl.r#type == FLAG_REQUIRE {
                fsm_complement(
                    opts,
                    fsm_concat(
                        opts,
                        fsm_optionality(
                            opts,
                            fsm_concat(
                                opts,
                                fsm_universal(),
                                fail_flags
                                    .take()
                                    .expect("fail_flags populated when flag was set"),
                            ),
                        ),
                        fsm_concat(
                            opts,
                            fsm_complement(
                                opts,
                                fsm_contains(
                                    opts,
                                    succeed_flags
                                        .take()
                                        .expect("succeed_flags populated when flag was set"),
                                ),
                            ),
                            fsm_concat(
                                opts,
                                self_.take().expect("self_ populated when flag was set"),
                                fsm_universal(),
                            ),
                        ),
                    ),
                )
            } else {
                fsm_complement(
                    opts,
                    fsm_contains(
                        opts,
                        fsm_concat(
                            opts,
                            fail_flags
                                .take()
                                .expect("fail_flags populated when flag was set"),
                            fsm_concat(
                                opts,
                                fsm_complement(
                                    opts,
                                    fsm_contains(
                                        opts,
                                        succeed_flags
                                            .take()
                                            .expect("succeed_flags populated when flag was set"),
                                    ),
                                ),
                                self_.take().expect("self_ populated when flag was set"),
                            ),
                        ),
                    ),
                )
            };

            filter = Some(match filter.take() {
                None => newfilter,
                Some(filter) => fsm_intersect(opts, filter, newfilter),
            });
        }
        flag = 0;
        f = fl.next.as_deref();
    }

    let newnet = if let Some(mut filter) = filter {
        /* C saved g_flag_is_epsilon, forced it to 0 around the two composes,
        and restored it; here the composes get an option copy with it off. */
        let compose_opts = FomaOptions {
            flag_is_epsilon: false,
            ..opts.clone()
        };
        let newnet = fsm_compose(
            &compose_opts,
            fsm_copy(&mut filter),
            fsm_compose(&compose_opts, net, fsm_copy(&mut filter)),
        );
        /* the filter itself is never destroyed in C (leak); dropped here */
        newnet
    } else {
        net
    };

    let mut newnet = newnet;
    flag_purge(&mut newnet, name);
    let mut newnet = fsm_minimize(opts, newnet);
    sigma_cleanup(&mut newnet, 0);
    sigma_sort(&mut newnet);
    /* free(flags) — C frees only the list head (remaining nodes and their
    name/value strings leak); Rust drops the whole list */
    drop(flags);
    fsm_topsort(newnet)
}

// [spec:foma:def:flags.flag-create-symbol-fn]
// [spec:foma:sem:flags.flag-create-symbol-fn]
pub(crate) fn flag_create_symbol(r#type: i32, name: &str, value: Option<&str>) -> Box<Fsm> {
    let value = value.unwrap_or_default();

    /* C: string = malloc(strlen(name)+strlen(value)+6), built with strcat and
    never freed (leak); an owned String here */
    let mut string = String::new();
    string.push('@');
    /* flag_type_to_char(type) — C would segfault on NULL for unknown types */
    string.push_str(flag_type_to_char(r#type).expect("known flag type has a char"));
    string.push('.');
    string.push_str(name);
    if !value.is_empty() {
        string.push('.');
        string.push_str(value);
    }
    string.push('@');

    fsm_symbol(&string)
}

// [spec:foma:def:flags.flag-type-to-char-fn]
// [spec:foma:sem:flags.flag-type-to-char-fn]
pub(crate) fn flag_type_to_char(r#type: i32) -> Option<&'static str> {
    match r#type {
        FLAG_UNIFY => return Some("U"),
        FLAG_CLEAR => return Some("C"),
        FLAG_DISALLOW => return Some("D"),
        FLAG_NEGATIVE => return Some("N"),
        FLAG_POSITIVE => return Some("P"),
        FLAG_REQUIRE => return Some("R"),
        FLAG_EQUAL => return Some("E"),
        _ => {}
    }
    None
}

// [spec:foma:def:flags.flag-build-fn]
// [spec:foma:sem:flags.flag-build-fn]
// [spec:foma:def:fomalib.flag-build-fn]
// [spec:foma:sem:fomalib.flag-build-fn]
pub fn flag_build(
    ftype: i32,
    fname: &str,
    fvalue: Option<&str>,
    fftype: i32,
    ffname: &str,
    ffvalue: Option<&str>,
) -> i32 {
    if fname != ffname {
        return NONE;
    }
    /* selfnull: the eliminated flag is valueless, e.g. @R.A@ or @D.A@ */
    let selfnull = fvalue.is_none();
    let fvalue = fvalue.unwrap_or("");
    let ffvalue = ffvalue.unwrap_or("");
    /* eq mirrors the C's `strcmp(fvalue, ffvalue) == 0` */
    let eq = fvalue == ffvalue;

    /* Pairwise compatibility decision table (see the sem rule); first matching
    row wins, anything unlisted is NONE. Columns: eliminated flag type, other
    flag type, required `eq` (None = don't care), required `selfnull`, result. */
    type Row = (i32, i32, Option<bool>, Option<bool>, i32);
    #[rustfmt::skip]
    let rows: [Row; 25] = [
        /* U flags */
        (FLAG_UNIFY,    FLAG_POSITIVE, Some(true),  None,        SUCCEED),
        (FLAG_UNIFY,    FLAG_CLEAR,    None,        None,        SUCCEED),
        (FLAG_UNIFY,    FLAG_UNIFY,    Some(false), None,        FAIL),
        (FLAG_UNIFY,    FLAG_POSITIVE, Some(false), None,        FAIL),
        (FLAG_UNIFY,    FLAG_NEGATIVE, Some(true),  None,        FAIL),
        /* R flag, valueless */
        (FLAG_REQUIRE,  FLAG_UNIFY,    None,        Some(true),  SUCCEED),
        (FLAG_REQUIRE,  FLAG_POSITIVE, None,        Some(true),  SUCCEED),
        (FLAG_REQUIRE,  FLAG_NEGATIVE, None,        Some(true),  SUCCEED),
        (FLAG_REQUIRE,  FLAG_CLEAR,    None,        Some(true),  FAIL),
        /* R flag, with value */
        (FLAG_REQUIRE,  FLAG_POSITIVE, Some(true),  Some(false), SUCCEED),
        (FLAG_REQUIRE,  FLAG_UNIFY,    Some(true),  Some(false), SUCCEED),
        (FLAG_REQUIRE,  FLAG_POSITIVE, Some(false), Some(false), FAIL),
        (FLAG_REQUIRE,  FLAG_UNIFY,    Some(false), Some(false), FAIL),
        (FLAG_REQUIRE,  FLAG_NEGATIVE, None,        Some(false), FAIL),
        (FLAG_REQUIRE,  FLAG_CLEAR,    None,        Some(false), FAIL),
        /* D flag, valueless */
        (FLAG_DISALLOW, FLAG_CLEAR,    None,        Some(true),  SUCCEED),
        (FLAG_DISALLOW, FLAG_POSITIVE, None,        Some(true),  FAIL),
        (FLAG_DISALLOW, FLAG_UNIFY,    None,        Some(true),  FAIL),
        (FLAG_DISALLOW, FLAG_NEGATIVE, None,        Some(true),  FAIL),
        /* D flag, with value */
        (FLAG_DISALLOW, FLAG_POSITIVE, Some(false), Some(false), SUCCEED),
        (FLAG_DISALLOW, FLAG_CLEAR,    None,        Some(false), SUCCEED),
        (FLAG_DISALLOW, FLAG_NEGATIVE, Some(true),  Some(false), SUCCEED),
        (FLAG_DISALLOW, FLAG_POSITIVE, Some(true),  Some(false), FAIL),
        (FLAG_DISALLOW, FLAG_UNIFY,    Some(true),  Some(false), FAIL),
        (FLAG_DISALLOW, FLAG_NEGATIVE, Some(false), Some(false), FAIL),
    ];
    for &(ft, fft, eq_req, null_req, result) in &rows {
        if ftype == ft
            && fftype == fft
            && eq_req.is_none_or(|r| r == eq)
            && null_req.is_none_or(|r| r == selfnull)
        {
            return result;
        }
    }
    NONE
}

/* Remove flags that are being eliminated from arcs and sigma */

// [spec:foma:def:flags.flag-purge-fn]
// [spec:foma:sem:flags.flag-purge-fn]
pub(crate) fn flag_purge(net: &mut Fsm, name: Option<&str>) {
    let sigmasize = sigma_max(&net.sigma) + 1;
    /* C: malloc'd int array, zeroed by the following loop */
    let mut ftable: Vec<i32> = vec![0; sigmasize as usize];

    for s in &net.sigma {
        let symbol = s.symbol.as_str();
        if flag_check(symbol) {
            match name {
                None => {
                    ftable[s.number as usize] = 1;
                }
                Some(name) => {
                    /* csym = (sigma->symbol) + 3 — skip "@X." (one-byte operator) */
                    let csym = &symbol.as_bytes()[3..];
                    let name_b = name.as_bytes();
                    /* strncmp(csym,name,strlen(name)) == 0 && strlen(csym) > strlen(name)
                    && (csym[strlen(name)] == '.' || csym[strlen(name)] == '@') */
                    if csym.starts_with(name_b)
                        && csym.len() > name_b.len()
                        && (csym[name_b.len()] == b'.' || csym[name_b.len()] == b'@')
                    {
                        ftable[s.number as usize] = 1;
                    }
                }
            }
        }
    }
    for i in 0..sigmasize {
        if ftable[i as usize] != 0 {
            sigma_remove_num(i, &mut net.sigma);
        }
    }

    let fsm = &mut net.states;
    let mut i = 0usize;
    while fsm[i].state_no != -1 {
        if fsm[i].r#in >= 0 && fsm[i].out >= 0 {
            if ftable[fsm[i].r#in as usize] != 0 {
                fsm[i].r#in = EPSILON as i16;
            }
            if ftable[fsm[i].out as usize] != 0 {
                fsm[i].out = EPSILON as i16;
            }
        }
        i += 1;
    }

    /* free(ftable) — drop */
    net.is_deterministic = NO;
    net.is_minimized = NO;
    net.is_epsilon_free = NO;
}

/* Extract all flags from network and place them in struct flag linked list */

// [spec:foma:def:flags.flag-extract-fn]
// [spec:foma:sem:flags.flag-extract-fn]
pub(crate) fn flag_extract(net: &Fsm) -> Option<Box<Flags>> {
    let mut flags: Option<Box<Flags>> = None;
    for s in &net.sigma {
        let symbol = s.symbol.as_str();
        if flag_check(symbol) {
            let flagst = Box::new(Flags {
                r#type: flag_get_type(symbol),
                name: flag_get_name(symbol),
                value: flag_get_value(symbol),
                next: flags,
            });
            flags = Some(flagst);
        }
    }
    flags
}

// [spec:foma:def:flags.flag-check-fn]
// [spec:foma:sem:flags.flag-check-fn]
// [spec:foma:def:fomalibconf.flag-check-fn]
// [spec:foma:sem:fomalibconf.flag-check-fn]
pub fn flag_check(s: &str) -> bool {
    /* Byte-level DFA for the flag-diacritic grammar (ND = any byte that is
    neither '.' nor NUL): return true iff s matches
        "@" [U|P|N|E] "." ND+ "." ND+ "@"   (U/P/N/E: attribute AND value)
      | "@" [R|D]     "." ND+ ["." ND+] "@" (R/D: value optional)
      | "@" C         "." ND+ "@"           (C: never a value)
    The C used numeric goto labels s0..s11; here each is a `State` variant with
    the same transitions. The end of the &str stands in for the C NUL. */
    #[derive(Clone, Copy)]
    enum State {
        Start,
        S1,
        S2,
        S3,
        S4,
        S5,
        S6,
        S7,
        S8,
        S9,
        S10,
        S11,
    }
    use State::*;

    let s = s.as_bytes();
    let at = |i: usize| -> u8 { if i < s.len() { s[i] } else { 0 } };

    let mut i = 0usize;
    let mut state = Start;
    loop {
        state = match state {
            Start => match at(i) {
                b'@' => {
                    i += 1;
                    S1
                }
                _ => return false,
            },
            S1 => match at(i) {
                b'C' => {
                    i += 1;
                    S4
                }
                b'N' | b'E' | b'U' | b'P' => {
                    i += 1;
                    S2
                }
                b'R' | b'D' => {
                    i += 1;
                    S3
                }
                _ => return false,
            },
            /* operator letter '.': into the first (mandatory) field */
            S2 if at(i) == b'.' => {
                i += 1;
                S5
            }
            S3 if at(i) == b'.' => {
                i += 1;
                S6
            }
            S4 if at(i) == b'.' => {
                i += 1;
                S7
            }
            S2 | S3 | S4 => return false,
            /* first ND byte of each field must exist */
            S5 => match at(i) {
                b'.' | 0 => return false,
                _ => {
                    i += 1;
                    S8
                }
            },
            S6 => match at(i) {
                b'.' | 0 => return false,
                _ => {
                    i += 1;
                    S9
                }
            },
            S7 => match at(i) {
                b'.' | 0 => return false,
                _ => {
                    i += 1;
                    S10
                }
            },
            /* S8: first field of U/P/N/E — quirk: '@' is an ordinary ND byte
            here, so e.g. "@U.a@b.c@" is accepted; only '.' ends the field */
            S8 => match at(i) {
                b'.' => {
                    i += 1;
                    S7
                }
                0 => return false,
                _ => {
                    i += 1;
                    S8
                }
            },
            /* S9: single field of R/D — '@' ends the string, '.' opens a value */
            S9 => match at(i) {
                b'@' => {
                    i += 1;
                    S11
                }
                b'.' => {
                    i += 1;
                    S7
                }
                0 => return false,
                _ => {
                    i += 1;
                    S9
                }
            },
            /* S10: last field — only '@' ends it */
            S10 => match at(i) {
                b'@' => {
                    i += 1;
                    S11
                }
                b'.' | 0 => return false,
                _ => {
                    i += 1;
                    S10
                }
            },
            /* S11: accept iff the '@' was the final byte */
            S11 => return at(i) == 0,
        };
    }
}

// [spec:foma:def:flags.flag-get-type-fn]
// [spec:foma:sem:flags.flag-get-type-fn]
// [spec:foma:def:fomalibconf.flag-get-type-fn]
// [spec:foma:sem:fomalibconf.flag-get-type-fn]
pub fn flag_get_type(string: &str) -> i32 {
    let b = string.as_bytes();
    /* strncmp(string+1, "X.", 2) == 0 — a string shorter than 3 bytes can't
    match (the C strncmp stops at the NUL and compares unequal) */
    let strncmp2 = |pat: &[u8; 2]| -> bool { b.len() >= 3 && b[1] == pat[0] && b[2] == pat[1] };
    if strncmp2(b"U.") {
        return FLAG_UNIFY;
    }
    if strncmp2(b"C.") {
        return FLAG_CLEAR;
    }
    if strncmp2(b"D.") {
        return FLAG_DISALLOW;
    }
    if strncmp2(b"N.") {
        return FLAG_NEGATIVE;
    }
    if strncmp2(b"P.") {
        return FLAG_POSITIVE;
    }
    if strncmp2(b"R.") {
        return FLAG_REQUIRE;
    }
    if strncmp2(b"E.") {
        return FLAG_EQUAL;
    }
    0
}

// [spec:foma:def:flags.flag-get-name-fn]
// [spec:foma:sem:flags.flag-get-name-fn]
// [spec:foma:def:fomalibconf.flag-get-name-fn]
// [spec:foma:sem:fomalibconf.flag-get-name-fn]
pub fn flag_get_name(string: &str) -> Option<SmolStr> {
    // A flag diacritic is `@X.name.value@` / `@X.name@`: the name is the run
    // between the first '.' and the next '.' or '@'. Those delimiters are ASCII,
    // so they never occur inside a multi-byte char — walk characters directly.
    let mut start: Option<usize> = None;
    for (i, c) in string.char_indices() {
        match c {
            '.' if start.is_none() => start = Some(i + 1),
            '.' | '@' if start.is_some() => return Some(string[start.unwrap()..i].into()),
            _ => {}
        }
    }
    None
}

// [spec:foma:def:flags.flag-get-value-fn]
// [spec:foma:sem:flags.flag-get-value-fn]
// [spec:foma:def:fomalibconf.flag-get-value-fn]
// [spec:foma:sem:fomalibconf.flag-get-value-fn]
pub fn flag_get_value(string: &str) -> Option<SmolStr> {
    // The value is the run between the SECOND '.' and the closing '@'
    // (`@X.name.value@`); a one-argument flag `@X.name@` has no second '.', so
    // `start` stays unset and the closing '@' yields nothing. ASCII delimiters,
    // so walk characters directly.
    let mut seen_first_dot = false;
    let mut start: Option<usize> = None;
    for (i, c) in string.char_indices() {
        match c {
            '.' if !seen_first_dot => seen_first_dot = true,
            '@' if start.is_some() => return Some(string[start.unwrap()..i].into()),
            '.' if seen_first_dot => start = Some(i + 1),
            _ => {}
        }
    }
    None
}

// [spec:foma:def:flags.flag-twosided-fn]
// [spec:foma:sem:flags.flag-twosided-fn]
// [spec:foma:def:fomalib.flag-twosided-fn]
// [spec:foma:sem:fomalib.flag-twosided-fn]
pub fn flag_twosided(opts: &FomaOptions, mut net: Box<Fsm>) -> Box<Fsm> {
    /* Enforces twosided flag diacritics */

    /* Mark flag symbols */
    let maxsigma = sigma_max(&net.sigma);
    /* C: calloc(maxsigma+1, sizeof(int)) */
    let mut isflag: Vec<i32> = vec![0; (maxsigma + 1) as usize];
    for s in &net.sigma {
        isflag[s.number as usize] = if flag_check(&s.symbol) { 1 } else { 0 };
    }
    let mut maxstate = 0;
    let mut change = 0;
    let mut i = 0usize;
    let mut newarcs = 0usize;
    while net.states[i].state_no != -1 {
        maxstate = if net.states[i].state_no > maxstate {
            net.states[i].state_no
        } else {
            maxstate
        };
        if net.states[i].target == -1 {
            i += 1;
            continue;
        }
        if isflag[net.states[i].r#in as usize] != 0 && net.states[i].out == EPSILON as i16 {
            change = 1;
            net.states[i].out = net.states[i].r#in;
        } else if isflag[net.states[i].out as usize] != 0 && net.states[i].r#in == EPSILON as i16 {
            change = 1;
            net.states[i].r#in = net.states[i].out;
        }
        if (isflag[net.states[i].r#in as usize] != 0 || isflag[net.states[i].out as usize] != 0)
            && net.states[i].r#in != net.states[i].out
        {
            newarcs += 1;
        }
        i += 1;
    }

    if newarcs == 0 {
        if change == 1 {
            net.is_deterministic = UNK;
            net.is_minimized = UNK;
            net.is_pruned = UNK;
            return fsm_topsort(fsm_minimize(opts, net));
        }
        return net;
    }
    /* Grow the line table to hold the i original lines, newarcs split arcs,
    and one new sentinel. */
    net.states.resize(
        i + newarcs + 1,
        FsmState {
            state_no: 0,
            r#in: 0,
            out: 0,
            target: 0,
            final_state: 0,
            start_state: 0,
        },
    );
    let tail = i;
    let mut j = i as i32;
    maxstate += 1;
    for i in 0..tail {
        if net.states[i].target == -1 {
            continue;
        }
        let in_i = net.states[i].r#in;
        let out_i = net.states[i].out;
        let target_i = net.states[i].target;
        if (isflag[in_i as usize] != 0 || isflag[out_i as usize] != 0) && in_i != out_i {
            if isflag[in_i as usize] != 0 && isflag[out_i as usize] == 0 {
                j = add_fsm_arc(
                    &mut net.states,
                    j,
                    maxstate,
                    EPSILON,
                    out_i as i32,
                    target_i,
                    0,
                    0,
                );
                net.states[i].out = net.states[i].r#in;
                net.states[i].target = maxstate;
                maxstate += 1;
            } else if isflag[out_i as usize] != 0 && isflag[in_i as usize] == 0 {
                j = add_fsm_arc(
                    &mut net.states,
                    j,
                    maxstate,
                    out_i as i32,
                    out_i as i32,
                    target_i,
                    0,
                    0,
                );
                net.states[i].out = EPSILON as i16;
                net.states[i].target = maxstate;
                maxstate += 1;
            } else if isflag[in_i as usize] != 0 && isflag[out_i as usize] != 0 {
                j = add_fsm_arc(
                    &mut net.states,
                    j,
                    maxstate,
                    out_i as i32,
                    out_i as i32,
                    target_i,
                    0,
                    0,
                );
                net.states[i].out = net.states[i].r#in;
                net.states[i].target = maxstate;
                maxstate += 1;
            }
        }
    }
    /* Add sentinel */
    add_fsm_arc(&mut net.states, j, -1, -1, -1, -1, -1, -1);
    net.is_deterministic = UNK;
    net.is_minimized = UNK;
    fsm_topsort(fsm_minimize(opts, net))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply_init, apply_words};
    use crate::regex::fsm_parse_regex;
    use crate::types::{
        FLAG_CLEAR, FLAG_DISALLOW, FLAG_EQUAL, FLAG_NEGATIVE, FLAG_POSITIVE, FLAG_REQUIRE,
        FLAG_UNIFY,
    };

    /* All symbols in a net's sigma (excluding the -1 sentinel), by symbol text. */
    fn sigma_syms(net: &Fsm) -> Vec<String> {
        let mut v: Vec<String> = net
            .sigma
            .iter()
            .map(|node| node.symbol.to_string())
            .collect();
        v.sort();
        v
    }

    /* Map a sigma number to its symbol text; EPSILON prints as "0". */
    fn num_to_sym(net: &Fsm, n: i16) -> String {
        if n as i32 == EPSILON {
            return "0".to_string();
        }
        for node in &net.sigma {
            if node.number == n as i32 {
                return node.symbol.to_string();
            }
        }
        format!("#{}", n)
    }

    /* Multiset of (in,out) arc labels as symbol text. */
    fn arc_labels(net: &Fsm) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = net
            .states
            .iter()
            .take_while(|l| l.state_no != -1)
            .filter(|l| l.target != -1)
            .map(|l| (num_to_sym(net, l.r#in), num_to_sym(net, l.out)))
            .collect();
        v.sort();
        v
    }

    /* Enumerate the whole (finite) language via apply_words. */
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

    // [spec:foma:sem:flags.flag-check-fn/test]
    // [spec:foma:sem:fomalibconf.flag-check-fn/test]
    #[test]
    fn flag_check_dfa() {
        /* U/P/N/E require both attribute and value */
        assert!(flag_check("@U.F.V@"));
        assert!(flag_check("@P.F.V@"));
        assert!(flag_check("@N.F.V@"));
        assert!(flag_check("@E.F.V@"));
        /* R/D value optional */
        assert!(flag_check("@R.F@"));
        assert!(flag_check("@D.F@"));
        assert!(flag_check("@R.F.V@"));
        assert!(flag_check("@D.F.V@"));
        /* C never takes a value */
        assert!(flag_check("@C.X@"));
        /* Quirk: '@' is an ordinary ND byte in the mandatory first field of
        U/P/N/E, so this is accepted despite the interior '@' */
        assert!(flag_check("@U.a@b.c@"));

        /* Rejections */
        assert!(!(flag_check("")));
        assert!(!(flag_check("a")));
        assert!(!(flag_check("@Z.F.V@")), "bad operator letter");
        assert!(!(flag_check("@U.F@")), "U needs a value field");
        assert!(!(flag_check("@C.X.Y@")), "C may not take a value");
        assert!(!(flag_check("@R.F.V.W@")), "at most two fields");
        assert!(!(flag_check("@U.F.V@x")), "must end right after '@'");
    }

    // [spec:foma:sem:flags.flag-get-type-fn/test]
    // [spec:foma:sem:fomalibconf.flag-get-type-fn/test]
    // [spec:foma:sem:flags.flag-get-name-fn/test]
    // [spec:foma:sem:fomalibconf.flag-get-name-fn/test]
    // [spec:foma:sem:flags.flag-get-value-fn/test]
    // [spec:foma:sem:fomalibconf.flag-get-value-fn/test]
    #[test]
    fn flag_field_extractors() {
        assert_eq!(flag_get_type("@U.F.V@"), FLAG_UNIFY);
        assert_eq!(flag_get_type("@C.X@"), FLAG_CLEAR);
        assert_eq!(flag_get_type("@D.F@"), FLAG_DISALLOW);
        assert_eq!(flag_get_type("@N.F.V@"), FLAG_NEGATIVE);
        assert_eq!(flag_get_type("@P.F.V@"), FLAG_POSITIVE);
        assert_eq!(flag_get_type("@R.A@"), FLAG_REQUIRE);
        assert_eq!(flag_get_type("@E.F.V@"), FLAG_EQUAL);
        assert_eq!(flag_get_type("@Z.x@"), 0);

        assert_eq!(flag_get_name("@U.FEAT.VAL@").as_deref(), Some("FEAT"));
        assert_eq!(flag_get_name("@R.A@").as_deref(), Some("A"));

        assert_eq!(flag_get_value("@U.FEAT.VAL@").as_deref(), Some("VAL"));
        /* valueless flags yield NULL */
        assert_eq!(flag_get_value("@R.A@"), None);
        assert_eq!(flag_get_value("@C.X@"), None);
        assert_eq!(flag_get_value("@D.F@"), None);
    }

    // [spec:foma:sem:flags.flag-type-to-char-fn/test]
    // [spec:foma:sem:flags.flag-create-symbol-fn/test]
    #[test]
    fn flag_create_symbol_builds_symbol() {
        assert_eq!(flag_type_to_char(FLAG_UNIFY), Some("U"));
        assert_eq!(flag_type_to_char(FLAG_CLEAR), Some("C"));
        assert_eq!(flag_type_to_char(FLAG_DISALLOW), Some("D"));
        assert_eq!(flag_type_to_char(FLAG_NEGATIVE), Some("N"));
        assert_eq!(flag_type_to_char(FLAG_POSITIVE), Some("P"));
        assert_eq!(flag_type_to_char(FLAG_REQUIRE), Some("R"));
        assert_eq!(flag_type_to_char(FLAG_EQUAL), Some("E"));
        assert_eq!(flag_type_to_char(999), None);

        /* "@" + type + "." + name + "." + value + "@" */
        let n = flag_create_symbol(FLAG_UNIFY, "F", Some("V"));
        assert_eq!(sigma_syms(&n), vec!["@U.F.V@".to_string()]);
        /* value omitted when NULL */
        let n = flag_create_symbol(FLAG_REQUIRE, "A", None);
        assert_eq!(sigma_syms(&n), vec!["@R.A@".to_string()]);
        /* value omitted when empty */
        let n = flag_create_symbol(FLAG_POSITIVE, "F", Some(""));
        assert_eq!(sigma_syms(&n), vec!["@P.F@".to_string()]);
    }

    // [spec:foma:sem:flags.flag-build-fn/test]
    // [spec:foma:sem:fomalib.flag-build-fn/test]
    #[test]
    fn flag_build_rows() {
        /* Different attribute names -> NONE immediately */
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_UNIFY, "G", Some("1")),
            NONE
        );

        /* U rows (first matching row wins) */
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_POSITIVE, "F", Some("1")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_CLEAR, "F", None),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_UNIFY, "F", Some("2")),
            FAIL
        );
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_POSITIVE, "F", Some("2")),
            FAIL
        );
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_NEGATIVE, "F", Some("1")),
            FAIL
        );
        /* U vs U equal value is explicitly NONE */
        assert_eq!(
            flag_build(FLAG_UNIFY, "F", Some("1"), FLAG_UNIFY, "F", Some("1")),
            NONE
        );

        /* R valueless */
        assert_eq!(
            flag_build(FLAG_REQUIRE, "F", None, FLAG_UNIFY, "F", Some("1")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_REQUIRE, "F", None, FLAG_POSITIVE, "F", Some("1")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_REQUIRE, "F", None, FLAG_NEGATIVE, "F", Some("1")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_REQUIRE, "F", None, FLAG_CLEAR, "F", None),
            FAIL
        );
        /* R with value */
        assert_eq!(
            flag_build(FLAG_REQUIRE, "F", Some("1"), FLAG_POSITIVE, "F", Some("1")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_REQUIRE, "F", Some("1"), FLAG_POSITIVE, "F", Some("2")),
            FAIL
        );

        /* D valueless */
        assert_eq!(
            flag_build(FLAG_DISALLOW, "F", None, FLAG_CLEAR, "F", None),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_DISALLOW, "F", None, FLAG_POSITIVE, "F", Some("1")),
            FAIL
        );
        /* D with value */
        assert_eq!(
            flag_build(FLAG_DISALLOW, "F", Some("1"), FLAG_POSITIVE, "F", Some("2")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_DISALLOW, "F", Some("1"), FLAG_NEGATIVE, "F", Some("1")),
            SUCCEED
        );
        assert_eq!(
            flag_build(FLAG_DISALLOW, "F", Some("1"), FLAG_POSITIVE, "F", Some("1")),
            FAIL
        );

        /* Any ftype of C/N/P/E yields NONE (masks the flag_eliminate `|` bug) */
        assert_eq!(
            flag_build(FLAG_POSITIVE, "F", Some("1"), FLAG_UNIFY, "F", Some("1")),
            NONE
        );
        assert_eq!(
            flag_build(FLAG_NEGATIVE, "F", Some("1"), FLAG_UNIFY, "F", Some("1")),
            NONE
        );
        assert_eq!(
            flag_build(FLAG_EQUAL, "F", Some("1"), FLAG_UNIFY, "F", Some("1")),
            NONE
        );
        assert_eq!(
            flag_build(FLAG_CLEAR, "F", None, FLAG_UNIFY, "F", Some("1")),
            NONE
        );
    }

    // [spec:foma:sem:flags.flag-extract-fn/test]
    #[test]
    fn flag_extract_from_sigma() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, r#""@U.F.1@" a "@R.G@""#, None, None).unwrap();
        let flags = flag_extract(&net);
        /* Collect (type, name, value) triples; non-flag "a" is excluded. */
        let mut got: Vec<(i32, Option<String>, Option<String>)> = Vec::new();
        let mut f = flags.as_deref();
        while let Some(fl) = f {
            got.push((
                fl.r#type,
                fl.name.as_deref().map(String::from),
                fl.value.as_deref().map(String::from),
            ));
            f = fl.next.as_deref();
        }
        got.sort();
        assert_eq!(
            got,
            vec![
                (FLAG_UNIFY, Some("F".to_string()), Some("1".to_string())),
                (FLAG_REQUIRE, Some("G".to_string()), None),
            ]
        );
        /* A net with no flag symbols yields the empty list. */
        let plain = fsm_parse_regex(opts, "a b c", None, None).unwrap();
        assert!(flag_extract(&plain).is_none());
    }

    // [spec:foma:sem:flags.flag-purge-fn/test]
    #[test]
    fn flag_purge_targeted() {
        let opts = &FomaOptions::default();
        /* Purge only attribute F; G survives. */
        let mut net = fsm_parse_regex(opts, r#""@U.F.1@" a "@U.G.1@""#, None, None).unwrap();
        flag_purge(&mut net, Some("F"));
        let syms = sigma_syms(&net);
        assert!(
            !syms.contains(&"@U.F.1@".to_string()),
            "F symbol removed from sigma"
        );
        assert!(syms.contains(&"@U.G.1@".to_string()), "G symbol kept");
        /* The F arc became epsilon; the G arc is unchanged. */
        let labels = arc_labels(&net);
        assert!(labels.iter().any(|(i, o)| i == "@U.G.1@" && o == "@U.G.1@"));
        assert!(!labels.iter().any(|(i, _)| i == "@U.F.1@"));
        assert_eq!(net.is_deterministic, NO);
        assert_eq!(net.is_minimized, NO);
        assert_eq!(net.is_epsilon_free, NO);

        /* name == None purges every flag. */
        let mut net2 = fsm_parse_regex(opts, r#""@U.F.1@" a "@U.G.1@""#, None, None).unwrap();
        flag_purge(&mut net2, None);
        let syms2 = sigma_syms(&net2);
        assert!(!syms2.contains(&"@U.F.1@".to_string()));
        assert!(!syms2.contains(&"@U.G.1@".to_string()));
        assert!(syms2.contains(&"a".to_string()));
    }

    // [spec:foma:sem:flags.flag-eliminate-fn+1/test]
    // [spec:foma:sem:fomalib.flag-eliminate-fn+1/test]
    #[test]
    fn flag_eliminate_end_to_end() {
        let opts = &FomaOptions::default();
        /* U/R/D flags. Surviving paths (verified against C foma):
        @P.F.1@ a @U.F.1@ -> "a" (U unify equal), the @P.F.2@ b @U.F.1@ path
        fails (U unify unequal); @P.G.1@ c @R.G@ -> "c" (R require satisfied),
        d @R.G@ fails (nothing set G); e @D.H@ -> "e" (D disallow, H unset),
        @P.H.1@ f @D.H@ fails (D disallow but H set). */
        let src = r#"["@P.F.1@" a "@U.F.1@"] | ["@P.F.2@" b "@U.F.1@"] | ["@P.G.1@" c "@R.G@"] | [d "@R.G@"] | [e "@D.H@"] | ["@P.H.1@" f "@D.H@"]"#;
        let net = fsm_parse_regex(opts, src, None, None).unwrap();
        let result = flag_eliminate(opts, net, None);
        /* Wave 4 fix (`&` type mask): the body now runs only for U/R/D/E flags.
        Because flag_build already classified nothing for the other types, the
        observable flag-filtered language is identical to the pre-fix behavior. */
        assert_eq!(all_words(&result), vec!["a", "c", "e"]);
        /* No flag symbols remain in sigma. */
        for s in sigma_syms(&result) {
            assert!(!(flag_check(&s)), "no flag symbols remain: {}", s);
        }

        /* Eliminating a single named attribute leaves the other flags' effect. */
        let net2 = fsm_parse_regex(
            opts,
            r#"["@P.F.1@" a "@U.F.1@"] | ["@P.F.2@" b "@U.F.1@"]"#,
            None,
            None,
        )
        .unwrap();
        let result2 = flag_eliminate(opts, net2, Some("F"));
        assert_eq!(all_words(&result2), vec!["a"]);
    }

    // [spec:foma:sem:flags.flag-twosided-fn/test]
    // [spec:foma:sem:fomalib.flag-twosided-fn/test]
    #[test]
    fn flag_twosided_arc_split() {
        let opts = &FomaOptions::default();
        /* Pure repair: a flag:epsilon arc gains the flag on the output tape
        (newarcs == 0, change == 1). */
        let net = fsm_parse_regex(opts, r#""@U.F.1@":0"#, None, None).unwrap();
        let net = flag_twosided(opts, net);
        assert_eq!(
            arc_labels(&net),
            vec![("@U.F.1@".to_string(), "@U.F.1@".to_string())]
        );

        /* Arc splitting: flag:real (in is a flag, out a real symbol, in != out)
        splits into flag:flag then epsilon:real via a fresh intermediate state. */
        let net = fsm_parse_regex(opts, r#""@U.F.1@":a"#, None, None).unwrap();
        let net = flag_twosided(opts, net);
        assert_eq!(
            arc_labels(&net),
            vec![
                ("0".to_string(), "a".to_string()),
                ("@U.F.1@".to_string(), "@U.F.1@".to_string()),
            ]
        );
        /* Three states on the split path (start -> S -> final). */
        assert_eq!(net.statecount, 3);
    }
}
