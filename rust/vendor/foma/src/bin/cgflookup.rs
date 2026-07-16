//! foma/cgflookup.c — literal (bug-for-bug) Wave-2 port.
//!
//! Like flookup but stdin-only (no UDP server, no Windows socket setup) and
//! with constraint-grammar (CG) cohort output. See
//! docs/spec/port/foma/cgflookup.md.
//!
//! File-static mutable globals become module-level thread_locals keeping the C
//! names (upper-cased); lookup_chain pointer walks become Vec index walks.

use std::cell::{Cell, RefCell};
use std::io::{self, BufRead, BufReader, BufWriter, Stdin, Stdout, Write};

use foma::apply::{apply_clear, apply_down, apply_index, apply_init, apply_up};
use foma::io::{
    fsm_read_binary_file_multiple, fsm_read_binary_file_multiple_init, parse_leading_i32,
};
use foma::structures::{fsm_destroy, fsm_get_library_version_string, fsm_sort_arcs};
use foma::types::{APPLY_INDEX_INPUT, APPLY_INDEX_OUTPUT, ApplyHandle, Fsm};

const DIR_DOWN: i32 = 0;
const DIR_UP: i32 = 1;

const EXIT_FAILURE: i32 = 1;

static USAGESTRING: &str = "Usage: cgflookup [-h] [-a] [-i] [-s \"separator\"] [-w \"wordseparator\"] [-v] [-x] [-b] [-I <#|#k|#m|f>] <binary foma file>\n";

static HELPSTRING: &str = concat!(
    "Applies words from stdin to a foma transducer/automaton read from a file and prints results to stdout.\n",
    "If the file contains several nets, inputs will be passed through all of them (simulating composition) or applied as alternates if the -a flag is specified (simulating priority union: the first net is tried first, if that fails to produce an output, then the second is tried, etc.).\n\n",
    "Options:\n\n",
    "-h\t\tprint help\n",
    "-a\t\ttry alternatives (in order of nets loaded, default is to pass words through each)\n",
    "-b\t\tunbuffered output (flushes output after each input word, for use in bidirectional piping)\n",
    "-i\t\tinverse application (apply down instead of up)\n",
    "-I indextype\tindex arcs with indextype (one of -I f -I #k -I #m or -I #)\n",
    "\t\t(usually slower than the default except for states > 1,000 arcs)\n",
    "\t\t  -I # will index all states containing # arcs or more\n",
    "\t\t  -I NUMk will index states from densest to sparsest until reaching mem limit of # kB\n",
    "\t\t  -I NUMM will index states from densest to sparsest until reaching mem limit of # MB\n",
    "\t\t  -I f will index flag-containing states only\n",
    "-q\t\tdon't sort arcs before applying (usually slower, except for really small, sparse automata)\n",
    "-s \"separator\"\tchange input/output separator symbol (default is TAB)\n",
    "-u \"separator\"\tmark uppercase words with <*>\n",
    "-w \"separator\"\tchange words separator symbol (default is LF)\n",
    "-v\t\tprint version number\n",
);

// [spec:foma:def:cgflookup.lookup-chain]
// struct lookup_chain { net; ah; next; prev; } — owned net/ah here; the
// `next`/`prev` pointer fields become Vec indices (None <-> NULL).
struct LookupChain {
    net: Option<Box<Fsm>>,
    ah: Option<Box<ApplyHandle>>,
    next: Option<usize>,
    prev: Option<usize>,
}

type Applyer = fn(&mut ApplyHandle, Option<&str>) -> Option<String>;

