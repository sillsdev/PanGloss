//! foma/foma.c — the interactive CLI front-end, ported per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/foma.md
//! (the `foma.*` ids: main-fn, rl-gets-fn, print-help-fn, my-completion-fn,
//! my-generator-fn, xprintf-fn, add-history-fn).
//!
//! The annotated items below are literal ports of foma.c. The command-dispatch
//! machinery (`my_interfaceparse` and its helpers) is UNANNOTATED plumbing that
//! reproduces foma/interface.l's xfst command lexer: the same command set,
//! prefix/abbreviation semantics, multi-line `regex …;` / `define …;`
//! continuation, `apply up/down/med` repeat mode, `source`, comments and the
//! quit commands. It covers every command whose `iface_*` implementation exists
//! in `foma::iface`.
//!
//! DEVIATION from C: GNU readline is not linked. `rl_gets`, `my_completion`,
//! `my_generator` and `add_history` retain their structure for fidelity but read
//! from std stdin and are otherwise wired to nothing (no interactive history or
//! tab-completion at runtime).

use std::cell::{Cell, RefCell};
use std::io::{self, Write};
use std::process;

use foma::define::{Defined, add_defined, add_defined_function, find_defined, remove_defined};
use foma::iface::*;
use foma::io::file_to_mem;
use foma::regex::fsm_parse_regex;
use foma::session::Session;
use foma::structures::fsm_copy;
use foma::topsort::fsm_topsort;
use foma::types::{
    AP_D, AP_M, AP_U, BUILD_VERSION, MAJOR_VERSION, MINOR_VERSION, PROMPT_A, PROMPT_MAIN,
    STATUS_VERSION,
};

/* interface.l: #define RE 0 (regex) / #define DE 1 (define) */
const RE: i32 = 0;
const DE: i32 = 1;

/* C: `char *usagestring = ...` (foma.c) — printed by print_help and on a bad
option. */
const USAGESTRING: &str =
    "Usage: foma [-e \"command\"] [-f run-once-script] [-l startupscript] [-p] [-q] [-s] [-v]\n";

/* C: `char disclaimer[] = ...` (foma.c) — the startup banner. */
const DISCLAIMER: &str = "Foma, version 0.10.0\nCopyright © 2008-2021 Mans Hulden\nThis is free software; see the source code for copying conditions.\nThere is ABSOLUTELY NO WARRANTY; for details, type \"help license\"\n\nType \"help\" to list all commands available.\nType \"help <topic>\" or help \"<operator>\" for further help.\n\n";

/* C: `char *cmd[]` (foma.c) — the readline completion command table. */
static CMD: &[&str] = &[
    "ambiguous upper",
    "apply down",
    "apply med",
    "apply up",
    "apropos",
    "assert-stack",
    "clear stack",
    "close sigma",
    "compact sigma",
    "complete net",
    "compose net",
    "concatenate net",
    "crossproduct net",
    "define",
    "determinize net",
    "echo",
    "eliminate flags",
    "eliminate flag",
    "export cmatrix",
    "extract ambiguous",
    "extract unambiguous",
    "factorize",
    "help license",
    "help warranty",
    "ignore net",
    "intersect net",
    "invert net",
    "label net",
    "letter machine",
    "load defined",
    "lower-side net",
    "minimize net",
    "name net",
    "negate net",
    "one-plus net",
    "pop stack",
    "print defined",
    "print dot",
    "print lower-words",
    "print cmatrix",
    "print name",
    "print net",
    "print random-lower",
    "print random-upper",
    "print random-words",
    "print sigma",
    "print size",
    "print shortest-string",
    "print shortest-string-length",
    "print words",
    "print pairs",
    "print random-pairs",
    "print upper-words",
    "prune net",
    "push defined",
    "quit",
    "read att",
    "read cmatrix",
    "read prolog",
    "read lexc",
    "read regex",
    "read spaced-text",
    "read text",
    "reverse net",
    "rotate stack",
    "save defined",
    "save stack",
    "sequentialize",
    "set",
    "show variables",
    "show variable",
    "shuffle net",
    "sigma",
    "sigma net",
    "source",
    "sort in",
    "sort net",
    "sort out",
    "substitute defined",
    "substitute symbol",
    "system",
    "test unambiguous",
    "test equivalent",
    "test functional",
    "test identity",
    "test lower-universal",
    "test upper-universal",
    "test non-null",
    "test null",
    "test sequential",
    "turn stack",
    "twosided flag-diacritics",
    "undefine",
    "union net",
    "upper-side net",
    "view net",
    "write att",
    "write prolog",
    "zero-plus net",
];

/* C: `char *abbrvcmd[]` (foma.c) — the completion abbreviation table. */
static ABBRVCMD: &[&str] = &[
    "ambiguous",
    "close",
    "down",
    "up",
    "med",
    "size",
    "loadd",
    "lower-words",
    "upper-words",
    "net",
    "random-lower",
    "random-upper",
    "words",
    "random-words",
    "regex",
    "rpl",
    "au revoir",
    "bye",
    "exit",
    "saved",
    "seq",
    "ss",
    "stack",
    "tunam",
    "tid",
    "tfu",
    "tlu",
    "tuu",
    "tnu",
    "tnn",
    "tseq",
    "tsf",
    "equ",
    "pss",
    "psz",
    "ratt",
    "tfd",
    "hyvästi",
    "watt",
    "wpl",
    "examb",
    "exunamb",
    "pairs",
    "random-pairs",
];

/* Front-end behavior variables. C: `int pipe_mode`, `static int use_readline`,
`int promptmode`, `int apply_direction`, plus interface.l's `int input_is_file`.
File-static/global → thread_local per the conventions (they persist across
my_interfaceparse calls, exactly as the flex start-condition `yy_start` does). */
thread_local! {
    static PIPE_MODE: Cell<i32> = const { Cell::new(0) };
    static USE_READLINE: Cell<i32> = const { Cell::new(1) };
    static PROMPTMODE: Cell<i32> = const { Cell::new(PROMPT_MAIN) };
    static APPLY_DIRECTION: Cell<i32> = const { Cell::new(0) };
    static INPUT_IS_FILE: Cell<i32> = const { Cell::new(0) };
    static LINENO: Cell<i32> = const { Cell::new(1) };
    /// Persistent "in REGEX/DEFINE start-condition" state (survives across
    /// my_interfaceparse calls, like flex's global yy_start).
    static PENDING_REGEX: RefCell<Option<PendingRegex>> = const { RefCell::new(None) };
    /* readline completion statics (my_completion/my_generator). */
    static SMATCH: Cell<usize> = const { Cell::new(0) };
    static RL_LINE_BUFFER: RefCell<String> = const { RefCell::new(String::new()) };
    static RL_POINT: Cell<usize> = const { Cell::new(0) };
    static GEN_LIST_INDEX: Cell<usize> = const { Cell::new(0) };
    static GEN_LIST_INDEX2: Cell<usize> = const { Cell::new(0) };
    static GEN_LEN: Cell<usize> = const { Cell::new(0) };
    static GEN_NUMMATCHES: Cell<i32> = const { Cell::new(0) };
}

struct PendingRegex {
    pmode: i32,
    defname: String,
    accum: String,
}

// [spec:foma:def:foma.xprintf-fn]
// [spec:foma:def:fomalibconf.xprintf-fn]
// [spec:foma:sem:foma.xprintf-fn]
// [spec:foma:sem:fomalibconf.xprintf-fn]
// C: `void xprintf(char *string) { return ; printf("%s",string); }` — the
// `return;` makes the printf unreachable dead code (a disabled output hook).
#[allow(dead_code, unused_variables)]
fn xprintf(string: &str) {
    return;
    #[allow(unreachable_code)]
    {
        print!("{}", string);
    }
}

// [spec:foma:def:foma.add-history-fn]
// [spec:foma:sem:foma.add-history-fn]
// C: `extern int add_history(const char *)` — resolved from libreadline at link
// time; appends the line to readline's interactive history.
// DEVIATION from C: readline is not linked; this is a no-op stand-in. A real
// port would wire this to an equivalent line-history facility.
#[allow(unused_variables)]
fn add_history(line: &str) -> i32 {
    0
}

// [spec:foma:def:foma.rl-gets-fn]
// [spec:foma:sem:foma.rl-gets-fn]
// C returns a pointer the caller must not free, or NULL on EOF. DEVIATION from
// C: readline is not linked, so both branches read a line from std stdin; the
// use_readline flag still selects the two code paths (and the -r option still
// toggles it) for structural fidelity.
fn rl_gets(prompt: &str) -> Option<String> {
    if USE_READLINE.with(|u| u.get()) == 0 {
        // C use_readline == 0: printf the prompt, fgets, strip_newline.
        print!("{}", prompt);
        io::stdout().flush().expect("flush stdout");
        let mut line = String::new();
        let n = io::stdin().read_line(&mut line).unwrap_or(0);
        if n == 0 {
            return None;
        }
        // strip_newline: replace the first '\n' with NUL (truncate there).
        if let Some(p) = line.find('\n') {
            line.truncate(p);
        }
        Some(line)
    } else {
        // C use_readline == 1: readline(prompt) (which displays the prompt); add
        // the non-empty line to history. Here: print the prompt ourselves (no
        // readline to display it) and read from stdin.
        print!("{}", prompt);
        io::stdout().flush().expect("flush stdout");
        let mut line = String::new();
        let n = io::stdin().read_line(&mut line).unwrap_or(0);
        if n == 0 {
            return None;
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        if !line.is_empty() {
            add_history(&line);
        }
        Some(line)
    }
}

// [spec:foma:def:foma.print-help-fn]
// [spec:foma:sem:foma.print-help-fn]
fn print_help() {
    print!("{}", USAGESTRING);
    println!("Options:");
    println!("-e \"command\"\texecute a command on startup (-e can be invoked several times)");
    println!("-f scriptfile\tread commands from scriptfile on startup, and quit");
    println!("-l scriptfile\tread commands from scriptfile on startup");
    println!("-p\t\tpipe-mode");
    println!("-q\t\tquiet mode (more quiet than pipe-mode)");
    println!("-r\t\tdon't use readline library for input");
    println!("-s\t\tstop execution and exit");
    println!("-v\t\tprint version number");
}

// [spec:foma:def:foma.my-completion-fn]
// [spec:foma:sem:foma.my-completion-fn]
// C readline attempted-completion hook: stores `start` (word column) into the
// file-static `smatch`, then returns rl_completion_matches(text, my_generator).
// DEVIATION from C (readline not linked; retained for fidelity, wired to
// nothing): we stash the line/point into the my_generator statics and emulate
// rl_completion_matches by driving my_generator ourselves.
#[allow(dead_code)]
fn my_completion(text: &str, start: usize, end: usize) -> Vec<String> {
    SMATCH.with(|s| s.set(start));
    RL_LINE_BUFFER.with(|b| *b.borrow_mut() = text.to_string());
    RL_POINT.with(|p| p.set(end));
    let mut matches: Vec<String> = Vec::new();
    let mut state = 0;
    while let Some(m) = my_generator(text, state) {
        matches.push(m);
        state = 1;
    }
    matches
}

// [spec:foma:def:foma.my-generator-fn]
// [spec:foma:sem:foma.my-generator-fn]
// C readline match generator: ignores `text`, matches the WHOLE line
// (rl_line_buffer) against cmd[] (then abbrvcmd[] when rl_point > 0), returning
// strdup(name + smatch) for each prefix hit; resets its static cursors on
// state == 0. DEVIATION from C (readline not linked): rl_line_buffer/rl_point
// are the stand-in thread_locals set by my_completion.
#[allow(unused_variables)]
fn my_generator(text: &str, state: i32) -> Option<String> {
    let text = RL_LINE_BUFFER.with(|b| b.borrow().clone());
    if state == 0 {
        GEN_LIST_INDEX.with(|c| c.set(0));
        GEN_LIST_INDEX2.with(|c| c.set(0));
        GEN_NUMMATCHES.with(|c| c.set(0));
        GEN_LEN.with(|c| c.set(text.len()));
    }
    let smatch = SMATCH.with(|s| s.get());

    // Scan cmd[] (strncmp(name, text, len) == 0 ↔ text is a prefix of name).
    loop {
        let li = GEN_LIST_INDEX.with(|c| c.get());
        if li >= CMD.len() {
            break;
        }
        GEN_LIST_INDEX.with(|c| c.set(li + 1));
        let name = CMD[li];
        if name.as_bytes().starts_with(text.as_bytes()) {
            GEN_NUMMATCHES.with(|c| c.set(c.get() + 1));
            return Some(name.get(smatch..).unwrap_or("").to_string());
        }
    }

    // C: `if (rl_point > 0)` before scanning the abbreviations.
    if RL_POINT.with(|p| p.get()) > 0 {
        loop {
            let li = GEN_LIST_INDEX2.with(|c| c.get());
            if li >= ABBRVCMD.len() {
                break;
            }
            GEN_LIST_INDEX2.with(|c| c.set(li + 1));
            let name = ABBRVCMD[li];
            if name.as_bytes().starts_with(text.as_bytes()) {
                return Some(name.get(smatch..).unwrap_or("").to_string());
            }
        }
    }
    None
}

// [spec:foma:def:foma.main-fn]
// [spec:foma:sem:foma.main-fn]
fn main() {
    // Route library diagnostics (tracing events) to stderr in a compact,
    // CLI-friendly form. Library INFO events are already gated behind the
    // interpreter's `verbose` flag, so a fixed INFO level here shows warnings
    // and errors always, progress only when the emitting code opts in.
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let args: Vec<String> = std::env::args().collect();
    let argv0 = args.first().cloned().unwrap_or_else(|| "foma".to_string());

    let mut session = Session::new();
    // DEVIATION from C: `srand((unsigned int)time(NULL))` seeds dynarray's
    // crate-private LCG, which this (separate) binary crate cannot reach.
    // apply_init reseeds that LCG with time(NULL) before any random enumeration,
    // so runtime randomness is preserved; only startup net auto-naming (before
    // the first apply) uses the default seed.
    // getopt(argc, argv, "e:f:hl:pqrsv"), acted on in command-line order.
    let mut idx = 1;
    while idx < args.len() {
        let cur = args[idx].clone();
        let cb = cur.as_bytes();
        if cb.len() < 2 || cb[0] != b'-' {
            break; // first non-option: getopt returns -1
        }
        let mut k = 1;
        while k < cb.len() {
            let opt = cb[k] as char;
            match opt {
                'e' => {
                    match getoptarg(&args, &mut idx, &cur, &mut k) {
                        Some(a) => my_interfaceparse(&mut session, &a),
                        None => usage_err(),
                    }
                    break;
                }
                'f' => {
                    if let Some(a) = getoptarg(&args, &mut idx, &cur, &mut k) {
                        if let Ok(bytes) = file_to_mem(&a) {
                            INPUT_IS_FILE.with(|f| f.set(1));
                            my_interfaceparse(&mut session, &String::from_utf8_lossy(&bytes));
                        }
                    }
                    process::exit(0);
                }
                'h' => {
                    print_help();
                    process::exit(0);
                }
                'l' => {
                    if let Some(a) = getoptarg(&args, &mut idx, &cur, &mut k) {
                        if let Ok(bytes) = file_to_mem(&a) {
                            INPUT_IS_FILE.with(|f| f.set(1));
                            my_interfaceparse(&mut session, &String::from_utf8_lossy(&bytes));
                        }
                    }
                    break;
                }
                'p' => {
                    PIPE_MODE.with(|p| p.set(1));
                    k += 1;
                }
                'q' => {
                    session.opts.verbose = false;
                    k += 1;
                }
                'r' => {
                    USE_READLINE.with(|u| u.set(0));
                    k += 1;
                }
                's' => {
                    process::exit(0);
                }
                'v' => {
                    println!(
                        "{} {}.{}.{}{}",
                        argv0, MAJOR_VERSION, MINOR_VERSION, BUILD_VERSION, STATUS_VERSION
                    );
                    process::exit(0);
                }
                _ => usage_err(),
            }
        }
        idx += 1;
    }

    if PIPE_MODE.with(|p| p.get()) == 0 && session.opts.verbose {
        print!("{}", DISCLAIMER);
    }
    // C: rl_basic_word_break_characters = " >";
    //    rl_attempted_completion_function = my_completion;  (no-op without readline)

    loop {
        let promptmode = PROMPTMODE.with(|p| p.get());
        let mut prompt = if promptmode == PROMPT_MAIN {
            format!("foma[{}]: ", session.stack_size())
        } else {
            let d = APPLY_DIRECTION.with(|d| d.get());
            if d == AP_D {
                "apply down> ".to_string()
            } else if d == AP_U {
                "apply up> ".to_string()
            } else if d == AP_M {
                "apply med> ".to_string()
            } else {
                String::new()
            }
        };
        if PIPE_MODE.with(|p| p.get()) != 0 || !session.opts.verbose {
            prompt = String::new();
        }

        io::stdout().flush().expect("flush stdout");

        let command = rl_gets(&prompt);
        match command {
            None if promptmode == PROMPT_MAIN => {
                println!();
                process::exit(0);
            }
            None => {
                // EOF at an apply prompt: reset to the main prompt and continue.
                PROMPTMODE.with(|p| p.set(PROMPT_MAIN));
                println!();
                continue;
            }
            Some(cmd) => {
                INPUT_IS_FILE.with(|f| f.set(0));
                my_interfaceparse(&mut session, &cmd);
            }
        }
    }
}

/* getopt argument fetch: the rest of the current arg after the option letter,
else the next argv element. */
fn getoptarg(args: &[String], idx: &mut usize, cur: &str, k: &mut usize) -> Option<String> {
    let after = &cur[(*k + 1)..];
    if !after.is_empty() {
        *k = cur.len();
        Some(after.to_string())
    } else {
        *idx += 1;
        args.get(*idx).cloned()
    }
}

/* C: fprintf(stderr, "%s", usagestring); exit(EXIT_FAILURE); */
fn usage_err() -> ! {
    eprint!("{}", USAGESTRING);
    process::exit(1);
}

// ────────────────────────────────────────────────────────────────────────────
// Command dispatch — UNANNOTATED plumbing reproducing foma/interface.l.
// ────────────────────────────────────────────────────────────────────────────

/* interface.l my_interfaceparse: scan a whole buffer (a -e/-f/-l/source file or
one REPL line) and dispatch its commands. Persistent state (PROMPTMODE apply
mode, PENDING_REGEX) survives across calls, matching flex's global yy_start. */
fn my_interfaceparse(session: &mut Session, buffer: &str) {
    // interface.l scans a NUL-terminated C string (file_to_mem buffers carry a
    // trailing '\0'); flex stops at the first NUL, so trim there before splitting
    // into lines — otherwise the terminator becomes a spurious final command.
    let buffer = match buffer.find('\0') {
        Some(i) => &buffer[..i],
        None => buffer,
    };
    LINENO.with(|l| l.set(1));
    for raw in buffer.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if !process_line(session, line) {
            return; // an unknown command aborts the buffer (flex returns 1)
        }
        LINENO.with(|l| l.set(l.get() + 1));
    }
}