thread_local! {
    // static char buffer[2048]; stdout full-buffering region (setvbuf _IOFBF).
    static OUT: RefCell<BufWriter<Stdout>> =
        RefCell::new(BufWriter::with_capacity(2048, io::stdout()));
    static INFILE: RefCell<BufReader<Stdin>> = RefCell::new(BufReader::new(io::stdin()));

    static APPLY_ALTERNATES: Cell<i32> = const { Cell::new(0) };
    static RESULTS: Cell<i32> = const { Cell::new(0) };
    static MARK_UPPERCASE: Cell<i32> = const { Cell::new(0) };

    static SEPARATOR: RefCell<String> = RefCell::new("\t".to_string());
    // wordseparator default "" — empty, unlike flookup's "\n".
    static WORDSEPARATOR: RefCell<String> = const { RefCell::new(String::new()) };
    static LINE: RefCell<String> = const { RefCell::new(String::new()) };
    static INDENT: RefCell<String> = RefCell::new("\t".to_string());

    // static char *(*applyer)(...) = &apply_up;
    static APPLYER: Cell<Applyer> = Cell::new(apply_up as Applyer);

    static CHAIN: RefCell<Vec<LookupChain>> = const { RefCell::new(Vec::new()) };
    static CHAIN_HEAD: Cell<Option<usize>> = const { Cell::new(None) };
    static CHAIN_TAIL: Cell<Option<usize>> = const { Cell::new(None) };
}

fn out_str(s: &str) {
    OUT.with_borrow_mut(|o| {
        o.write_all(s.as_bytes())
            .expect("write lookup output to stdout");
    });
}
fn out_flush() {
    OUT.with_borrow_mut(|o| {
        o.flush().expect("flush lookup output");
    });
}
fn finish(code: i32) -> ! {
    out_flush();
    std::process::exit(code);
}
fn perror(prefix: &str) {
    eprintln!("{}: {}", prefix, io::Error::last_os_error());
}

/* Minimal getopt(3) twin: clustered flags, attached/separate option
arguments, "--" terminator, stop at first non-option operand. */
struct GetOpt {
    args: Vec<String>,
    optind: usize,
    subpos: usize,
    optarg: Option<String>,
}
impl GetOpt {
    fn new(args: Vec<String>) -> Self {
        GetOpt {
            args,
            optind: 1,
            subpos: 0,
            optarg: None,
        }
    }
    fn next(&mut self, argtakers: &str) -> Option<u8> {
        self.optarg = None;
        if self.subpos == 0 {
            if self.optind >= self.args.len() {
                return None;
            }
            let cur = &self.args[self.optind];
            let b = cur.as_bytes();
            if b.len() < 2 || b[0] != b'-' {
                return None;
            }
            if cur == "--" {
                self.optind += 1;
                return None;
            }
            self.subpos = 1;
        }
        let cur = self.args[self.optind].clone();
        let b = cur.as_bytes();
        let ch = b[self.subpos];
        self.subpos += 1;
        if argtakers.as_bytes().contains(&ch) {
            if self.subpos < b.len() {
                self.optarg = Some(cur[self.subpos..].to_string());
                self.optind += 1;
                self.subpos = 0;
            } else {
                self.optind += 1;
                self.subpos = 0;
                if self.optind < self.args.len() {
                    self.optarg = Some(self.args[self.optind].clone());
                    self.optind += 1;
                } else {
                    return Some(b'?');
                }
            }
            return Some(ch);
        }
        if self.subpos >= b.len() {
            self.optind += 1;
            self.subpos = 0;
        }
        Some(ch)
    }
}

// [spec:foma:def:cgflookup.applyer-fn]
// [spec:foma:sem:cgflookup.applyer-fn]
// (the file-static function pointer lives in the APPLYER thread_local above,
// defaulting to apply_up; -i repoints it to apply_down)

// [spec:foma:def:cgflookup.app-print-fn]
// [spec:foma:sem:cgflookup.app-print-fn]
fn app_print(result: Option<&str>) {
    match result {
        None => {
            // A word with no analyses: bare cohort header only.
            let line = LINE.with_borrow(|l| l.clone());
            out_str(&format!("\"<{}>\"\n", line));
        }
        Some(r) => {
            let indent = INDENT.with_borrow(|i| i.clone());
            if MARK_UPPERCASE.get() != 0 {
                // C: mbstowcs(testuc, line, 1); iswupper(*testuc).
                // DEVIATION from C (mbstowcs/iswupper are locale-dependent; here
                // the first char is tested with Unicode char::is_uppercase; empty
                // line — where C reads an uninitialized wchar — is treated as
                // not uppercase).
                let upper = LINE
                    .with_borrow(|l| l.chars().next())
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false);
                if upper {
                    out_str(&format!("{}{} <*>\n", indent, r));
                } else {
                    out_str(&format!("{}{}\n", indent, r));
                }
            } else {
                out_str(&format!("{}{}\n", indent, r));
            }
        }
    }
}