fn process_line(session: &mut Session, line: &str) -> bool {
    if PENDING_REGEX.with(|p| p.borrow().is_some()) {
        return regex_feed(session, line);
    }
    if PROMPTMODE.with(|p| p.get()) == PROMPT_A {
        return apply_feed(session, line);
    }
    dispatch(session, line)
}

/* <APPLY_P>: each line is an apply input word until "END;" or (in the REPL) EOF.
Empty lines are ignored (they don't match {NONL}+). */
fn apply_feed(session: &mut Session, line: &str) -> bool {
    if line.is_empty() {
        return true;
    }
    if line == "END;" {
        PROMPTMODE.with(|p| p.set(PROMPT_MAIN));
        return true;
    }
    let d = APPLY_DIRECTION.with(|d| d.get());
    if d == AP_D {
        iface_apply_down(session, line);
    } else if d == AP_M {
        iface_apply_med(session, line);
    } else if d == AP_U {
        iface_apply_up(session, line);
    }
    true
}

/* <REGEX> continuation: append the line and scan for the terminating top-level
`;`. When found, compile and re-dispatch any leftover on the same line. */
fn regex_feed(session: &mut Session, line: &str) -> bool {
    regex_append_and_scan(session, line, true)
}

fn start_regex(session: &mut Session, pmode: i32, defname: String, initial: &str) -> bool {
    PENDING_REGEX.with(|p| {
        *p.borrow_mut() = Some(PendingRegex {
            pmode,
            defname,
            accum: String::new(),
        })
    });
    regex_append_and_scan(session, initial, false)
}

fn regex_append_and_scan(session: &mut Session, text: &str, prepend_nl: bool) -> bool {
    let scan = PENDING_REGEX.with(|p| {
        let mut b = p.borrow_mut();
        let pr = b
            .as_mut()
            .expect("pending regex active when append_and_scan runs");
        if prepend_nl {
            pr.accum.push('\n');
        }
        pr.accum.push_str(text);
        find_regex_terminator(&pr.accum).map(|i| {
            let body = pr.accum[..i].to_string();
            let leftover = pr.accum[i + 1..].to_string();
            (body, leftover, pr.pmode, pr.defname.clone())
        })
    });
    match scan {
        None => true, // no `;` yet: keep accumulating on the next line
        Some((body, leftover, pmode, defname)) => {
            PENDING_REGEX.with(|p| *p.borrow_mut() = None);
            compile_regex(session, pmode, &defname, &body);
            process_line(session, &leftover)
        }
    }
}

/* Find the byte index of the top-level `;` that terminates a regex, honoring
`{…}`, `"…"`, `#`/`!` line comments and the `.#` word-boundary digraph, exactly
as interface.l's REGEX/REGEXB/REGEXQ/RCOMMENT sub-states do. */
fn find_regex_terminator(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = 0usize;
    let (mut in_brace, mut in_quote, mut in_comment) = (false, false, false);
    while i < b.len() {
        let c = b[i];
        if in_comment {
            if c == b'\n' {
                in_comment = false;
            }
            i += 1;
            continue;
        }
        if in_quote {
            if c == b'"' {
                in_quote = false;
            }
            i += 1;
            continue;
        }
        if in_brace {
            if c == b'}' {
                in_brace = false;
            }
            i += 1;
            continue;
        }
        if c == b'.' && i + 1 < b.len() && b[i + 1] == b'#' {
            i += 2; // `.#` digraph — the '#' is not a comment start here
            continue;
        }
        match c {
            b'#' | b'!' => in_comment = true,
            b'{' => in_brace = true,
            b'"' => in_quote = true,
            b';' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/* interface.l <REGEX>(;) action: parse the accumulated body; on success push
(RE) or define (DE) fsm_topsort(fsm_minimize(&session.opts, current_parse)). */
fn compile_regex(session: &mut Session, pmode: i32, defname: &str, body: &str) {
    let verbose = session.opts.verbose;
    let parsed = fsm_parse_regex(
        &session.opts,
        body,
        Some(&mut session.defines),
        Some(&mut session.defines_f),
    );
    match parsed {
        None => {
            // DEVIATION from C: C prints "invalid regex detected" only when
            // the parse succeeds but minimize returns NULL; on a syntax
            // error the nfst-xre parser has already printed a diagnostic.
            // We cannot tell the two apart, so we stay silent here to avoid
            // double-reporting a syntax error.
        }
        Some(net) => {
            let tempnet = fsm_topsort(net);
            if pmode == RE {
                session.stack_add(tempnet); // prints stats itself when verbose
            } else {
                let olddef = add_defined(&mut session.defines, Some(tempnet), defname);
                if verbose {
                    if olddef == Defined::Redefined {
                        print!("redefined {}: ", defname);
                    } else {
                        print!("defined {}: ", defname);
                    }
                    if let Some(n) = find_defined(&mut session.defines, defname) {
                        print_stats(n);
                    }
                }
            }
        }
    }
}

/* interface.l <DEFI> "define NAME" (no regex body): name the top-of-stack net. */
fn define_top_of_stack(session: &mut Session, name: &str) {
    if !iface_stack_check(session, 1) {
        return;
    }
    let net = session.stack_pop();
    let name2 = name.trim_end_matches(';');
    let verbose = session.opts.verbose;
    let olddef = add_defined(&mut session.defines, net, name2);
    if verbose {
        // C: `olddef != 0` — a NameTooLong (-1) net here also printed "redefined",
        // a quirk preserved by testing against New rather than Redefined alone.
        if olddef != Defined::New {
            print!("redefined {}: ", name2);
        } else {
            print!("defined {}: ", name2);
        }
        if let Some(n) = find_defined(&mut session.defines, name2) {
            print_stats(n);
        }
    }
}

/* interface.l <DEFI>/FUNC_* "define NAME(args) body": store the function body
with each argument name rewritten to @ARGUMENTNN@.
DEVIATION from C: C stores the name as "NAME(" (with the paren); this port stores
the bare NAME so that regex.rs's function_apply (which looks up the nfst-xre
FunctionCall name, paren stripped) resolves it. */
fn define_function(session: &mut Session, name: &str, args: &[String], body: &str) {
    let numargs = args.len() as i32;
    let funcdef = substitute_func_args(body, args);
    add_defined_function(
        &session.opts,
        &mut session.defines_f,
        name,
        &funcdef,
        numargs,
    );
}

fn substitute_func_args(body: &str, args: &[String]) -> String {
    let b = body.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c == b'{' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'}' {
                i += 1;
            }
            if i < b.len() {
                i += 1;
            }
            out.push_str(&body[start..i]);
            continue;
        }
        if c == b'"' {
            let start = i;
            i += 1;
            while i < b.len() && b[i] != b'"' {
                i += 1;
            }
            if i < b.len() {
                i += 1;
            }
            out.push_str(&body[start..i]);
            continue;
        }
        if c == b'%' && i + 1 < b.len() {
            let cl = utf8_len(b[i + 1]);
            let end = (i + 1 + cl).min(b.len());
            out.push_str(&body[i..end]);
            i = end;
            continue;
        }
        if is_token_byte(c) {
            let start = i;
            while i < b.len() && is_token_byte(b[i]) {
                i += 1;
            }
            let tok = &body[start..i];
            match args.iter().position(|a| a == tok) {
                Some(idx) => out.push_str(&format!("@ARGUMENT{:02}@", idx + 1)),
                None => out.push_str(tok),
            }
            continue;
        }
        out.push(c as char);
        i += 1;
    }
    out
}

fn is_token_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'?' || c == b'\'' || c == b'=' || c >= 0x80
}