// [spec:foma:def:cgflookup.main-fn]
// [spec:foma:sem:cgflookup.main-fn+1]
fn main() {
    // Route library diagnostics (tracing events) to stderr in a compact,
    // CLI-friendly form (LEVEL message, no timestamp/target).
    let _ = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .without_time()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let mut sortarcs = 1i32;
    let mut direction = DIR_UP;
    let mut index_arcs = 0i32;
    let mut index_flag_states = 0i32;
    let mut index_cutoff = 0i32;
    let mut index_mem_limit = i32::MAX;
    let mut buffered_output = 1i32;
    let mut numnets = 0i32;

    // setvbuf(stdout, buffer, _IOFBF, 2048) — modeled by the OUT BufWriter.

    let mut go = GetOpt::new(std::env::args().collect());
    // optstring "abhHiI:qs:uw:vx"; arg-taking letters: I s w
    while let Some(opt) = go.next("Isw") {
        match opt {
            b'a' => {
                APPLY_ALTERNATES.set(1);
            }
            b'b' => {
                buffered_output = 0;
            }
            b'h' => {
                out_str(USAGESTRING);
                out_str(HELPSTRING);
                out_str("\n");
                finish(0);
            }
            b'i' => {
                direction = DIR_DOWN;
                APPLYER.set(apply_down as Applyer);
            }
            b'q' => {
                sortarcs = 0;
            }
            b'I' => {
                let optarg = go.optarg.clone().unwrap_or_default();
                if optarg == "f" {
                    index_flag_states = 1;
                    index_arcs = 1;
                } else if optarg.contains('k') || optarg.contains('K') {
                    /* k limit: "-I 4k" / "-I 4K" → 4 * 1024 bytes */
                    index_mem_limit = 1024 * parse_leading_i32(&optarg);
                    index_arcs = 1;
                } else if optarg.contains('m') || optarg.contains('M') {
                    /* m limit: "-I 4m" / "-I 4M" → 4 * 1024 * 1024 bytes */
                    index_mem_limit = 1024 * 1024 * parse_leading_i32(&optarg);
                    index_arcs = 1;
                } else if optarg.starts_with(|c: char| c.is_ascii_digit()) {
                    // Plain "-I 4" is an arc-count cutoff. (C required BOTH letter
                    // cases in the arg for the k/m branches, so "-I 4k"/"-I 4M"
                    // fell through here — the k/m suffix was silently ignored.)
                    index_arcs = 1;
                    index_cutoff = parse_leading_i32(&optarg);
                }
            }
            b's' => {
                let optarg = go.optarg.clone().unwrap_or_default();
                // separator is never actually used — cgflookup has no echo mode.
                SEPARATOR.with_borrow_mut(|s| *s = optarg);
            }
            b'u' => {
                MARK_UPPERCASE.set(1);
                // setlocale(LC_CTYPE, "").
                // DEVIATION from C (std has no setlocale; treated as always
                // succeeding — the "can't set locale" branch is unreachable —
                // and uppercase testing uses Unicode, see app_print).
            }
            b'w' => {
                let optarg = go.optarg.clone().unwrap_or_default();
                WORDSEPARATOR.with_borrow_mut(|s| *s = optarg);
            }
            b'v' => {
                out_str(&format!(
                    "cgflookup 1.03 (foma library version {})\n",
                    fsm_get_library_version_string()
                ));
                finish(0);
            }
            // -x (advertised in the usage text) disables echoing of the input
            // word. cgflookup never echoes, so it is accepted as a no-op rather
            // than erroring out as C did (it was in the optstring but had no case).
            b'x' => {}
            // 'H' is in the optstring but has no case, so — like any unknown
            // option — it prints usage to stderr and exit(EXIT_FAILURE).
            _ => {
                eprint!("{}", USAGESTRING);
                finish(EXIT_FAILURE);
            }
        }
    }
    if go.optind == go.args.len() {
        eprint!("{}", USAGESTRING);
        finish(EXIT_FAILURE);
    }

    let infilename = go.args[go.optind].clone();

    let mut fsrh = match fsm_read_binary_file_multiple_init(&infilename) {
        Some(h) => Some(h),
        None => {
            perror("File error");
            finish(EXIT_FAILURE);
        }
    };
    CHAIN_HEAD.set(None);
    CHAIN_TAIL.set(None);

    while let Some(mut net) = fsm_read_binary_file_multiple(&mut fsrh) {
        numnets += 1;
        if direction == DIR_DOWN && net.arcs_sorted_in != 1 && sortarcs != 0 {
            fsm_sort_arcs(&mut net, 1);
        }
        if direction == DIR_UP && net.arcs_sorted_out != 1 && sortarcs != 0 {
            fsm_sort_arcs(&mut net, 2);
        }
        let mut ah = apply_init(&net);
        if direction == DIR_DOWN && index_arcs != 0 {
            apply_index(
                &mut ah,
                APPLY_INDEX_INPUT,
                index_cutoff,
                index_mem_limit,
                index_flag_states,
            );
        }
        if direction == DIR_UP && index_arcs != 0 {
            apply_index(
                &mut ah,
                APPLY_INDEX_OUTPUT,
                index_cutoff,
                index_mem_limit,
                index_flag_states,
            );
        }

        let idx = CHAIN.with_borrow_mut(|c| {
            c.push(LookupChain {
                net: Some(net),
                ah: Some(ah),
                next: None,
                prev: None,
            });
            c.len() - 1
        });
        if CHAIN_TAIL.get().is_none() {
            CHAIN_TAIL.set(Some(idx));
            CHAIN_HEAD.set(Some(idx));
        } else if direction == DIR_DOWN || APPLY_ALTERNATES.get() == 1 {
            let t = CHAIN_TAIL
                .get()
                .expect("chain tail set once the chain is non-empty");
            CHAIN.with_borrow_mut(|c| {
                c[t].next = Some(idx);
                c[idx].prev = Some(t);
            });
            CHAIN_TAIL.set(Some(idx));
        } else {
            // Default up direction: prepend at head (up-mode runs nets in
            // reverse file order).
            let h = CHAIN_HEAD
                .get()
                .expect("chain head set once the chain is non-empty");
            CHAIN.with_borrow_mut(|c| {
                c[idx].next = Some(h);
                c[h].prev = Some(idx);
            });
            CHAIN_HEAD.set(Some(idx));
        }
    }

    if numnets < 1 {
        eprintln!("File error: {}", infilename);
        finish(EXIT_FAILURE);
    }

    /* Standard read from stdin */
    LINE.with_borrow_mut(|l| *l = String::new());
    while get_next_line() {
        RESULTS.set(0);
        let line = LINE.with_borrow(|l| l.clone());
        handle_line(&line);
        if RESULTS.get() == 0 {
            app_print(None);
        }
        let wsep = WORDSEPARATOR.with_borrow(|w| w.clone());
        out_str(&wsep);
        if buffered_output == 0 {
            out_flush();
        }
    }

    /* Cleanup */
    let mut chain_pos = CHAIN_HEAD.get();
    while let Some(p) = chain_pos {
        let next = CHAIN.with_borrow(|c| c[p].next);
        CHAIN_HEAD.set(next);
        let (ah, net) = CHAIN.with_borrow_mut(|c| (c[p].ah.take(), c[p].net.take()));
        if let Some(ah) = ah {
            apply_clear(ah);
        }
        if let Some(net) = net {
            fsm_destroy(net);
        }
        chain_pos = CHAIN_HEAD.get();
    }
    finish(0);
}