fn utf8_len(lead: u8) -> usize {
    if lead < 0x80 {
        1
    } else if lead >> 5 == 0b110 {
        2
    } else if lead >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

// ───────────────────────── the command matcher ─────────────────────────

fn dispatch(session: &mut Session, line: &str) -> bool {
    let t = lstrip(line);
    if t.is_empty() {
        return true;
    }
    if t.starts_with('#') || t.starts_with('!') {
        return true; // comment line
    }
    let ws: Vec<&str> = t.split_whitespace().collect();
    let w0 = ws[0];
    let w1 = ws.get(1).copied().unwrap_or("");

    // quit / exit / bye / au revoir / hyvästi
    if w0 == "quit"
        || w0 == "exit"
        || w0 == "bye"
        || w0 == "hyvästi"
        || (w0 == "au" && w1 == "revoir")
    {
        iface_quit(session); // never returns
        return true;
    }

    // apply up/down/med (single word, bare repeat-mode, or < file redirection)
    {
        let (dirword, nskip) = if w0 == "apply" && ws.len() >= 2 {
            (w1, 2)
        } else {
            (w0, 1)
        };
        let dir = match dirword {
            "down" => Some(AP_D),
            "up" => Some(AP_U),
            "med" => Some(AP_M),
            _ => None,
        };
        if let Some(dir) = dir {
            let arg = arg_after(t, nskip);
            if arg.is_empty() {
                if iface_stack_check(session, 1) {
                    PROMPTMODE.with(|p| p.set(PROMPT_A));
                    APPLY_DIRECTION.with(|d| d.set(dir));
                }
                return true;
            }
            if (dir == AP_D || dir == AP_U) && arg.starts_with('<') {
                let a2 = arg[1..].trim();
                if let Some(gp) = a2.find('>') {
                    iface_apply_file(session, a2[..gp].trim(), Some(a2[gp + 1..].trim()), dir);
                } else {
                    iface_apply_file(session, a2.trim(), None, dir);
                }
                return true;
            }
            if dir == AP_D {
                iface_apply_down(session, &arg);
            } else if dir == AP_U {
                iface_apply_up(session, &arg);
            } else {
                iface_apply_med(session, &arg);
            }
            return true;
        }
    }

    // define NAME … / define NAME(args) … / define NAME (top of stack)
    if pfx(w0, "define", 2) && ws.len() >= 2 {
        return handle_define(session, &arg_after(t, 1));
    }

    // NAME = body  (define shorthand)
    if let Some(eq) = t.find('=') {
        let left = t[..eq].trim();
        if !left.is_empty() && !left.contains([' ', '\t', '#', '!']) {
            return start_regex(session, DE, left.to_string(), &t[eq + 1..]);
        }
    }

    // regex / read family
    if let Some(r) = try_read_or_regex(session, t, w0, w1) {
        return r;
    }

    // apropos <string>
    if pfx(w0, "apropos", 3) && ws.len() >= 2 {
        iface_apropos(&arg_after(t, 1));
        return true;
    }
    // assert-stack <n>
    if w0 == "assert-stack" {
        let level: i32 = arg_after(t, 1).trim().parse().unwrap_or(0);
        if level != session.stack_size() {
            eprintln!("Stack size {} not {}", session.stack_size(), level);
            process::exit(1);
        }
        return true;
    }
    // help / help <topic> / (help) license|warranty
    if w0 == "license" || w0 == "licence" || w0 == "warranty" {
        iface_warranty();
        return true;
    }
    if w0 == "help"
        || pfx(w0, "help", 1)
            && ws.len() >= 2
            && (w1 == "license" || w1 == "licence" || w1 == "warranty")
    {
        if w0 == "help" {
            if ws.len() == 1 {
                iface_help();
            } else if w1 == "license" || w1 == "licence" || w1 == "warranty" {
                iface_warranty();
            } else {
                iface_help_search(&arg_after(t, 1));
            }
        } else {
            iface_warranty();
        }
        return true;
    }

    // sigma net / label net / letter machine  (before the print/short-form family)
    if w0 == "sigma" && pfx(w1, "net", 1) {
        iface_sigma_net(session);
        return true;
    }
    if w0 == "label" && pfx(w1, "net", 1) {
        iface_label_net(session);
        return true;
    }
    if w0 == "letter" && pfx(w1, "machine", 1) {
        iface_letter_machine(session);
        return true;
    }

    // name [net] <string>  (else "name"/"na" alone falls through to print name)
    if pfx(w0, "name", 2) {
        let nskip = if pfx(w1, "net", 1) { 2 } else { 1 };
        let arg = arg_after(t, nskip);
        if !arg.is_empty() {
            iface_name_net(session, &arg);
            return true;
        }
    }

    // eliminate flags / eliminate flag <name>
    if w0 == "eliminate" {
        if w1 == "flags" {
            iface_eliminate_flags(session);
            return true;
        }
        if w1 == "flag" && ws.len() >= 3 {
            iface_eliminate_flag(session, &arg_after(t, 2));
            return true;
        }
    }

    // export cmatrix [> file]
    if w0 == "export" && pfx(w1, "cmatrix", 3) {
        let arg = arg_after(t, 2);
        if arg.is_empty() {
            iface_print_cmatrix_att(session, None);
        } else {
            iface_print_cmatrix_att(session, Some(strip_redir(&arg).as_str()));
        }
        return true;
    }

    // substitute defined|symbol X for Y
    if pfx(w0, "substitute", 3) && ws.len() >= 2 {
        return handle_substitute(session, t, w1);
    }

    // set / show
    if w0 == "set" && ws.len() >= 3 {
        iface_set_variable(session, w1, ws[2]);
        return true;
    }
    if w0 == "show" && ws.len() >= 2 {
        if pfx(w1, "variables", 3) {
            if ws.len() == 2 {
                iface_show_variables(session);
            } else {
                iface_show_variable(session, ws[2]);
            }
        } else {
            iface_show_variable(session, w1);
        }
        return true;
    }

    // load / save
    if w0 == "loadd" {
        iface_load_defined(session, &arg_after(t, 1));
        return true;
    }
    if w0 == "load" {
        if w1 == "defined" {
            iface_load_defined(session, &arg_after(t, 2));
        } else if w1 == "stack" {
            iface_load_stack(session, &arg_after(t, 2));
        } else {
            iface_load_stack(session, &arg_after(t, 1));
        }
        return true;
    }
    if w0 == "saved" {
        iface_save_defined(session, &strip_redir(&arg_after(t, 1)));
        return true;
    }
    if w0 == "ss" {
        iface_save_stack(session, &strip_redir(&arg_after(t, 1)));
        return true;
    }
    if w0 == "save" {
        if w1 == "defined" {
            iface_save_defined(session, &strip_redir(&arg_after(t, 2)));
            return true;
        }
        if w1 == "stack" {
            iface_save_stack(session, &strip_redir(&arg_after(t, 2)));
            return true;
        }
    }

    // push [defined] <name>
    if pfx(w0, "push", 2) {
        let nskip = if pfx(w1, "defined", 3) { 2 } else { 1 };
        let name = arg_after(t, nskip);
        if !name.is_empty() {
            iface_push(session, &name);
        }
        return true;
    }
    // undefine <name>
    if pfx(w0, "undefine", 3) && ws.len() >= 2 {
        let name = arg_after(t, 1);
        let name = name.trim_end_matches(';');
        remove_defined(&mut session.defines, Some(name));
        return true;
    }

    // system <cmd>
    if pfx(w0, "system", 2) && ws.len() >= 2 {
        let cmd = arg_after(t, 1);
        let _ = process::Command::new("sh").arg("-c").arg(&cmd).status();
        return true;
    }
    // source <file>
    if pfx(w0, "source", 3) && ws.len() >= 2 {
        let file = arg_after(t, 1);
        match file_to_mem(&file).ok() {
            Some(bytes) => {
                println!("Opening file '{}'.", file);
                INPUT_IS_FILE.with(|f| f.set(1));
                my_interfaceparse(session, &String::from_utf8_lossy(&bytes));
            }
            None => println!("Error opening file '{}'", file),
        }
        return true;
    }
    // echo / echo <string>
    if w0 == "echo" {
        if ws.len() == 1 {
            println!();
        } else {
            // print everything after "echo" + one whitespace, raw.
            let after = &t[4..];
            let after = after
                .strip_prefix(' ')
                .or_else(|| after.strip_prefix('\t'))
                .unwrap_or(after);
            println!("{}", after);
        }
        return true;
    }

    // write att / write prolog (with optional filename)
    if w0 == "watt" {
        return write_att_cmd(session, &arg_after(t, 1));
    }
    if w0 == "wpl" {
        return write_prolog_cmd(session, &arg_after(t, 1));
    }
    if pfx(w0, "write", 2) && ws.len() >= 2 {
        if pfx(w1, "att", 2) {
            return write_att_cmd(session, &arg_after(t, 2));
        }
        if pfx(w1, "prolog", 4) {
            return write_prolog_cmd(session, &arg_after(t, 2));
        }
    }

    // read shorthands ratt/rpl already handled in try_read_or_regex.

    // test family & abbreviations
    if let Some(r) = try_test(session, w0, w1) {
        return r;
    }

    // print family and bare short forms (net, sigma, words, pss, …)
    if let Some(r) = try_print_family(session, t, &ws) {
        return r;
    }

    // all remaining zero-argument commands
    if let Some(r) = try_zero_arg(session, w0, w1) {
        return r;
    }

    // Unknown command.
    if INPUT_IS_FILE.with(|f| f.get()) == 0 {
        println!("Unknown command. Ignoring until end of line.");
    } else {
        println!(
            "***Unknown command '{}' on line {}. Aborting.",
            t,
            LINENO.with(|l| l.get())
        );
    }
    false
}

fn handle_define(session: &mut Session, rest: &str) -> bool {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return true;
    }
    let name_end = rest.find([' ', '\t', '(']).unwrap_or(rest.len());
    if rest[name_end..].starts_with('(') {
        // function definition
        let name = &rest[..name_end];
        let after = &rest[name_end + 1..];
        let (arglist, body) = match after.find(')') {
            Some(cp) => (&after[..cp], after[cp + 1..].to_string()),
            None => (after, String::new()),
        };
        let args: Vec<String> = arglist
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        define_function(session, name, &args, body.trim());
        return true;
    }
    let name = &rest[..name_end];
    let body = rest[name_end..].trim();
    if body.is_empty() || body == ";" {
        define_top_of_stack(session, name);
        true
    } else {
        start_regex(session, DE, name.to_string(), body)
    }
}