// [spec:foma:def:cgflookup.get-next-line-fn]
// [spec:foma:sem:cgflookup.get-next-line-fn]
fn get_next_line() -> bool {
    // fgets(line, LINE_LIMIT, INFILE); truncate at first '\n'/'\r'.
    // DEVIATION from C (reads a full logical line, not the LINE_LIMIT-1 fgets
    // chunk split; bytes → String via from_utf8_lossy).
    let mut raw: Vec<u8> = Vec::new();
    let n = INFILE.with_borrow_mut(|r| r.read_until(b'\n', &mut raw).unwrap_or(0));
    if n == 0 {
        return false;
    }
    let cut = raw
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .unwrap_or(raw.len());
    raw.truncate(cut);
    let s = String::from_utf8_lossy(&raw).into_owned();
    LINE.with_borrow_mut(|l| *l = s);
    true
}

fn apply_at(p: usize, word: Option<&str>) -> Option<String> {
    let f = APPLYER.get();
    CHAIN.with_borrow_mut(|c| {
        f(
            c[p].ah
                .as_deref_mut()
                .expect("chain node carries an apply handle"),
            word,
        )
    })
}

/* CG cohort header printed immediately before a word's first reading. */
fn print_cohort_header() {
    let line = LINE.with_borrow(|l| l.clone());
    out_str(&format!("\"<{}>\"\n", line));
}