fn handle_substitute(session: &mut Session, t: &str, w1: &str) -> bool {
    // "substitute defined|symbol X for Y" → replace Y with X.
    let (is_defined, is_symbol) = (pfx(w1, "defined", 3), pfx(w1, "symbol", 3));
    if !is_defined && !is_symbol {
        return true;
    }
    // tokens after "substitute defined": X for Y…
    let rest = arg_after(t, 2);
    let rw: Vec<&str> = rest.split_whitespace().collect();
    if rw.len() < 3 || rw[1] != "for" {
        return true;
    }
    let x = rw[0];
    // Y = everything after the "for" token.
    let y = arg_after(&rest, 2);
    if is_defined {
        iface_substitute_defined(session, &y, x);
    } else {
        iface_substitute_symbol(session, &y, x);
    }
    true
}

fn try_read_or_regex(session: &mut Session, t: &str, w0: &str, w1: &str) -> Option<bool> {
    // Standalone read abbreviations.
    if w0 == "ratt" {
        iface_read_att(session, &read_file_arg(t, 1));
        return Some(true);
    }
    if w0 == "rpl" {
        iface_read_prolog(session, &read_file_arg(t, 1));
        return Some(true);
    }
    // "reg"/"rege"/"regex" (min 3) → always a regex; body follows.
    if pfx(w0, "regex", 3) {
        return Some(start_regex(session, RE, String::new(), &arg_after(t, 1)));
    }
    // "re"/"rea"/"read": either a read subcommand or a bare "re <regex>".
    let is_re_prefix = w0 == "re" || pfx(w0, "read", 3);
    if is_re_prefix {
        if let Some(r) = read_subcommand(session, t, w1) {
            return Some(r);
        }
        // "re <stuff>" with no read-subcommand → regex; "read <stuff>" → unknown.
        if w0 == "re" {
            return Some(start_regex(session, RE, String::new(), &arg_after(t, 1)));
        }
        return None;
    }
    None
}

fn read_subcommand(session: &mut Session, t: &str, w1: &str) -> Option<bool> {
    if pfx(w1, "att", 2) {
        iface_read_att(session, &read_file_arg(t, 2));
        return Some(true);
    }
    if pfx(w1, "prolog", 4) {
        iface_read_prolog(session, &read_file_arg(t, 2));
        return Some(true);
    }
    if w1 == "spaced-text" {
        iface_read_spaced_text(session, &read_file_arg(t, 2));
        return Some(true);
    }
    if w1 == "text" {
        iface_read_text(session, &read_file_arg(t, 2));
        return Some(true);
    }
    if pfx(w1, "regex", 3) {
        return Some(start_regex(session, RE, String::new(), &arg_after(t, 2)));
    }
    if pfx(w1, "cmatrix", 3) {
        // DEVIATION from C: my_cmatrixparse is not yet ported; not wired.
        eprintln!("read cmatrix: not supported in this build");
        return Some(true);
    }
    if w1 == "lexc" {
        /* interface.l RLEXC action: file_to_mem then
        fsm_lexc_parse_string(&session.opts, buf, 1), result pushed via stack_add */
        let fname = read_file_arg(t, 2);
        if let Some(net) =
            foma::lexcread::fsm_lexc_parse_file(&session.opts, Some(&mut session.defines), &fname)
        {
            session.stack_add(net);
        }
        return Some(true);
    }
    None
}

fn write_att_cmd(session: &mut Session, arg: &str) -> bool {
    let arg = strip_redir(arg);
    if arg.is_empty() {
        iface_write_att(session, None);
    } else {
        iface_write_att(session, Some(arg.as_str()));
    }
    true
}

fn write_prolog_cmd(session: &mut Session, arg: &str) -> bool {
    let arg = strip_redir(arg);
    if arg.is_empty() {
        iface_write_prolog(session, None);
    } else {
        iface_write_prolog(session, Some(arg.as_str()));
    }
    true
}

fn try_test(session: &mut Session, w0: &str, w1: &str) -> Option<bool> {
    // Standalone abbreviations.
    let abbr: Option<fn(&mut Session)> = match w0 {
        "tunam" => Some(iface_test_unambiguous),
        "equ" => Some(iface_test_equivalent),
        "tfu" => Some(iface_test_functional),
        "tid" => Some(iface_test_identity),
        "tnn" => Some(iface_test_nonnull),
        "tnu" => Some(iface_test_null),
        "tlu" => Some(iface_test_lower_universal),
        "tuu" => Some(iface_test_upper_universal),
        "tseq" => Some(iface_test_sequential),
        _ => None,
    };
    if let Some(f) = abbr {
        f(session);
        return Some(true);
    }
    if w0 == "test" {
        let f: Option<fn(&mut Session)> = match w1 {
            "unambiguous" => Some(iface_test_unambiguous),
            "equivalent" => Some(iface_test_equivalent),
            "functional" => Some(iface_test_functional),
            "identity" => Some(iface_test_identity),
            "non-null" => Some(iface_test_nonnull),
            "null" => Some(iface_test_null),
            "lower-universal" => Some(iface_test_lower_universal),
            "upper-universal" => Some(iface_test_upper_universal),
            "sequential" => Some(iface_test_sequential),
            _ => None,
        };
        if let Some(f) = f {
            f(session);
            return Some(true);
        }
    }
    None
}

fn try_print_family(session: &mut Session, t: &str, ws: &[&str]) -> Option<bool> {
    // Strip an optional leading "print"/"pr"/"pri"/"prin" prefix; the remainder
    // (`sub_owned`) is matched against the print sub-commands, which also stand
    // alone as short forms ("net", "sigma", "words", "pss", …).
    let had_print = ws.len() >= 2 && pfx(ws[0], "print", 2);
    let sub_owned = if had_print {
        arg_after(t, 1)
    } else {
        t.to_string()
    };
    let subws: Vec<&str> = sub_owned.split_whitespace().collect();
    if subws.is_empty() {
        return None;
    }
    let s0 = subws[0];
    let gt = sub_owned.find('>');

    if pfx(s0, "cmatrix", 3) {
        iface_print_cmatrix(session);
        return Some(true);
    }
    if pfx(s0, "defined", 3) {
        iface_print_defined(session);
        return Some(true);
    }
    if s0 == "dot" {
        if let Some(g) = gt {
            iface_print_dot(session, Some(sub_owned[g + 1..].trim()));
        } else if subws.len() >= 2 {
            // "dot NAME": interface.l has no action (no-op).
        } else {
            iface_print_dot(session, None);
        }
        return Some(true);
    }
    if pfx(s0, "name", 2) {
        iface_print_name(session);
        return Some(true);
    }
    if s0 == "net" {
        if let Some(g) = gt {
            let before = sub_owned[..g].trim();
            let after = sub_owned[g + 1..].trim();
            let name = arg_after(before, 1);
            if name.is_empty() {
                iface_print_net(session, None, Some(after));
            } else {
                iface_print_net(session, Some(&name), Some(after));
            }
        } else if subws.len() >= 2 {
            let name = arg_after(&sub_owned, 1);
            iface_print_net(session, Some(&name), None);
        } else {
            iface_print_net(session, None, None);
        }
        return Some(true);
    }
    if s0 == "stack-size" {
        println!("STACK SIZE: {}", session.stack_size());
        return Some(true);
    }
    if pfx_hyphen(s0, "lower-words", 3) {
        if let Some(g) = gt {
            iface_words_file(session, sub_owned[g + 1..].trim(), 2);
        } else {
            iface_lower_words(session, num_arg(&subws, t));
        }
        return Some(true);
    }
    if pfx_hyphen(s0, "upper-words", 3) {
        if let Some(g) = gt {
            iface_words_file(session, sub_owned[g + 1..].trim(), 1);
        } else {
            iface_upper_words(session, num_arg(&subws, t));
        }
        return Some(true);
    }
    if s0 == "words" {
        if let Some(g) = gt {
            iface_words_file(session, sub_owned[g + 1..].trim(), 0);
        } else {
            iface_words(session, num_arg(&subws, t));
        }
        return Some(true);
    }
    if s0 == "pairs" {
        if let Some(g) = gt {
            iface_pairs_file(session, sub_owned[g + 1..].trim());
        } else {
            iface_pairs(session, -1);
        }
        return Some(true);
    }
    if s0 == "random-lower" {
        iface_random_lower(session, num_arg(&subws, t));
        return Some(true);
    }
    if s0 == "random-upper" {
        iface_random_upper(session, num_arg(&subws, t));
        return Some(true);
    }
    if s0 == "random-words" {
        iface_random_words(session, num_arg(&subws, t));
        return Some(true);
    }
    if s0 == "random-pairs" {
        iface_random_pairs(session, -1);
        return Some(true);
    }
    if pfx(s0, "sigma", 3) {
        iface_print_sigma(session);
        return Some(true);
    }
    if pfx(s0, "size", 3) {
        iface_print_stats(session);
        return Some(true);
    }
    if s0 == "shortest-string" || s0 == "pss" {
        iface_print_shortest_string(session);
        return Some(true);
    }
    if s0 == "shortest-string-size" || s0 == "psz" {
        iface_print_shortest_string_size(session);
        return Some(true);
    }
    None
}

fn try_zero_arg(session: &mut Session, w0: &str, w1: &str) -> Option<bool> {
    // ambiguous [upper]
    if w0 == "ambiguous" {
        iface_ambiguous_upper(session);
        return Some(true);
    }
    if w0 == "clear" {
        session.stack_clear();
        return Some(true);
    }
    if w0 == "close" {
        iface_close(session);
        return Some(true);
    }
    // compact sigma / complete / compose / concatenate
    if pfx(w0, "compact", 4) && pfx(w1, "sigma", 3) {
        iface_compact(session);
        return Some(true);
    }
    if pfx(w0, "complete", 5) {
        iface_complete(session);
        return Some(true);
    }
    if pfx(w0, "compose", 5) {
        iface_compose(session);
        return Some(true);
    }
    if pfx(w0, "concatenate", 4) {
        iface_conc(session);
        return Some(true);
    }
    if pfx(w0, "crossproduct", 5) {
        iface_crossproduct(session);
        return Some(true);
    }
    if pfx(w0, "determinize", 3) {
        iface_determinize(session);
        return Some(true);
    }
    if w0 == "examb" {
        iface_extract_ambiguous(session);
        return Some(true);
    }
    if w0 == "exunamb" {
        iface_extract_unambiguous(session);
        return Some(true);
    }
    if w0 == "extract" {
        if w1 == "ambiguous" {
            iface_extract_ambiguous(session);
            return Some(true);
        }
        if w1 == "unambiguous" {
            iface_extract_unambiguous(session);
            return Some(true);
        }
    }
    if w0 == "fac" || pfx(w0, "factorize", 3) {
        iface_factorize(session);
        return Some(true);
    }
    if w0 == "seq" || pfx(w0, "sequentialize", 3) {
        iface_sequentialize(session);
        return Some(true);
    }
    if pfx(w0, "ignore", 4) {
        iface_ignore(session);
        return Some(true);
    }
    if pfx(w0, "intersect", 5) {
        iface_intersect(session);
        return Some(true);
    }
    if pfx(w0, "invert", 3) {
        iface_invert(session);
        return Some(true);
    }
    if w0 == "lower-side" {
        iface_lower_side(session);
        return Some(true);
    }
    if w0 == "upper-side" {
        iface_upper_side(session);
        return Some(true);
    }
    if pfx(w0, "minimize", 3) {
        iface_minimize(session);
        return Some(true);
    }
    if pfx(w0, "negate", 3) {
        iface_negate(session);
        return Some(true);
    }
    if pfx(w0, "one-plus", 2) {
        iface_one_plus(session);
        return Some(true);
    }
    if pfx(w0, "zero-plus", 2) {
        iface_zero_plus(session);
        return Some(true);
    }
    if pfx(w0, "pop", 2) {
        iface_pop(session);
        return Some(true);
    }
    if pfx(w0, "prune", 3) {
        iface_prune(session);
        return Some(true);
    }
    if w0 == "rev" || pfx(w0, "reverse", 3) {
        iface_reverse(session);
        return Some(true);
    }
    if pfx(w0, "rotate", 3) {
        iface_rotate(session);
        return Some(true);
    }
    if pfx(w0, "shuffle", 3) {
        iface_shuffle(session);
        return Some(true);
    }
    // sort in / sort out / sort [net]
    if pfx(w0, "sort", 2) {
        if pfx(w1, "input", 2) {
            iface_sort_input(session);
        } else if pfx(w1, "output", 3) {
            iface_sort_output(session);
        } else {
            iface_sort(session);
        }
        return Some(true);
    }
    if pfx(w0, "turn", 2) {
        iface_turn(session);
        return Some(true);
    }
    if w0 == "tfd" || (w0 == "twosided" && pfx(w1, "flag-diacritics", 4)) {
        iface_twosided_flags(session);
        return Some(true);
    }
    if pfx(w0, "union", 3) {
        iface_union(session);
        return Some(true);
    }
    if pfx(w0, "view", 4) {
        iface_view(session);
        return Some(true);
    }
    None
}