// [spec:foma:def:cgflookup.handle-line-fn]
// [spec:foma:sem:cgflookup.handle-line-fn]
fn handle_line(s: &str) {
    /* Apply alternative */
    RESULTS.set(0);
    if APPLY_ALTERNATES.get() == 1 {
        let mut chain_pos = CHAIN_HEAD.get();
        let tempstr = s.to_string();
        loop {
            let p = chain_pos.expect("chain has at least one node (numnets>=1)");
            let result = apply_at(p, Some(&tempstr));
            if let Some(r) = result {
                RESULTS.set(RESULTS.get() + 1);
                if RESULTS.get() == 1 {
                    print_cohort_header();
                }
                app_print(Some(&r));
                while let Some(r2) = apply_at(p, None) {
                    RESULTS.set(RESULTS.get() + 1);
                    app_print(Some(&r2));
                }
                break;
            }
            if chain_pos == CHAIN_TAIL.get() {
                break;
            }
            chain_pos = CHAIN.with_borrow(|c| c[p].next);
        }
    } else {
        /* Get result from chain (cascade via depth-first search) */
        let mut chain_pos = CHAIN_HEAD.get();
        let mut tempstr = s.to_string();
        loop {
            let p = chain_pos.expect("chain has at least one node (numnets>=1)");
            let mut result = apply_at(p, Some(&tempstr));
            let is_tail = chain_pos == CHAIN_TAIL.get();
            if result.is_some() && !is_tail {
                tempstr = result.take().expect("result is Some in this branch");
                chain_pos = CHAIN.with_borrow(|c| c[p].next);
                continue;
            }
            if result.is_some() && is_tail {
                loop {
                    RESULTS.set(RESULTS.get() + 1);
                    if RESULTS.get() == 1 {
                        print_cohort_header();
                    }
                    app_print(result.as_deref());
                    result = apply_at(p, None);
                    if result.is_none() {
                        break;
                    }
                }
            }
            if result.is_none() {
                /* Move up (backtrack) */
                let mut bp = CHAIN.with_borrow(|c| c[p].prev);
                loop {
                    match bp {
                        None => break,
                        Some(bpi) => {
                            let r = apply_at(bpi, None);
                            if let Some(r) = r {
                                tempstr = r;
                                break;
                            }
                            bp = CHAIN.with_borrow(|c| c[bpi].prev);
                        }
                    }
                }
                chain_pos = bp;
            }
            if chain_pos.is_none() {
                break;
            }
            chain_pos =
                CHAIN.with_borrow(|c| c[chain_pos.expect("chain_pos checked non-None above")].next);
        }
    }
}