// interface.l PUSH action wrapped as a helper (find_defined + copy + stack_add).
fn iface_push(session: &mut Session, name: &str) {
    let copy = match find_defined(&mut session.defines, name) {
        None => {
            println!("'{}' is not a defined symbol.", name);
            return;
        }
        Some(net) => fsm_copy(net),
    };
    session.stack_add(copy);
}

// ───────────────────────── small helpers ─────────────────────────

fn lstrip(s: &str) -> &str {
    s.trim_start_matches([' ', '\t', '\r'])
}

/// Skip `n` whitespace-delimited tokens of `t`, returning the trimmed remainder.
fn arg_after(t: &str, n: usize) -> String {
    let mut s = t.trim_start_matches([' ', '\t']);
    for _ in 0..n {
        match s.find([' ', '\t']) {
            Some(i) => s = s[i..].trim_start_matches([' ', '\t']),
            None => {
                s = "";
                break;
            }
        }
    }
    s.trim().to_string()
}

/// A file argument: like arg_after, but also drop a leading redirection '<'.
fn read_file_arg(t: &str, n: usize) -> String {
    arg_after(t, n)
        .trim_start_matches(['<', ' ', '\t'])
        .trim()
        .to_string()
}

/// Strip a leading '>' redirection marker (write/save/export use '>? filename').
fn strip_redir(s: &str) -> String {
    s.trim_start_matches(['>', ' ', '\t']).trim().to_string()
}

/// A numbered variant: `<cmd> N` → iface_extract_number over the whole line.
fn num_arg(subws: &[&str], t: &str) -> i32 {
    if subws.len() >= 2 && !subws[1].is_empty() && subws[1].bytes().all(|b| b.is_ascii_digit()) {
        iface_extract_number(t)
    } else {
        -1
    }
}

/// `tok` is a prefix of `full` of length >= `min` (foma's abbreviation rule).
fn pfx(tok: &str, full: &str, min: usize) -> bool {
    tok.len() >= min && tok.len() <= full.len() && full.as_bytes().starts_with(tok.as_bytes())
}

/// Same as pfx; named separately at hyphenated command sites for readability.
fn pfx_hyphen(tok: &str, full: &str, min: usize) -> bool {
    pfx(tok, full, min)
}

// ────────────────────────────────────────────────────────────────────────────
// Wave-3 unit tests. The readline-completion stand-ins (my_completion /
// my_generator) and the no-op hooks (xprintf / add_history) are inert at
// runtime (readline is not linked), so they have no observable effect via the
// spawned CLI. Their sem-rule behavior is pinned here directly. thread_local
// completion cursors are isolated per test thread by the harness.
// ────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // my_generator ignores its `text` argument and matches the WHOLE line
    // (rl_line_buffer, primed by my_completion) as a prefix against cmd[], then —
    // when rl_point > 0 — against abbrvcmd[], returning strdup(name + smatch) for
    // each hit and NULL once both tables are exhausted. my_completion stores
    // start→smatch / end→rl_point and drives the generator (state 0 then 1).
    // [spec:foma:sem:foma.my-generator-fn/test]
    // [spec:foma:sem:foma.my-completion-fn/test]
    #[test]
    fn my_generator_prefix_matching() {
        // "apply " is a prefix of the three "apply …" commands; smatch = 6 drops
        // the shared "apply " so only the completion tail is returned (readline
        // substitutes only the word starting at column smatch).
        assert_eq!(
            my_completion("apply ", 6, 6),
            vec!["down".to_string(), "med".to_string(), "up".to_string()]
        );

        // smatch = 0 returns the full command names (whole-line prefix match).
        assert_eq!(
            my_completion("apply", 0, 5),
            vec![
                "apply down".to_string(),
                "apply med".to_string(),
                "apply up".to_string()
            ]
        );

        // rl_point > 0 makes it also scan the abbreviation table: "do" is not a
        // prefix of any cmd[] entry but matches abbrvcmd[]'s "down".
        assert_eq!(my_completion("do", 0, 2), vec!["down".to_string()]);

        // No prefix hit in either table → empty (my_generator returns None at once).
        assert!(my_completion("zzzz", 0, 4).is_empty());
    }

    // On state == 0 my_generator resets its static cursors and caches len; it then
    // yields each cmd[] prefix hit and finally None. With rl_point == 0 the
    // abbrvcmd[] scan is skipped, so "quit" yields exactly one candidate.
    // [spec:foma:sem:foma.my-generator-fn/test]
    #[test]
    fn my_generator_state_reset_and_exhaustion() {
        SMATCH.with(|s| s.set(0));
        RL_LINE_BUFFER.with(|b| *b.borrow_mut() = "quit".to_string());
        RL_POINT.with(|p| p.set(0));
        // First call (state 0) resets the cursors and returns the first hit.
        assert_eq!(my_generator("quit", 0), Some("quit".to_string()));
        // Continuations (state 1) run to exhaustion and then return None.
        let mut last = my_generator("quit", 1);
        while last.is_some() {
            last = my_generator("quit", 1);
        }
        assert_eq!(last, None);
    }

    // xprintf: C body is `{ return; printf("%s", string); }` — the printf is
    // unreachable, so the function is a no-op that discards its argument.
    // [spec:foma:sem:foma.xprintf-fn/test]
    // [spec:foma:sem:fomalibconf.xprintf-fn/test]
    #[test]
    fn xprintf_is_a_noop() {
        // Returns unit with no observable effect; simply calling it must succeed.
        xprintf("this text is discarded");
    }

    // add_history: readline is not linked, so this stand-in is a no-op returning 0
    // (the C extern's int result is ignored by foma in any case).
    // [spec:foma:sem:foma.add-history-fn/test]
    #[test]
    fn add_history_is_a_noop_returning_zero() {
        assert_eq!(add_history("some line"), 0);
    }
}
