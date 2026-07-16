//! foma/io.c — literal (bug-for-bug) Wave-2 port per
//! docs/port/rust-conventions.md. Sem rules: docs/spec/port/foma/io.md
//! (per-file `io.*` ids) plus the fomalib.h prototype ids for the exported
//! readers/writers.
//!
//! The gzip layer that C reaches through zlib's gzopen/gzread/gzprintf is
//! provided by flate2. zlib's gzopen transparently reads *uncompressed* files
//! too (gzdirect), whereas flate2's GzDecoder errors on non-gzip input, so the
//! readers sniff the 1f 8b magic and fall back to a plain read to reproduce the
//! C behavior. Writers wrap the output file in a GzEncoder.
//!
//! In-memory buffer walking: C's io_buf_handle threads a `char *io_buf` and an
//! interior `char *io_buf_ptr` cursor; per the conventions the cursor becomes a
//! byte index into the buffer Vec. The spaced-text tokenizers likewise take a
//! `(buffer, cursor)` pair instead of the C `char **` (the buffer is implicit
//! in the C pointer).

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;

use crate::constructions::fsm_count;
use crate::define::add_defined;
use crate::dynarray::{
    fsm_construct_add_arc, fsm_construct_add_symbol, fsm_construct_check_symbol,
    fsm_construct_done, fsm_construct_init, fsm_construct_set_final, fsm_construct_set_initial,
};
use crate::error::FomaError;
use crate::options::FomaOptions;
use crate::sigma::{sigma_add_number, sigma_max, sigma_string, sigma_to_list};
use crate::structures::{fsm_create, fsm_destroy};
use crate::topsort::fsm_topsort;
use crate::trie::{
    fsm_trie_add_word, fsm_trie_done, fsm_trie_end_word, fsm_trie_init, fsm_trie_symbol,
};
use crate::types::{
    DefinedNetworks, Fsm, FsmConstructHandle, FsmReadBinaryHandle, FsmState, IDENTITY, UNKNOWN,
};
use smol_str::SmolStr;

/* C: #define READ_BUF_SIZE 4096 (the io_gets/io_net_read line buffer size).
Rust uses growable Strings for line buffers, so no fixed-size overrun. */
pub const READ_BUF_SIZE: usize = 4096;

/* ------------------------------------------------------------------ */
/* Types declared inside io.c                                          */
/* ------------------------------------------------------------------ */

// [spec:foma:def:io.binaryline]
// C: struct binaryline { int type; int state; int in; int target; int out;
// int symbol; char *name; char *value; } — declared in io.c but never read by
// any function; kept for id coverage.
#[derive(Debug, Clone)]
pub struct Binaryline {
    pub r#type: i32,
    pub state: i32,
    pub r#in: i32,
    pub target: i32,
    pub out: i32,
    pub symbol: i32,
    pub name: Option<SmolStr>,
    pub value: Option<SmolStr>,
}

// [spec:foma:def:io.io-buf-handle]
// C: struct io_buf_handle { char *io_buf; char *io_buf_ptr; }.
// io_buf is the whole (decompressed) file image; io_buf_ptr is an interior read
// cursor into it, represented as a byte index per the conventions.
#[derive(Debug)]
pub struct IoBufHandle {
    /// C: char *io_buf — None ↔ NULL (before io_gz_file_to_mem loads it).
    pub io_buf: Option<Vec<u8>>,
    /// C: char *io_buf_ptr — interior cursor into io_buf → byte index.
    pub io_buf_ptr: usize,
}

// [spec:foma:def:io.bom]
// C: typedef struct BOM { char code[4]; int len; char *name; } BOM;
pub(crate) struct Bom {
    code: [u8; 4],
    len: i32,
    name: Option<&'static str>,
}

/* Static-dispatch stdout-or-File writer. Replaces the boxed trait-object writer
used at the stdout-or-File selection sites so no trait-object dispatch remains;
both the stdout and file arms forward to their inner writer. */
pub(crate) enum Output {
    Stdout(std::io::Stdout),
    File(std::fs::File),
}

impl std::io::Write for Output {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Output::Stdout(w) => w.write(buf),
            Output::File(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Output::Stdout(w) => w.flush(),
            Output::File(w) => w.flush(),
        }
    }
}

/* ------------------------------------------------------------------ */
/* C library twins (no spec ids — these are libc, not io.c functions)  */
/* ------------------------------------------------------------------ */

/* C `strtol(s, &endptr, 10)`: skip leading whitespace, optional sign, base-10
digits, saturating to i64::MIN/MAX when the value is out of range (errno
ERANGE). glibc's atoi/atoll are strtol/strtoll with the reporting dropped, so
the parse_leading_* wrappers below share this walk. */
pub struct ParsedDecimal {
    pub value: i64,
    /* endptr == s in C: no digits were consumed */
    pub no_digits: bool,
    /* errno == ERANGE */
    pub out_of_range: bool,
}

pub fn parse_leading_decimal(s: &str) -> ParsedDecimal {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let mut neg = false;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        neg = bytes[i] == b'-';
        i += 1;
    }
    let mut any = false;
    let mut acc: i64 = 0;
    let mut range = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        any = true;
        let d = (bytes[i] - b'0') as i64;
        if !range {
            match acc.checked_mul(10).and_then(|v| v.checked_add(d)) {
                Some(v) => acc = v,
                None => range = true,
            }
        }
        i += 1;
    }
    let value = if range {
        if neg { i64::MIN } else { i64::MAX }
    } else if neg {
        -acc
    } else {
        acc
    };
    ParsedDecimal {
        value,
        no_digits: !any,
        out_of_range: range,
    }
}

/* C `atoll`: strtoll with endptr/ERANGE ignored. */
pub fn parse_leading_i64(s: &str) -> i64 {
    parse_leading_decimal(s).value
}

/* C `atoi`: like atoll truncated to int. */
pub fn parse_leading_i32(s: &str) -> i32 {
    parse_leading_i64(s) as i32
}

/* ------------------------------------------------------------------ */
/* Functions                                                           */
/* ------------------------------------------------------------------ */

// [spec:foma:def:io.escape-print-fn]
// [spec:foma:sem:io.escape-print-fn]
pub fn escape_print<W: std::io::Write + ?Sized>(
    stream: &mut W,
    string: &str,
) -> std::io::Result<()> {
    if string.contains('"') {
        /* strchr(string, '"') != NULL: byte-by-byte, emitting \" for each " */
        for &c in string.as_bytes() {
            if c == b'"' {
                stream.write_all(b"\\\"")?;
            } else {
                stream.write_all(&[c])?;
            }
        }
    } else {
        /* fprintf(stream, "%s", string) */
        stream.write_all(string.as_bytes())?;
    }
    Ok(())
}

// [spec:foma:def:io.foma-write-prolog-fn]
// [spec:foma:sem:io.foma-write-prolog-fn+1]
// [spec:foma:def:fomalib.foma-write-prolog-fn]
// [spec:foma:sem:fomalib.foma-write-prolog-fn]
pub fn foma_write_prolog(net: &mut Fsm, filename: Option<&str>) -> Result<(), FomaError> {
    /* C fell back to stdout (printing "Error writing to file … Using stdout.")
    when the target could not be created; here the caller is handed the error
    instead so it can report and abort. */
    let mut out: Output = match filename {
        None => Output::Stdout(std::io::stdout()),
        Some(fname) => match File::create(fname) {
            Ok(f) => Output::File(f),
            Err(e) => {
                return Err(FomaError::Io(format!(
                    "cannot write prolog to '{fname}': {e}"
                )));
            }
        },
    };
    fsm_count(net);
    let maxsigma = sigma_max(&net.sigma);
    /* calloc(maxsigma+1, sizeof(int)) */
    let mut used_symbols: Vec<i32> = vec![0; (maxsigma + 1) as usize];
    /* malloc(sizeof(int)*statecount) — indexed by state_no below.
    DEVIATION from C (state_no >= statecount OOB-writes in C; Rust panics) */
    let mut finals: Vec<i32> = vec![0; net.statecount as usize];
    /* identifier[100]; strcpy(identifier, net->name) — net->name is capped at
    40 bytes here (the raw over-read on a non-NUL-terminated 40-byte name is not
    reproduced) */
    let identifier = net.name.clone();

    /* Print identifier: fprintf(out, "%s%s%s", "network(", identifier, ").\n") */
    writeln!(out, "network({}).", identifier)?;

    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let state_no = net.states[i].state_no;
        let in_ = net.states[i].r#in;
        let out_ = net.states[i].out;
        let final_state = net.states[i].final_state;
        if final_state == 1 {
            finals[state_no as usize] = 1;
        } else {
            finals[state_no as usize] = 0;
        }
        if in_ != -1 {
            used_symbols[in_ as usize] = 1;
        }
        if out_ != -1 {
            used_symbols[out_ as usize] = 1;
        }
        i += 1;
    }

    for k in 3..=maxsigma {
        if used_symbols[k as usize] == 0 {
            /* C derefs sigma_string unconditionally (NULL for a numbering gap) */
            let mut instring =
                sigma_string(k, &net.sigma).expect("symbol number resolves in the net sigma");
            if instring == "0" {
                instring = "%0";
            }
            write!(out, "symbol({}, \"", identifier)?;
            escape_print(&mut out, instring)?;
            writeln!(out, "\").")?;
        }
    }

    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let state_no = net.states[i].state_no;
        let target = net.states[i].target;
        let in_ = net.states[i].r#in as i32;
        let out_ = net.states[i].out as i32;
        if target == -1 {
            i += 1;
            continue;
        }
        write!(out, "arc({}, {}, {}, ", identifier, state_no, target)?;
        let mut instring: &str = if in_ == 0 {
            "0"
        } else if in_ == 1 || in_ == 2 {
            "?"
        } else {
            sigma_string(in_, &net.sigma).expect("symbol number resolves in the net sigma")
        };
        let mut outstring: &str = if out_ == 0 {
            "0"
        } else if out_ == 1 || out_ == 2 {
            "?"
        } else {
            sigma_string(out_, &net.sigma).expect("symbol number resolves in the net sigma")
        };
        if instring == "0" && in_ != 0 {
            instring = "%0";
        }
        if outstring == "0" && out_ != 0 {
            outstring = "%0";
        }
        if instring == "?" && in_ > 2 {
            instring = "%?";
        }
        /* Wave 4 fix: the C out-side "?" escape tested stateptr->in > 2 (a copy
        typo); test out_ > 2 so a literal "?" out-symbol is escaped by its own
        symbol number, symmetrically with the in-side above. */
        if outstring == "?" && out_ > 2 {
            outstring = "%?";
        }

        if net.arity == 2 && in_ == IDENTITY && out_ == IDENTITY {
            writeln!(out, "\"?\").")?;
        } else if net.arity == 2 && in_ == out_ && in_ != UNKNOWN {
            write!(out, "\"")?;
            escape_print(&mut out, instring)?;
            writeln!(out, "\").")?;
        } else if net.arity == 2 {
            write!(out, "\"")?;
            escape_print(&mut out, instring)?;
            write!(out, "\":\"")?;
            escape_print(&mut out, outstring)?;
            writeln!(out, "\").")?;
        } else if net.arity == 1 {
            write!(out, "\"")?;
            escape_print(&mut out, instring)?;
            writeln!(out, "\").")?;
        }
        i += 1;
    }

    for k in 0..net.statecount {
        if finals[k as usize] != 0 {
            writeln!(out, "final({}, {}).", identifier, k)?;
        }
    }
    /* if (filename != NULL) fclose(out); — the File is dropped here either way;
    stdout is not closed. free(finals)/free(used_symbols) — dropped. */
    Ok(())
}

// [spec:foma:def:io.read-att-fn]
// [spec:foma:sem:io.read-att-fn]
// [spec:foma:def:fomalib.read-att-fn]
// [spec:foma:sem:fomalib.read-att-fn]
pub fn read_att(opts: &FomaOptions, filename: &str) -> Option<Box<Fsm>> {
    let infile = match File::open(filename) {
        Ok(f) => f,
        Err(_) => return None,
    };
    let mut reader = BufReader::new(infile);
    let mut h = fsm_construct_init(filename);
    let mut inword = String::new();
    /* fgets loop (the 1023-byte line split is not reproduced; read_line reads a
    whole line). read_line requires valid UTF-8 — a decode error is treated as
    EOF (DEVIATION: C reads raw bytes). */
    loop {
        inword.clear();
        match reader.read_line(&mut inword) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        /* strip a single trailing '\n' (a '\r' is left in place, as in C) */
        if inword.ends_with('\n') {
            inword.pop();
        }
        /* strtok on "\t": non-empty tokens only, capped at 6 */
        let tokens: Vec<&str> = inword
            .split('\t')
            .filter(|s| !s.is_empty())
            .take(6)
            .collect();
        let i = tokens.len();
        if i == 0 {
            continue;
        }
        if i >= 4 {
            let ge = opts.att_epsilon.clone();
            let t2 = if tokens[2] == ge.as_str() {
                "@_EPSILON_SYMBOL_@"
            } else {
                tokens[2]
            };
            let t3 = if tokens[3] == ge.as_str() {
                "@_EPSILON_SYMBOL_@"
            } else {
                tokens[3]
            };
            fsm_construct_add_arc(
                &mut h,
                parse_leading_i32(tokens[0]),
                parse_leading_i32(tokens[1]),
                t2,
                t3,
            );
        } else {
            /* i in 1..=3 */
            fsm_construct_set_final(&mut h, parse_leading_i32(tokens[0]));
        }
    }
    fsm_construct_set_initial(&mut h, 0);
    /* fclose (drop reader) */
    let mut net = fsm_construct_done(h);
    fsm_count(&mut net);
    let net = fsm_topsort(net);
    Some(net)
}

// [spec:foma:def:io.fsm-read-prolog-fn]
// [spec:foma:sem:io.fsm-read-prolog-fn+1]
// [spec:foma:def:fomalib.fsm-read-prolog-fn]
// [spec:foma:sem:fomalib.fsm-read-prolog-fn+1]
pub fn fsm_read_prolog(filename: &str) -> Option<Box<Fsm>> {
    /* The C source's strstr/strchr lookups below were unchecked (NULL-deref
    crash on a malformed line, or an out-of-order fact before the network
    declaration). On any missing delimiter or absent net handle, report a format
    error and return None instead of crashing. The fixed C buffers temp[1024]/
    in[128]/out[128] can overflow on long fields — here the extracted Strings
    grow (memory-safe). */
    macro_rules! field {
        ($e:expr) => {
            match $e {
                Some(v) => v,
                None => {
                    tracing::error!("File format error in prolog file.");
                    return None;
                }
            }
        };
    }
    let mut has_net = 0i32;
    let prolog_file = match File::open(filename) {
        Ok(f) => f,
        Err(_) => return None,
    };
    let mut reader = BufReader::new(prolog_file);
    let mut outh: Option<Box<FsmConstructHandle>> = None;
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        if buf.starts_with("network(") {
            /* Extract network name */
            if has_net == 1 {
                /* C: perror(...) — appends the errno string, not reproduced */
                tracing::warn!("prolog file contains multiple nets. Only returning the first one.");
                break;
            } else {
                has_net = 1;
            }
            let temp_ptr = field!(buf.find("network(")) + 8;
            let temp_ptr2 = field!(buf.find(")."));
            let temp = &buf[temp_ptr..temp_ptr2];
            outh = Some(fsm_construct_init(temp));
        }
        if buf.starts_with("final(") {
            let temp_ptr = field!(buf.find(' ')) + 1;
            let temp_ptr2 = field!(buf[temp_ptr..].find(").")) + temp_ptr;
            let temp = &buf[temp_ptr..temp_ptr2];
            fsm_construct_set_final(field!(outh.as_deref_mut()), parse_leading_i32(temp));
        }
        if buf.starts_with("symbol(") {
            let temp_ptr = field!(buf.find(", \"")) + 3;
            let temp_ptr2 = field!(buf[temp_ptr..].find("\").")) + temp_ptr;
            let mut temp = buf[temp_ptr..temp_ptr2].to_string();
            if temp == "%0" {
                temp = "0".to_string();
            }
            let oh = field!(outh.as_deref_mut());
            if fsm_construct_check_symbol(oh, &temp) == -1 {
                fsm_construct_add_symbol(oh, &temp);
            }
            continue;
        }
        if buf.starts_with("arc(") {
            let mut in_: String;
            let mut out_ = String::new();

            let arity = if buf.find("\":\"").is_none() || buf.find(", \":\").").is_some() {
                1
            } else {
                2
            };

            /* Get source */
            let mut temp_ptr = field!(buf.find(' ')) + 1;
            let mut temp_ptr2 = field!(buf[temp_ptr..].find(',')) + temp_ptr;
            let source = parse_leading_i32(&buf[temp_ptr..temp_ptr2]);

            /* Get target */
            temp_ptr = field!(buf[temp_ptr2..].find(' ')) + temp_ptr2 + 1;
            temp_ptr2 = field!(buf[temp_ptr..].find(',')) + temp_ptr;
            let target = parse_leading_i32(&buf[temp_ptr..temp_ptr2]);

            temp_ptr = field!(buf[temp_ptr2..].find('"')) + temp_ptr2 + 1;
            if arity == 2 {
                temp_ptr2 = field!(buf[temp_ptr..].find("\":")) + temp_ptr;
            } else {
                temp_ptr2 = field!(buf[temp_ptr..].find("\").")) + temp_ptr;
            }
            in_ = buf[temp_ptr..temp_ptr2].to_string();

            if arity == 2 {
                temp_ptr = field!(buf[temp_ptr2..].find(":\"")) + temp_ptr2 + 2;
                temp_ptr2 = field!(buf[temp_ptr..].find("\").")) + temp_ptr;
                out_ = buf[temp_ptr..temp_ptr2].to_string();
            }
            if arity == 1 && in_ == "?" {
                in_ = "@_IDENTITY_SYMBOL_@".to_string();
            }
            if arity == 2 && in_ == "?" {
                in_ = "@_UNKNOWN_SYMBOL_@".to_string();
            }
            if arity == 2 && out_ == "?" {
                out_ = "@_UNKNOWN_SYMBOL_@".to_string();
            }
            if in_ == "0" {
                in_ = "@_EPSILON_SYMBOL_@".to_string();
            }
            if out_ == "0" {
                out_ = "@_EPSILON_SYMBOL_@".to_string();
            }
            if in_ == "%0" {
                in_ = "0".to_string();
            }
            if out_ == "%0" {
                out_ = "0".to_string();
            }
            if in_ == "%?" {
                in_ = "?".to_string();
            }
            if out_ == "%?" {
                out_ = "?".to_string();
            }

            let oh = field!(outh.as_deref_mut());
            if arity == 1 {
                fsm_construct_add_arc(oh, source, target, &in_, &in_);
            } else {
                fsm_construct_add_arc(oh, source, target, &in_, &out_);
            }
        }
    }
    /* fclose (drop reader) */
    if has_net == 1 {
        fsm_construct_set_initial(outh.as_deref_mut().expect("outh built when has_net==1"), 0);
        let mut outnet = fsm_construct_done(outh.take().expect("outh built when has_net==1"));
        /* C: fsm_topsort(outnet) with the return value ignored (relies on
        in-place update). DEVIATION from C: fsm_topsort consumes/returns the
        Box, so we rebind — observably identical, topsort returns the net. */
        outnet = fsm_topsort(outnet);
        Some(outnet)
    } else {
        None
    }
}

// [spec:foma:def:io.io-init-fn]
// [spec:foma:sem:io.io-init-fn]
pub fn io_init() -> Box<IoBufHandle> {
    Box::new(IoBufHandle {
        io_buf: None,
        io_buf_ptr: 0,
    })
}

// [spec:foma:def:io.io-free-fn]
// [spec:foma:sem:io.io-free-fn]
#[allow(clippy::boxed_local)]
pub fn io_free(mut iobh: Box<IoBufHandle>) {
    if iobh.io_buf.is_some() {
        /* free(io_buf); io_buf = NULL */
        iobh.io_buf = None;
    }
    /* free(iobh) — dropped */
}

// [spec:foma:def:io.fsm-read-spaced-text-file-fn]
// [spec:foma:sem:io.fsm-read-spaced-text-file-fn]
// [spec:foma:def:fomalib.fsm-read-spaced-text-file-fn]
// [spec:foma:sem:fomalib.fsm-read-spaced-text-file-fn]
pub fn fsm_read_spaced_text_file(filename: &str) -> Option<Box<Fsm>> {
    let text = match file_to_mem(filename) {
        Err(_) => return None,
        Ok(t) => t,
    };
    /* file_to_mem appends a trailing NUL; drop it and read the words as text. */
    let body = String::from_utf8_lossy(&text[..text.len() - 1]);
    let mut th = fsm_trie_init();

    /* A record is one line (a single-tape word) or two consecutive non-blank
    lines (an upper:lower two-tape word); blank lines separate records. Within a
    line, whitespace-separated tokens are the word's symbols, where "0" is
    epsilon and "%0" is a literal "0". */
    fn sym(tok: &str) -> &str {
        match tok {
            "0" => "@_EPSILON_SYMBOL_@",
            "%0" => "0",
            other => other,
        }
    }

    let mut lines = body.lines().peekable();
    loop {
        while lines.peek() == Some(&"") {
            lines.next();
        }
        let upper = match lines.next() {
            None => break,
            Some(l) => l,
        };
        let two_tape = matches!(lines.peek(), Some(l) if !l.is_empty());
        if two_tape {
            let lower = lines.next().expect("peeked a non-empty lower line");
            let mut ins = upper.split_whitespace();
            let mut outs = lower.split_whitespace();
            loop {
                let (i, o) = (ins.next(), outs.next());
                if i.is_none() && o.is_none() {
                    break;
                }
                let insym = i.map(sym).unwrap_or("@_EPSILON_SYMBOL_@");
                let outsym = o.map(sym).unwrap_or("@_EPSILON_SYMBOL_@");
                fsm_trie_symbol(&mut th, insym, outsym);
            }
        } else {
            for tok in upper.split_whitespace() {
                let s = sym(tok);
                fsm_trie_symbol(&mut th, s, s);
            }
        }
        fsm_trie_end_word(&mut th);
    }
    Some(fsm_trie_done(th))
}

// [spec:foma:def:io.fsm-read-text-file-fn]
// [spec:foma:sem:io.fsm-read-text-file-fn]
// [spec:foma:def:fomalib.fsm-read-text-file-fn]
// [spec:foma:sem:fomalib.fsm-read-text-file-fn]
pub fn fsm_read_text_file(filename: &str) -> Option<Box<Fsm>> {
    let text = match file_to_mem(filename) {
        Err(_) => return None,
        Ok(t) => t,
    };
    /* file_to_mem appends a trailing NUL; drop it and add each non-empty line
    as a word (its characters become the arc symbols). */
    let body = String::from_utf8_lossy(&text[..text.len() - 1]);
    let mut th = fsm_trie_init();
    for line in body.lines() {
        if !line.is_empty() {
            fsm_trie_add_word(&mut th, line);
        }
    }
    Some(fsm_trie_done(th))
}

// [spec:foma:def:io.fsm-write-binary-file-fn]
// [spec:foma:sem:io.fsm-write-binary-file-fn]
// [spec:foma:def:fomalib.fsm-write-binary-file-fn]
// [spec:foma:sem:fomalib.fsm-write-binary-file-fn]
pub fn fsm_write_binary_file(net: &Fsm, filename: &str) -> std::io::Result<()> {
    /* gzopen(filename, "wb") — a GzEncoder over the output File. C returned 1
    on any failure and 0 on success; the same open/write/gzip-finish failures
    (e.g. a missing directory or a full disk) now propagate as the io::Error
    instead of collapsing to a sentinel — the named-file twin of
    fsm_write_binary. */
    let file = File::create(filename)?;
    let mut outfile = GzEncoder::new(file, Compression::default());
    foma_net_print(net, &mut outfile)?;
    /* gzclose(outfile) — finish the gzip stream */
    outfile.finish()?;
    Ok(())
}

// [spec:foma:def:io.fsm-write-binary-fn]
// [spec:foma:sem:io.fsm-write-binary-fn]
// New public API (no C counterpart): stream the gzip-compressed foma binary
// image of `net` to an arbitrary writer, mirroring fsm_write_binary_file's gzip
// behavior but to any `Write` sink instead of a named file.
pub fn fsm_write_binary<W: std::io::Write>(net: &Fsm, out: W) -> std::io::Result<()> {
    let mut enc = GzEncoder::new(out, Compression::default());
    foma_net_print(net, &mut enc)?;
    enc.finish()?;
    Ok(())
}

// [spec:foma:def:io.fsm-read-binary-file-multiple-fn]
// [spec:foma:sem:io.fsm-read-binary-file-multiple-fn]
// [spec:foma:def:fomalib.fsm-read-binary-file-multiple-fn]
// [spec:foma:sem:fomalib.fsm-read-binary-file-multiple-fn]
// The opaque handle is reused across calls and freed on the NULL return, so the
// caller passes it as `&mut Option<...>`; on the NULL path the handle is dropped
// (io_free) and the caller's Option becomes None ("must not be used again").
pub fn fsm_read_binary_file_multiple(
    fsrh: &mut Option<Box<FsmReadBinaryHandle>>,
) -> Option<Box<Fsm>> {
    /* iobh = (struct io_buf_handle *) fsrh (must be non-NULL) */
    let result = {
        let handle = fsrh.as_mut().expect("fsrh handle must be present");
        io_net_read(&mut handle.iobh)
    };
    match result {
        Ok(Some(net)) => Some(net),
        // Clean end of stream or a format error: the C returned NULL either way.
        Ok(None) | Err(_) => {
            /* io_free(iobh) — drop the whole handle */
            *fsrh = None;
            None
        }
    }
}

// [spec:foma:def:io.fsm-read-binary-file-multiple-init-fn]
// [spec:foma:sem:io.fsm-read-binary-file-multiple-init-fn]
pub fn fsm_read_binary_file_multiple_init(filename: &str) -> Option<Box<FsmReadBinaryHandle>> {
    let mut iobh = io_init();
    if io_gz_file_to_mem(&mut iobh, filename) == 0 {
        io_free(iobh);
        return None;
    }
    /* (void *) iobh */
    Some(Box::new(FsmReadBinaryHandle { iobh }))
}

// [spec:foma:def:io.fsm-read-binary-file-fn]
// [spec:foma:sem:io.fsm-read-binary-file-fn+1]
// [spec:foma:def:fomalib.fsm-read-binary-file-fn]
// [spec:foma:sem:fomalib.fsm-read-binary-file-fn]
// Wave 4: returns `Result<Box<Fsm>, FomaError>` instead of the C NULL sentinel —
// an unreadable/empty file is `Err(Io)`, a structurally malformed image is
// `Err(Format)`. The single regex.rs caller adapts with `.ok()`.
pub fn fsm_read_binary_file(filename: &str) -> Result<Box<Fsm>, FomaError> {
    let mut iobh = io_init();
    if io_gz_file_to_mem(&mut iobh, filename) == 0 {
        io_free(iobh);
        return Err(FomaError::Io(format!(
            "cannot read binary file '{filename}'"
        )));
    }
    let net = io_net_read(&mut iobh);
    io_free(iobh);
    net?.ok_or_else(|| FomaError::Format(format!("malformed foma binary file '{filename}'")))
}

// [spec:foma:def:io.fsm-read-binary-mem-fn]
// [spec:foma:sem:io.fsm-read-binary-mem-fn]
// New public API (no C counterpart): read a foma binary image already held in
// memory. Sniffs the gzip magic (1f 8b) like io_gz_file_to_mem: if gzip,
// GzDecoder-decompress into a Vec; otherwise use the bytes as-is. A trailing 0
// terminates the buffer image, then io_net_read parses it.
pub fn fsm_read_binary_mem(bytes: &[u8]) -> Result<Box<Fsm>, FomaError> {
    let mut content: Vec<u8> = Vec::new();
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        let mut dec = GzDecoder::new(bytes);
        dec.read_to_end(&mut content)
            .map_err(|e| FomaError::Io(format!("gzip decode error: {e}")))?;
    } else {
        content.extend_from_slice(bytes);
    }
    /* buf[size] = '\0' — matches io_gz_file_to_mem's terminator */
    content.push(0);
    let mut iobh = IoBufHandle {
        io_buf: Some(content),
        io_buf_ptr: 0,
    };
    io_net_read(&mut iobh)?
        .ok_or_else(|| FomaError::Format("malformed foma binary image".to_string()))
}

// New public API (no C counterpart): like `fsm_read_binary_mem`, but reads
// exactly ONE gzip member (one foma image) off the front of `bytes` and reports
// how many input bytes it consumed. Lets a caller reading a multi-image stream
// (e.g. HFST's per-transducer [header][gzip-image] framing) leave the remaining
// images in place instead of swallowing the whole tail.
pub fn fsm_read_binary_mem_prefix(bytes: &[u8]) -> Result<(Box<Fsm>, usize), FomaError> {
    let mut content: Vec<u8> = Vec::new();
    let consumed = if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        // A BUFREAD gzip decoder advances the underlying BufRead by exactly the
        // bytes it consumes (via `consume()`), unlike the `read` variant which
        // buffers ahead and would over-report. Reading one member off a Cursor
        // leaves the cursor exactly at that member's end.
        let mut cursor = std::io::Cursor::new(bytes);
        let mut dec = flate2::bufread::GzDecoder::new(&mut cursor);
        dec.read_to_end(&mut content)
            .map_err(|e| FomaError::Io(format!("gzip decode error: {e}")))?;
        drop(dec);
        cursor.position() as usize
    } else {
        content.extend_from_slice(bytes);
        bytes.len()
    };
    content.push(0);
    let mut iobh = IoBufHandle {
        io_buf: Some(content),
        io_buf_ptr: 0,
    };
    let net = io_net_read(&mut iobh)?
        .ok_or_else(|| FomaError::Format("malformed foma binary image".to_string()))?;
    Ok((net, consumed))
}

// [spec:foma:def:io.fsm-read-binary-fn]
// [spec:foma:sem:io.fsm-read-binary-fn]
// New public API (no C counterpart): read a foma binary image from an arbitrary
// reader by draining it to a Vec and delegating to fsm_read_binary_mem.
pub fn fsm_read_binary<R: std::io::Read>(mut reader: R) -> Result<Box<Fsm>, FomaError> {
    let mut bytes: Vec<u8> = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|e| FomaError::Io(format!("read error: {e}")))?;
    fsm_read_binary_mem(&bytes)
}

// [spec:foma:def:io.save-defined-fn]
// [spec:foma:sem:io.save-defined-fn]
// [spec:foma:def:fomalib.save-defined-fn]
// [spec:foma:sem:fomalib.save-defined-fn]
pub fn save_defined(def: &mut DefinedNetworks, filename: &str) -> Result<(), FomaError> {
    /* C: def == NULL → "No defined networks.\n" (stderr) and return 0. A &mut
    reference is never NULL, so that NULL check stays at the call site. */
    let file = match File::create(filename) {
        Ok(f) => f,
        Err(e) => {
            return Err(FomaError::Io(format!(
                "cannot open file {filename} for writing: {e}"
            )));
        }
    };
    let mut outfile = GzEncoder::new(file, Compression::default());
    let mut d = Some(&mut *def);
    while let Some(node) = d {
        let name = node.name.as_deref().unwrap_or("").to_string();
        let Some(net) = node.net.as_mut() else {
            tracing::warn!("Skipping definition without network.");
            d = node.next.as_deref_mut();
            continue;
        };
        /* C: strncpy(d->net->name, d->name, FSM_NAME_LEN) — the 40-byte cap was
        a struct-buffer size, gone now that names are heap Strings. */
        net.name = name.into();
        foma_net_print(net, &mut outfile)?;
        d = node.next.as_deref_mut();
    }
    /* gzclose(outfile) */
    outfile.finish()?;
    Ok(())
}

// [spec:foma:def:io.load-defined-fn]
// [spec:foma:sem:io.load-defined-fn]
// [spec:foma:def:fomalib.load-defined-fn]
// [spec:foma:sem:fomalib.load-defined-fn]
pub fn load_defined(def: &mut DefinedNetworks, filename: &str) -> Result<(), FomaError> {
    let mut iobh = io_init();
    if io_gz_file_to_mem(&mut iobh, filename) == 0 {
        io_free(iobh);
        return Err(FomaError::Io(format!(
            "cannot read definitions from {filename}"
        )));
    }
    let result = loop {
        match io_net_read(&mut iobh) {
            Ok(None) => break Ok(()),
            Ok(Some(net)) => {
                /* the stored net name is the definition name; add_defined
                copies it out of the net */
                let name = net.name.clone();
                add_defined(def, Some(net), &name);
            }
            Err(e) => break Err(e),
        }
    };
    io_free(iobh);
    result
}

// [spec:foma:def:io.explode-line-fn]
// [spec:foma:sem:io.explode-line-fn]
// `values` is a growable Vec, so a states line with more than 5 fields (which
// the C int[5] the sole caller passes would overrun) merely lengthens it and
// io_net_read's switch default reports the format error.
pub(crate) fn explode_line(buf: &str, values: &mut Vec<i32>) -> i32 {
    values.clear();
    let bytes = buf.as_bytes();
    let mut j = 0usize;
    let mut items = 0i32;
    loop {
        let i = j;
        while j < bytes.len() && bytes[j] != b' ' {
            j += 1;
        }
        if j >= bytes.len() {
            /* buf[j] == '\0' */
            values.push(parse_leading_i32(&buf[i..j]));
            items += 1;
            break;
        } else {
            /* buf[j] == ' ' */
            values.push(parse_leading_i32(&buf[i..j]));
            items += 1;
            j += 1;
        }
    }
    items
}

/* The file format we use is an extremely simple text format */
/* which is gzip compressed through libz and consists of the following sections: */
/* ##foma-net VERSION## / ##props## / PROPERTIES LINE / ##sigma## / ...SIGMA... */
/* / ##states## / ...TRANSITIONS... / ##end## (see foma/io.c for the full note) */

// [spec:foma:def:io.io-net-read-fn]
// [spec:foma:sem:io.io-net-read-fn+4]
// C signature: struct fsm *io_net_read(io_buf_handle *iobh, char **net_name).
// The name rides on net.name (C's *net_name out-param was always identical to
// it). `Ok(None)` is a clean end of the buffer (no more nets); `Err` is a
// structural format error (the C printed a diagnostic and returned NULL — the
// caller now decides what to report).
pub fn io_net_read(iobh: &mut IoBufHandle) -> Result<Option<Box<Fsm>>, FomaError> {
    let mut buf = String::new();
    let mut lineint: Vec<i32> = Vec::new();
    /* char last_final = '1' (49) in C — a latent typo (an int 0/1 was surely
    intended), kept as-is: it is only consumed when the first states line has 2
    or 3 fields, which the writer never emits (line 1 always sets state_no), so
    the value is unobservable for well-formed files and memory-safe either way. */
    let mut last_final: i8 = b'1' as i8;

    if io_gets(iobh, &mut buf) == 0 {
        return Ok(None);
    }

    let mut net = fsm_create("");

    if buf != "##foma-net 1.0##" {
        fsm_destroy(net);
        return Err(FomaError::Format("File format error foma!".to_string()));
    }
    io_gets(iobh, &mut buf);
    if buf != "##props##" {
        fsm_destroy(net);
        return Err(FomaError::Format("File format error props!".to_string()));
    }
    /* Properties */
    io_gets(iobh, &mut buf);
    let mut extras: i32 = 0;
    {
        /* sscanf(buf, "%i %i %i %i %i %lld %i %i %i %i %i %i %s", ...) — each %i
        stops assigning on a missing field (a truncated props line leaves the
        remaining net fields at their fsm_create defaults) */
        let toks: Vec<&str> = buf.split_whitespace().collect();
        if !toks.is_empty() {
            net.arity = parse_leading_i32(toks[0]);
        }
        if toks.len() > 1 {
            net.arccount = parse_leading_i32(toks[1]);
        }
        if toks.len() > 2 {
            net.statecount = parse_leading_i32(toks[2]);
        }
        if toks.len() > 3 {
            net.linecount = parse_leading_i32(toks[3]);
        }
        if toks.len() > 4 {
            net.finalcount = parse_leading_i32(toks[4]);
        }
        if toks.len() > 5 {
            net.pathcount = parse_leading_i64(toks[5]);
        }
        if toks.len() > 6 {
            net.is_deterministic = parse_leading_i32(toks[6]);
        }
        if toks.len() > 7 {
            net.is_pruned = parse_leading_i32(toks[7]);
        }
        if toks.len() > 8 {
            net.is_minimized = parse_leading_i32(toks[8]);
        }
        if toks.len() > 9 {
            net.is_epsilon_free = parse_leading_i32(toks[9]);
        }
        if toks.len() > 10 {
            net.is_loop_free = parse_leading_i32(toks[10]);
        }
        if toks.len() > 11 {
            extras = parse_leading_i32(toks[11]);
        }
        // [spec:foma:sem:io.io-net-read-fn+4] a missing name field yields an empty
        // name. C's sscanf left the buffer holding the whole props line, so that
        // line became the net name.
        net.name = toks.get(12).copied().unwrap_or("").into();
    }
    io_gets(iobh, &mut buf);

    net.is_completed = extras & 3;
    net.arcs_sorted_in = (extras & 12) >> 2;
    net.arcs_sorted_out = (extras & 48) >> 4;

    /* Sigma header: skip anything until ##sigma## */
    while buf != "##sigma##" {
        if buf.is_empty() {
            fsm_destroy(net);
            return Err(FomaError::Format(
                "File format error at sigma definition!".to_string(),
            ));
        }
        io_gets(iobh, &mut buf);
    }

    /* Sigma lines */
    loop {
        let before = iobh.io_buf_ptr;
        io_gets(iobh, &mut buf);
        if buf.as_bytes().first() == Some(&b'#') {
            break;
        }
        if buf.is_empty() {
            // [spec:foma:sem:io.io-net-read-fn+3] a truly empty line is skipped,
            // but at end-of-buffer io_gets yields empty lines without advancing
            // the cursor; if no progress was made the file is truncated inside the
            // sigma section, so fail instead of looping forever (C hung here).
            if iobh.io_buf_ptr == before {
                fsm_destroy(net);
                return Err(FomaError::Format(
                    "File format error in sigma section!".to_string(),
                ));
            }
            continue;
        }
        /* new_symbol = strstr(buf, " ") — a spaceless line NULL-derefs in C;
        report a format error and bail instead, like the other sigma-section errors. */
        let p = match buf.find(' ') {
            Some(p) => p,
            None => {
                fsm_destroy(net);
                return Err(FomaError::Format(
                    "File format error in sigma section!".to_string(),
                ));
            }
        };
        let number_str = &buf[..p];
        let new_symbol = &buf[p + 1..];
        let n = parse_leading_i32(number_str);
        if new_symbol.is_empty() {
            /* a literal-newline symbol survives the line-oriented format */
            sigma_add_number(&mut net.sigma, "\n", n);
        } else {
            sigma_add_number(&mut net.sigma, new_symbol, n);
        }
    }

    /* States */
    if buf != "##states##" {
        /* C leaks net here */
        return Err(FomaError::Format("File format error!".to_string()));
    }
    /* malloc(linecount * sizeof(struct fsm_state)).
    DEVIATION from C (more lines than linecount OOB-write in C; Rust panics on
    the index; a negative/zero linecount likewise mis-sizes the buffer) */
    net.states = vec![
        FsmState {
            state_no: 0,
            r#in: 0,
            out: 0,
            target: 0,
            final_state: 0,
            start_state: 0,
        };
        net.linecount as usize
    ];
    let mut laststate: i32 = -1;
    let mut i = 0usize;
    loop {
        io_gets(iobh, &mut buf);
        if buf.as_bytes().first() == Some(&b'#') {
            break;
        }
        let items = explode_line(&buf, &mut lineint);
        match items {
            2 => {
                net.states[i].state_no = laststate;
                net.states[i].r#in = lineint[0] as i16;
                net.states[i].out = lineint[0] as i16;
                net.states[i].target = lineint[1];
                net.states[i].final_state = last_final;
            }
            3 => {
                net.states[i].state_no = laststate;
                net.states[i].r#in = lineint[0] as i16;
                net.states[i].out = lineint[1] as i16;
                net.states[i].target = lineint[2];
                net.states[i].final_state = last_final;
            }
            4 => {
                net.states[i].state_no = lineint[0];
                net.states[i].r#in = lineint[1] as i16;
                net.states[i].out = lineint[1] as i16;
                net.states[i].target = lineint[2];
                net.states[i].final_state = lineint[3] as i8;
                laststate = lineint[0];
                last_final = lineint[3] as i8;
            }
            5 => {
                net.states[i].state_no = lineint[0];
                net.states[i].r#in = lineint[1] as i16;
                net.states[i].out = lineint[2] as i16;
                net.states[i].target = lineint[3];
                net.states[i].final_state = lineint[4] as i8;
                laststate = lineint[0];
                last_final = lineint[4] as i8;
            }
            _ => {
                /* C leaks net here */
                return Err(FomaError::Format("File format error".to_string()));
            }
        }
        if laststate > 0 {
            net.states[i].start_state = 0;
        } else if laststate == -1 {
            net.states[i].start_state = -1;
        } else {
            net.states[i].start_state = 1;
        }
        i += 1;
    }

    if buf == "##cmatrix##" {
        crate::spelling::cmatrix_init(&mut net);
        let mut cm = 0usize;
        loop {
            io_gets(iobh, &mut buf);
            if buf.starts_with('#') {
                break;
            }
            let val: i32 = buf.trim().parse().unwrap_or(0);
            /* DEVIATION from C (no bounds check on cm; a matrix overrun writes
            OOB — Rust panics on the index instead) */
            net.medlookup
                .as_mut()
                .expect("cmatrix_init set up medlookup above")
                .confusion_matrix[cm] = val;
            cm += 1;
        }
    }
    if buf != "##end##" {
        /* C leaks net here */
        return Err(FomaError::Format("File format error!".to_string()));
    }
    Ok(Some(net))
}

// [spec:foma:def:io.io-gets-fn]
// [spec:foma:sem:io.io-gets-fn]
pub(crate) fn io_gets(iobh: &mut IoBufHandle, target: &mut String) -> i32 {
    /* NULL-derefs in C when io_buf == NULL; io_gets is only ever called after a
    successful load */
    let buf = iobh.io_buf.as_ref().expect("io_buf loaded before io_gets");
    let base = iobh.io_buf_ptr;
    let mut i = 0usize;
    let mut bytes: Vec<u8> = Vec::new();
    /* copy bytes up to but excluding the next '\n' or '\0' (or end of buffer).
    DEVIATION from C (no bounds check on target — a long line overruns the C
    READ_BUF_SIZE buffer; here target is a growable String) */
    while base + i < buf.len() && buf[base + i] != b'\n' && buf[base + i] != b'\0' {
        bytes.push(buf[base + i]);
        i += 1;
    }
    /* advance past the '\n', or onto the '\0'/end (sticky at end-of-buffer) */
    let new_ptr = if base + i >= buf.len() || buf[base + i] == b'\0' {
        base + i
    } else {
        base + i + 1
    };
    /* NUL-terminate target (the String length replaces the C NUL).
    DEVIATION from C (lossy UTF-8 decode; C keeps the raw bytes) */
    *target = String::from_utf8_lossy(&bytes).into_owned();
    iobh.io_buf_ptr = new_ptr;
    i as i32
}

// [spec:foma:def:io.foma-net-print-fn]
// [spec:foma:sem:io.foma-net-print-fn+1]
// [spec:foma:def:fomalib.foma-net-print-fn]
// [spec:foma:sem:fomalib.foma-net-print-fn+1]
// C signature: int foma_net_print(struct fsm *net, gzFile outfile) (the C `1`
// return was a vestigial always-success status). Here the gzip layer is the
// GzEncoder (or any other writer) the caller passes as `&mut W`, dispatched
// statically, and a write failure is propagated as an `io::Error` instead of
// being reported by a return code no caller inspected.
pub fn foma_net_print<W: std::io::Write + ?Sized>(
    net: &Fsm,
    outfile: &mut W,
) -> std::io::Result<()> {
    /* Header */
    outfile.write_all(b"##foma-net 1.0##\n")?;
    /* Properties */
    outfile.write_all(b"##props##\n")?;

    let extras = net.is_completed | (net.arcs_sorted_in << 2) | (net.arcs_sorted_out << 4);

    writeln!(
        outfile,
        "{} {} {} {} {} {} {} {} {} {} {} {} {}",
        net.arity,
        net.arccount,
        net.statecount,
        net.linecount,
        net.finalcount,
        net.pathcount,
        net.is_deterministic,
        net.is_pruned,
        net.is_minimized,
        net.is_epsilon_free,
        net.is_loop_free,
        extras,
        net.name
    )?;

    /* Sigma */
    outfile.write_all(b"##sigma##\n")?;
    for s in &net.sigma {
        /* gzprintf("%i %s\n", ...) — one "number symbol" line per entry, in
        alphabet order */
        writeln!(outfile, "{} {}", s.number, s.symbol)?;
    }

    /* State array */
    let mut laststate: i32 = -1;
    outfile.write_all(b"##states##\n")?;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let fsm = &net.states[i];
        if fsm.state_no != laststate {
            if fsm.r#in != fsm.out {
                writeln!(
                    outfile,
                    "{} {} {} {} {}",
                    fsm.state_no, fsm.r#in, fsm.out, fsm.target, fsm.final_state
                )?;
            } else {
                writeln!(
                    outfile,
                    "{} {} {} {}",
                    fsm.state_no, fsm.r#in, fsm.target, fsm.final_state
                )?;
            }
        } else if fsm.r#in != fsm.out {
            writeln!(outfile, "{} {} {}", fsm.r#in, fsm.out, fsm.target)?;
        } else {
            writeln!(outfile, "{} {}", fsm.r#in, fsm.target)?;
        }
        laststate = fsm.state_no;
        i += 1;
    }
    /* Sentinel for states */
    outfile.write_all(b"-1 -1 -1 -1 -1\n")?;

    /* Store confusion matrix */
    if let Some(ml) = net.medlookup.as_deref() {
        /* C: net->medlookup->confusion_matrix != NULL — an empty Vec ↔ NULL */
        if !ml.confusion_matrix.is_empty() {
            outfile.write_all(b"##cmatrix##\n")?;
            let maxsigma = sigma_max(&net.sigma) + 1;
            for k in 0..(maxsigma * maxsigma) {
                writeln!(outfile, "{}", ml.confusion_matrix[k as usize])?;
            }
        }
    }

    /* End */
    outfile.write_all(b"##end##\n")?;
    Ok(())
}

// [spec:foma:def:io.net-print-att-fn]
// [spec:foma:sem:io.net-print-att-fn+1]
// [spec:foma:def:fomalib.net-print-att-fn]
// [spec:foma:sem:fomalib.net-print-att-fn+1]
pub fn net_print_att<W: std::io::Write + ?Sized>(
    opts: &FomaOptions,
    net: &Fsm,
    outfile: &mut W,
) -> std::io::Result<()> {
    let mut sl = sigma_to_list(&net.sigma);
    if sigma_max(&net.sigma) >= 0 {
        /* (sl+0)->symbol = g_att_epsilon */
        sl[0].symbol = Some(opts.att_epsilon.clone());
    }
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let fsm = &net.states[i];
        if fsm.target != -1 {
            writeln!(
                outfile,
                "{}\t{}\t{}\t{}",
                fsm.state_no,
                fsm.target,
                sl[fsm.r#in as usize].symbol.as_deref().unwrap_or("(null)"),
                sl[fsm.out as usize].symbol.as_deref().unwrap_or("(null)")
            )?;
        }
        i += 1;
    }
    let mut prev: i32 = -1;
    let mut i = 0usize;
    while net.states[i].state_no != -1 {
        let fsm = &net.states[i];
        if fsm.state_no != prev && fsm.final_state == 1 {
            writeln!(outfile, "{}", fsm.state_no)?;
        }
        prev = fsm.state_no;
        i += 1;
    }
    /* free(sl) — dropped */
    Ok(())
}

// [spec:foma:def:io.io-get-gz-file-size-fn]
// [spec:foma:sem:io.io-get-gz-file-size-fn]
pub(crate) fn io_get_gz_file_size(filename: &str) -> usize {
    /* The last four bytes in a .gz file are the uncompressed size (ISIZE),
    little-endian. C leaves fopen unchecked; here a failed open/seek/read
    returns 0 (DEVIATION: C NULL-derefs / reads garbage). */
    let mut infile = match File::open(filename) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let mut bytes = [0u8; 4];
    if infile.seek(SeekFrom::End(-4)).is_err() {
        return 0;
    }
    if infile.read_exact(&mut bytes).is_err() {
        return 0;
    }
    ((bytes[0] as u32)
        | ((bytes[1] as u32) << 8)
        | ((bytes[2] as u32) << 16)
        | ((bytes[3] as u32) << 24)) as usize
}

// [spec:foma:def:io.io-get-regular-file-size-fn]
// [spec:foma:sem:io.io-get-regular-file-size-fn]
pub(crate) fn io_get_regular_file_size(filename: &str) -> usize {
    /* C: fopen, fseek END, ftell, fclose. fopen unchecked (DEVIATION: 0 on
    failure here). */
    std::fs::metadata(filename)
        .map(|m| m.len() as usize)
        .unwrap_or(0)
}

/* zlib's gzopen reads plain (uncompressed) files transparently — gzdirect()
reports which — but flate2's GzDecoder errors on non-gzip input, so we sniff the
gzip magic ourselves (the two bytes gzdirect keys on) and fall back to a plain
read. Reads two bytes from `file`, advancing its cursor; callers that must
re-read the body seek back to the start afterwards. */
fn is_gzip_magic<R: Read>(file: &mut R) -> bool {
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).is_ok() && magic == [0x1f, 0x8b]
}

// [spec:foma:def:io.io-get-file-size-fn]
// [spec:foma:sem:io.io-get-file-size-fn]
pub(crate) fn io_get_file_size(filename: &str) -> usize {
    /* C: gzopen(filename, "r"); if NULL return 0. gzdirect() == 1 (file is not
    gzip data, read raw) → regular on-disk size; else → gzip trailer size. */
    let mut file = match File::open(filename) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    if is_gzip_magic(&mut file) {
        io_get_gz_file_size(filename)
    } else {
        io_get_regular_file_size(filename)
    }
}

// [spec:foma:def:io.io-gz-file-to-mem-fn]
// [spec:foma:sem:io.io-gz-file-to-mem-fn]
pub fn io_gz_file_to_mem(iobh: &mut IoBufHandle, filename: &str) -> usize {
    let size = io_get_file_size(filename);
    if size == 0 {
        return 0;
    }
    /* C: malloc(size+1); gzopen "rb"; gzread(size); gzclose; buf[size]='\0'.
    gzopen transparently decompresses gzip AND passes plain files through. */
    let mut file = match File::open(filename) {
        Ok(f) => f,
        Err(_) => return 0,
    };
    let is_gzip = is_gzip_magic(&mut file);
    if file.seek(SeekFrom::Start(0)).is_err() {
        return 0;
    }
    let mut content: Vec<u8> = Vec::new();
    /* gzread's return is unchecked in C: a corrupt body leaves the tail
    uninitialized. read_to_end reads the whole (single-member) stream, which
    equals `size` for well-formed foma files; a read/decode error rewinds to
    the C NULL/0 failure return instead of handing back a truncated buffer. */
    let read_result = if is_gzip {
        let mut dec = GzDecoder::new(file);
        dec.read_to_end(&mut content)
    } else {
        file.read_to_end(&mut content)
    };
    if read_result.is_err() {
        return 0;
    }
    /* buf[size] = '\0' */
    content.push(0);
    iobh.io_buf = Some(content);
    iobh.io_buf_ptr = 0;
    size
}

// [spec:foma:def:io.check-bom-fn]
// [spec:foma:sem:io.check-bom-fn+1]
#[allow(non_snake_case)]
pub(crate) fn check_BOM(buffer: &[u8]) -> Option<&'static Bom> {
    /* Wave 4 fix: the C compared each entry with strncmp(code, buffer, len),
    which stops at a mutual NUL, so any buffer whose first byte was 0x00
    false-matched the UTF-32BE entry (00 00 FE FF) and "FF FE 00 <any>"
    false-matched UTF-32LE (the 4th byte was never checked). Compare the actual
    `len` BOM bytes exactly instead; a buffer shorter than `len` cannot match. */
    for bom in BOM_CODES.iter() {
        if bom.len == 0 {
            break;
        }
        let len = bom.len as usize;
        if buffer.len() >= len && buffer[..len] == bom.code[..len] {
            return Some(bom);
        }
    }
    None
}

/* C: static BOM BOM_codes[] — trailing initializers of `code` default to 0 */
static BOM_CODES: [Bom; 6] = [
    Bom {
        code: [0xEF, 0xBB, 0xBF, 0x00],
        len: 3,
        name: Some("UTF-8"),
    },
    Bom {
        code: [0xFF, 0xFE, 0x00, 0x00],
        len: 4,
        name: Some("UTF-32LE"),
    },
    Bom {
        code: [0x00, 0x00, 0xFE, 0xFF],
        len: 4,
        name: Some("UTF-32BE"),
    },
    Bom {
        code: [0xFF, 0xFE, 0x00, 0x00],
        len: 2,
        name: Some("UTF16-LE"),
    },
    Bom {
        code: [0xFE, 0xFF, 0x00, 0x00],
        len: 2,
        name: Some("UTF16-BE"),
    },
    Bom {
        code: [0x00, 0x00, 0x00, 0x00],
        len: 0,
        name: None,
    },
];

// [spec:foma:def:io.file-to-mem-fn]
// [spec:foma:sem:io.file-to-mem-fn+1]
// [spec:foma:def:fomalib.file-to-mem-fn]
// [spec:foma:sem:fomalib.file-to-mem-fn]
// Wave 4: returns `Result<Vec<u8>, FomaError>` instead of the C `char *`/NULL
// sentinel (the printed diagnostics are retained for CLI-output compatibility;
// the typed error lets binaries distinguish open/read failures from a BOM
// rejection). Callers that only care about success adapt with `.ok()`.
pub fn file_to_mem(name: &str) -> Result<Vec<u8>, FomaError> {
    let mut infile = match File::open(name) {
        Ok(f) => f,
        Err(_) => {
            tracing::error!("Error opening file '{}'", name);
            return Err(FomaError::Io(format!("cannot open file '{name}'")));
        }
    };
    /* fseek END + ftell → on-disk size */
    let numbytes = infile.metadata().map(|m| m.len() as usize).unwrap_or(0);
    /* malloc(numbytes+1) — never NULL in Rust; fread numbytes */
    let mut content = vec![0u8; numbytes];
    if infile.read_exact(&mut content).is_err() {
        tracing::error!("Error reading file '{}'", name);
        return Err(FomaError::Io(format!("cannot read file '{name}'")));
    }
    /* check_BOM runs on the buffer BEFORE the '\0' terminator is written, as in
    C (bytes past the file end are absent, so a short file cannot false-match). */
    if let Some(bom) = check_BOM(&content) {
        tracing::error!(
            "{} BOM mark is detected in file '{}'.",
            bom.name.expect("a matched BOM entry has a name"),
            name
        );
        return Err(FomaError::Format(format!(
            "{} BOM in file '{name}'",
            bom.name.expect("a matched BOM entry has a name")
        )));
    }
    /* fclose (drop infile); buffer[numbytes] = '\0' */
    content.push(0);
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::{apply_down, apply_init, apply_up};
    use crate::define::{defined_networks_init, find_defined};
    use crate::regex::fsm_parse_regex;
    use crate::spelling::cmatrix_init;

    /* ---- scratch files: unique per test, dropped on exit (best-effort) ---- */
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static CTR: AtomicU64 = AtomicU64::new(0);
            let n = CTR.fetch_add(1, Ordering::Relaxed);
            let mut p = std::env::temp_dir();
            p.push(format!("foma_io_test_{}_{}_{}", std::process::id(), tag, n));
            Scratch(p)
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /* ---- helpers ---- */
    fn parse(rx: &str) -> Box<Fsm> {
        let opts = &FomaOptions::default();
        fsm_parse_regex(opts, rx, None, None).expect("regex should compile")
    }

    fn drain_down(net: &Fsm, w: &str) -> Vec<String> {
        let mut h = apply_init(net);
        let mut o = Vec::new();
        let mut r = apply_down(&mut h, Some(w));
        while let Some(s) = r {
            o.push(s);
            r = apply_down(&mut h, None);
        }
        o
    }
    fn drain_up(net: &Fsm, w: &str) -> Vec<String> {
        let mut h = apply_init(net);
        let mut o = Vec::new();
        let mut r = apply_up(&mut h, Some(w));
        while let Some(s) = r {
            o.push(s);
            r = apply_up(&mut h, None);
        }
        o
    }

    fn sigma_pairs(net: &Fsm) -> Vec<(i32, Option<String>)> {
        net.sigma
            .iter()
            .map(|x| (x.number, Some(x.symbol.to_string())))
            .collect()
    }

    /* Transition table (state_no, in, out, target, final) up to the sentinel. */
    fn state_lines(net: &Fsm) -> Vec<(i32, i16, i16, i32, i8)> {
        let mut v = Vec::new();
        let mut i = 0usize;
        while net.states[i].state_no != -1 {
            let s = &net.states[i];
            v.push((s.state_no, s.r#in, s.out, s.target, s.final_state));
            i += 1;
        }
        v
    }

    /* Net equality on states/sigma/flags/name (start_state is recomputed by the
    reader and intentionally not compared). */
    fn assert_net_eq(a: &Fsm, b: &Fsm) {
        assert_eq!(a.name, b.name, "name");
        assert_eq!(a.arity, b.arity, "arity");
        assert_eq!(a.arccount, b.arccount, "arccount");
        assert_eq!(a.statecount, b.statecount, "statecount");
        assert_eq!(a.linecount, b.linecount, "linecount");
        assert_eq!(a.finalcount, b.finalcount, "finalcount");
        assert_eq!(a.pathcount, b.pathcount, "pathcount");
        assert_eq!(a.is_deterministic, b.is_deterministic, "is_deterministic");
        assert_eq!(a.is_pruned, b.is_pruned, "is_pruned");
        assert_eq!(a.is_minimized, b.is_minimized, "is_minimized");
        assert_eq!(a.is_epsilon_free, b.is_epsilon_free, "is_epsilon_free");
        assert_eq!(a.is_loop_free, b.is_loop_free, "is_loop_free");
        assert_eq!(a.is_completed, b.is_completed, "is_completed");
        assert_eq!(a.arcs_sorted_in, b.arcs_sorted_in, "arcs_sorted_in");
        assert_eq!(a.arcs_sorted_out, b.arcs_sorted_out, "arcs_sorted_out");
        assert_eq!(sigma_pairs(a), sigma_pairs(b), "sigma");
        assert_eq!(state_lines(a), state_lines(b), "states");
    }

    fn add_sig(net: &mut Fsm, syms: &[(i32, &str)]) {
        for (n, s) in syms {
            sigma_add_number(&mut net.sigma, s, *n);
        }
    }

    fn sent() -> FsmState {
        FsmState {
            state_no: -1,
            r#in: -1,
            out: -1,
            target: -1,
            final_state: -1,
            start_state: -1,
        }
    }

    /* A hand-built a:b transducer whose field values mirror the C `save stack`
    output (so foma_net_print emits an exact, known byte image). */
    fn craft_ab_net(name: &str) -> Box<Fsm> {
        let mut net = fsm_create(name);
        net.arity = 2;
        net.arccount = 1;
        net.statecount = 2;
        net.linecount = 3;
        net.finalcount = 1;
        net.pathcount = 1;
        net.is_deterministic = 1;
        net.is_pruned = 1;
        net.is_minimized = 1;
        net.is_epsilon_free = 1;
        net.is_loop_free = 1;
        net.is_completed = 2;
        net.arcs_sorted_in = 0;
        net.arcs_sorted_out = 0;
        add_sig(&mut net, &[(3, "a"), (4, "b")]);
        net.states = vec![
            FsmState {
                state_no: 0,
                r#in: 3,
                out: 4,
                target: 1,
                final_state: 0,
                start_state: 1,
            },
            FsmState {
                state_no: 1,
                r#in: -1,
                out: -1,
                target: -1,
                final_state: 1,
                start_state: 0,
            },
            sent(),
        ];
        net
    }

    /* The exact uncompressed foma wire image of craft_ab_net("test"). */
    const AB_FOMA_TEXT: &str = "##foma-net 1.0##\n##props##\n2 1 2 3 1 1 1 1 1 1 1 2 test\n##sigma##\n3 a\n4 b\n##states##\n0 3 4 1 0\n1 -1 -1 1\n-1 -1 -1 -1 -1\n##end##\n";

    fn read_first_bytes(path: &str, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        let mut f = File::open(path).unwrap();
        f.read_exact(&mut v).unwrap();
        v
    }

    /* =============================== tests =============================== */

    // [spec:foma:def:io.binaryline/test]
    #[test]
    fn binaryline_struct_holds_fields() {
        /* Dead struct, never read by any function — pin its shape for id coverage. */
        let b = Binaryline {
            r#type: 1,
            state: 2,
            r#in: 3,
            target: 4,
            out: 5,
            symbol: 6,
            name: Some("n".into()),
            value: None,
        };
        assert_eq!(
            (b.r#type, b.state, b.r#in, b.target, b.out, b.symbol),
            (1, 2, 3, 4, 5, 6)
        );
        assert_eq!(b.name.as_deref(), Some("n"));
        assert!(b.value.is_none());
    }

    // [spec:foma:sem:io.escape-print-fn/test]
    #[test]
    fn escape_print_quotes_and_passthrough() {
        /* No quote → single write of the whole string. */
        let mut a: Vec<u8> = Vec::new();
        escape_print(&mut a, "abc").expect("writing to in-memory buffer");
        assert_eq!(a, b"abc");
        /* Contains a quote → byte-by-byte, each `"` becomes `\"`; backslashes
        pass through unescaped (documented asymmetry). */
        let mut b: Vec<u8> = Vec::new();
        escape_print(&mut b, "he\"l\\o").expect("writing to in-memory buffer");
        assert_eq!(b, b"he\\\"l\\o");
    }

    // [spec:foma:def:io.io-buf-handle/test]
    // [spec:foma:sem:io.io-init-fn/test]
    #[test]
    fn io_init_zeroes_handle() {
        let h = io_init();
        assert!(h.io_buf.is_none());
        assert_eq!(h.io_buf_ptr, 0);
    }

    // [spec:foma:sem:io.io-free-fn/test]
    #[test]
    fn io_free_consumes_handle() {
        let mut h = io_init();
        h.io_buf = Some(vec![1, 2, 3]);
        /* frees io_buf and the handle (consumed by value) */
        io_free(h);
    }

    // [spec:foma:sem:io.io-gets-fn/test]
    #[test]
    fn io_gets_reads_lines_then_sticks_at_end() {
        let mut h = IoBufHandle {
            io_buf: Some(b"ab\ncd\0".to_vec()),
            io_buf_ptr: 0,
        };
        let mut t = String::new();
        assert_eq!(io_gets(&mut h, &mut t), 2);
        assert_eq!(t, "ab");
        assert_eq!(io_gets(&mut h, &mut t), 2);
        assert_eq!(t, "cd");
        /* at end-of-buffer every call returns 0 with an empty target (sticky) */
        assert_eq!(io_gets(&mut h, &mut t), 0);
        assert_eq!(t, "");
        assert_eq!(io_gets(&mut h, &mut t), 0);
        assert_eq!(t, "");
    }

    // [spec:foma:sem:io.explode-line-fn/test]
    #[test]
    fn explode_line_fields_and_overrun() {
        let mut v = Vec::new();
        assert_eq!(explode_line("0 3 4 1 0", &mut v), 5);
        assert_eq!(v, vec![0, 3, 4, 1, 0]);
        /* empty line yields one field of value 0 */
        assert_eq!(explode_line("", &mut v), 1);
        assert_eq!(v, vec![0]);
        /* consecutive spaces yield an empty field converted to 0 */
        assert_eq!(explode_line("1  2", &mut v), 3);
        assert_eq!(v, vec![1, 0, 2]);
        /* >5 fields: the growable Vec absorbs the overrun (DEVIATION) */
        assert_eq!(explode_line("1 2 3 4 5 6", &mut v), 6);
        assert_eq!(v, vec![1, 2, 3, 4, 5, 6]);
    }

    // [spec:foma:sem:io.foma-net-print-fn+1/test]
    // [spec:foma:sem:fomalib.foma-net-print-fn+1/test]
    #[test]
    fn foma_net_print_exact_wire_image() {
        let net = craft_ab_net("test");
        let mut buf: Vec<u8> = Vec::new();
        foma_net_print(&net, &mut buf).expect("writing net to in-memory buffer");
        assert_eq!(String::from_utf8(buf).unwrap(), AB_FOMA_TEXT);
    }

    // [spec:foma:sem:io.foma-net-print-fn+1/test]
    #[test]
    fn foma_net_print_cmatrix_section() {
        let mut net = craft_ab_net("cm");
        cmatrix_init(&mut net);
        net.medlookup.as_mut().unwrap().confusion_matrix[1] = 7;
        let mut buf: Vec<u8> = Vec::new();
        foma_net_print(&net, &mut buf).expect("writing net to in-memory buffer");
        let s = String::from_utf8(buf).unwrap();
        /* (sigma_max+1)^2 = 25 integer lines between ##cmatrix## and ##end## */
        assert!(s.contains("##cmatrix##\n"));
        let body = &s[s.find("##cmatrix##\n").unwrap() + 12..s.find("##end##").unwrap()];
        assert_eq!(body.lines().count(), 25);
    }

    // [spec:foma:sem:io.io-net-read-fn/test]
    #[test]
    fn io_net_read_parses_wire_image() {
        let mut buf = AB_FOMA_TEXT.as_bytes().to_vec();
        buf.push(0);
        let mut h = IoBufHandle {
            io_buf: Some(buf),
            io_buf_ptr: 0,
        };
        let net = io_net_read(&mut h).unwrap().unwrap();
        assert_eq!(net.name, "test");
        assert_net_eq(&net, &craft_ab_net("test"));
    }

    // [spec:foma:sem:io.io-net-read-fn/test]
    #[test]
    fn io_net_read_header_error_returns_none() {
        let mut h = IoBufHandle {
            io_buf: Some(b"garbage\0".to_vec()),
            io_buf_ptr: 0,
        };
        assert!(matches!(io_net_read(&mut h), Err(FomaError::Format(_))));
    }

    // [spec:foma:sem:io.fsm-write-binary-file-fn/test]
    // [spec:foma:sem:fomalib.fsm-write-binary-file-fn/test]
    // [spec:foma:sem:io.fsm-read-binary-file-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-read-binary-file-fn/test]
    #[test]
    fn binary_round_trip_acceptor_transducer_qmark() {
        for rx in ["[a b c]", "a:b", "?", "a:b|?"] {
            let mut net = parse(rx);
            net.name = "rt".into();
            let f = Scratch::new("bin");
            /* Ok = success */
            assert!(fsm_write_binary_file(&net, f.path()).is_ok());
            /* gzip magic 1f 8b in the file */
            assert_eq!(read_first_bytes(f.path(), 2), vec![0x1f, 0x8b]);
            let back = fsm_read_binary_file(f.path()).unwrap();
            assert_net_eq(&net, &back);
        }
    }

    // [spec:foma:sem:io.fsm-write-binary-file-fn/test]
    // [spec:foma:sem:fomalib.fsm-write-binary-file-fn/test]
    #[test]
    fn fsm_write_binary_file_open_failure_is_err() {
        let net = craft_ab_net("x");
        assert!(fsm_write_binary_file(&net, "/nonexistent_dir_zzz/deep/file.foma").is_err());
    }

    // [spec:foma:sem:io.fsm-write-binary-file-fn/test]
    // [spec:foma:sem:io.fsm-read-binary-file-fn+1/test]
    #[test]
    fn binary_round_trip_with_cmatrix() {
        let mut net = parse("[a b]");
        net.name = "cm".into();
        cmatrix_init(&mut net);
        net.medlookup.as_mut().unwrap().confusion_matrix[2] = 9;
        let f = Scratch::new("cm");
        assert!(fsm_write_binary_file(&net, f.path()).is_ok());
        let back = fsm_read_binary_file(f.path()).unwrap();
        assert_net_eq(&net, &back);
        assert_eq!(
            net.medlookup.as_ref().unwrap().confusion_matrix,
            back.medlookup.as_ref().unwrap().confusion_matrix
        );
    }

    // [spec:foma:sem:io.fsm-write-binary-fn/test]
    // [spec:foma:sem:io.fsm-read-binary-fn/test]
    // [spec:foma:sem:io.fsm-read-binary-mem-fn/test]
    #[test]
    fn stream_binary_round_trip() {
        let opts = &FomaOptions::default();
        let mut net = fsm_parse_regex(opts, "a:b;", None, None).expect("regex should compile");
        net.name = "stream".into();
        /* write the gzip-compressed image to an in-memory Vec */
        let mut buf: Vec<u8> = Vec::new();
        fsm_write_binary(&net, &mut buf).unwrap();
        /* gzip magic 1f 8b at the head */
        assert_eq!(&buf[..2], &[0x1f, 0x8b]);
        /* read it back through the Read-based entry point */
        let back = fsm_read_binary(&buf[..]).unwrap();
        assert_net_eq(&net, &back);
        /* the recognized relation survives the round trip */
        assert_eq!(drain_down(&back, "a"), vec!["b".to_string()]);
        /* and via the in-memory entry point directly */
        let back2 = fsm_read_binary_mem(&buf).unwrap();
        assert_net_eq(&net, &back2);
    }

    // [spec:foma:sem:io.io-gz-file-to-mem-fn/test]
    // [spec:foma:sem:io.fsm-read-binary-file-fn+1/test]
    #[test]
    fn read_uncompressed_foma_file_sniff_fallback() {
        /* Plain (uncompressed) .foma bytes must still parse via the sniff-fallback. */
        let f = Scratch::new("plain");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(AB_FOMA_TEXT.as_bytes()).unwrap();
        }
        let net = fsm_read_binary_file(f.path()).unwrap();
        assert_net_eq(&net, &craft_ab_net("test"));
    }

    // [spec:foma:sem:io.fsm-read-binary-file-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-read-binary-file-fn/test]
    #[test]
    fn read_c_foma_fixture_bytes() {
        /* Bytes captured from /opt/homebrew/bin/foma `save stack` for `a:b`
        (net name is a pointer-derived "7F50986"); our reader must parse it. */
        const C_AB: &[u8] = &[
            0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x13, 0x53, 0x56, 0x4e, 0xcb,
            0xcf, 0x4d, 0xd4, 0xcd, 0x4b, 0x2d, 0x51, 0x30, 0xd4, 0x33, 0x50, 0x56, 0xe6, 0x52,
            0x56, 0x2e, 0x28, 0xca, 0x2f, 0x28, 0x06, 0xb2, 0x8c, 0x14, 0x0c, 0x15, 0x8c, 0x14,
            0x8c, 0x81, 0x24, 0x32, 0x34, 0x52, 0x30, 0x77, 0x33, 0x35, 0xb0, 0xb4, 0x30, 0x03,
            0xaa, 0x2c, 0xce, 0x4c, 0xcf, 0x4d, 0x04, 0xaa, 0x34, 0x56, 0x48, 0xe4, 0x32, 0x51,
            0x48, 0x02, 0x89, 0x94, 0x24, 0x96, 0xa4, 0x82, 0x34, 0x1b, 0x00, 0x35, 0x9a, 0x00,
            0x95, 0x1b, 0x70, 0x19, 0x2a, 0xe8, 0x82, 0x91, 0x21, 0x97, 0x2e, 0x8c, 0x09, 0x46,
            0x40, 0xd5, 0xa9, 0x79, 0x29, 0x40, 0xa5, 0x00, 0xcc, 0x74, 0xaf, 0x15, 0x83, 0x00,
            0x00, 0x00,
        ];
        let f = Scratch::new("cfix");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(C_AB).unwrap();
        }
        let net = fsm_read_binary_file(f.path()).unwrap();
        assert_eq!(net.name, "7F50986");
        assert_eq!(net.arity, 2);
        assert_eq!(net.statecount, 2);
        assert_eq!(
            sigma_pairs(&net),
            vec![(3, Some("a".into())), (4, Some("b".into()))]
        );
    }

    // [spec:foma:sem:io.fsm-read-binary-file-multiple-init-fn/test]
    // [spec:foma:sem:io.fsm-read-binary-file-multiple-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-binary-file-multiple-fn/test]
    #[test]
    fn binary_multiple_iterates_all_nets_then_none() {
        let n1 = craft_ab_net("n1");
        let n2 = craft_ab_net("n2");
        let f = Scratch::new("multi");
        {
            let file = File::create(f.path()).unwrap();
            let mut enc = GzEncoder::new(file, Compression::default());
            foma_net_print(&n1, &mut enc).expect("writing net to scratch file");
            foma_net_print(&n2, &mut enc).expect("writing net to scratch file");
            enc.finish().unwrap();
        }
        let mut handle = fsm_read_binary_file_multiple_init(f.path());
        assert!(handle.is_some());
        let a = fsm_read_binary_file_multiple(&mut handle).unwrap();
        assert_eq!(a.name, "n1");
        let b = fsm_read_binary_file_multiple(&mut handle).unwrap();
        assert_eq!(b.name, "n2");
        /* NULL return frees the handle: caller's Option becomes None */
        assert!(fsm_read_binary_file_multiple(&mut handle).is_none());
        assert!(handle.is_none());
    }

    // [spec:foma:sem:io.fsm-read-binary-file-multiple-init-fn/test]
    #[test]
    fn binary_multiple_init_missing_file_none() {
        let mut p = std::env::temp_dir();
        p.push("foma_io_absent_zzz.foma");
        let _ = std::fs::remove_file(&p);
        assert!(fsm_read_binary_file_multiple_init(p.to_str().unwrap()).is_none());
    }

    // [spec:foma:sem:io.save-defined-fn/test]
    // [spec:foma:sem:fomalib.save-defined-fn/test]
    // [spec:foma:sem:io.load-defined-fn/test]
    // [spec:foma:sem:fomalib.load-defined-fn/test]
    #[test]
    fn save_and_load_defined_round_trip() {
        let mut def = defined_networks_init();
        add_defined(&mut def, Some(parse("a:b")), "T1");
        add_defined(&mut def, Some(parse("[c d]")), "T2");
        let f = Scratch::new("def");
        save_defined(&mut def, f.path()).expect("save round-trip");
        /* gzip magic present */
        assert_eq!(read_first_bytes(f.path(), 2), vec![0x1f, 0x8b]);

        let mut def2 = defined_networks_init();
        load_defined(&mut def2, f.path()).expect("load round-trip");
        let t1 = find_defined(&mut def2, "T1").expect("T1 reloaded");
        assert_eq!(drain_down(t1, "a"), vec!["b".to_string()]);
        let t2 = find_defined(&mut def2, "T2").expect("T2 reloaded");
        assert_eq!(drain_down(t2, "cd"), vec!["cd".to_string()]);
    }

    // [spec:foma:sem:io.load-defined-fn/test]
    // [spec:foma:sem:fomalib.load-defined-fn/test]
    #[test]
    fn load_defined_missing_file_returns_0() {
        let mut def = defined_networks_init();
        let mut p = std::env::temp_dir();
        p.push("foma_io_absent_def_zzz.foma");
        let _ = std::fs::remove_file(&p);
        assert!(matches!(
            load_defined(&mut def, p.to_str().unwrap()),
            Err(FomaError::Io(_))
        ));
    }

    // [spec:foma:sem:io.net-print-att-fn+1/test]
    // [spec:foma:sem:fomalib.net-print-att-fn+1/test]
    // [spec:foma:sem:io.read-att-fn/test]
    // [spec:foma:sem:fomalib.read-att-fn/test]
    #[test]
    fn att_round_trip_and_exact_bytes() {
        let opts = &FomaOptions::default();
        /* net_print_att emits arcs first, then final-state lines. */
        let net = craft_ab_net("att");
        let mut buf: Vec<u8> = Vec::new();
        net_print_att(opts, &net, &mut buf).expect("writing att to in-memory buffer");
        assert_eq!(buf, b"0\t1\ta\tb\n1\n");

        /* read_att parses that image back into an equivalent transducer. */
        let f = Scratch::new("att");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(&buf).unwrap();
        }
        let back = read_att(opts, f.path()).unwrap();
        assert_eq!(drain_down(&back, "a"), vec!["b".to_string()]);
        assert_eq!(drain_up(&back, "b"), vec!["a".to_string()]);
    }

    // [spec:foma:sem:io.read-att-fn/test]
    // [spec:foma:sem:fomalib.read-att-fn/test]
    #[test]
    fn read_att_missing_file_none() {
        let opts = &FomaOptions::default();
        let mut p = std::env::temp_dir();
        p.push("foma_io_absent_att_zzz.att");
        let _ = std::fs::remove_file(&p);
        assert!(read_att(opts, p.to_str().unwrap()).is_none());
    }

    // [spec:foma:sem:io.foma-write-prolog-fn+1/test]
    // [spec:foma:sem:fomalib.foma-write-prolog-fn/test]
    // [spec:foma:sem:io.fsm-read-prolog-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-read-prolog-fn+1/test]
    #[test]
    fn prolog_round_trip() {
        let mut net = parse("a:b");
        net.name = "rt".into();
        let f = Scratch::new("prolog");
        assert!(foma_write_prolog(&mut net, Some(f.path())).is_ok());
        let back = fsm_read_prolog(f.path()).unwrap();
        assert_eq!(drain_down(&back, "a"), vec!["b".to_string()]);
        assert_eq!(drain_up(&back, "b"), vec!["a".to_string()]);
    }

    // A malformed prolog file returns None instead of crashing (the C source's
    // unchecked strstr/strchr lookups NULL-deref on a missing delimiter, and a
    // fact before the first "network(" clause dereferences a NULL handle).
    // [spec:foma:sem:io.fsm-read-prolog-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-read-prolog-fn+1/test]
    #[test]
    fn prolog_malformed_returns_none_not_panic() {
        // "network(" line missing the ")." terminator.
        let f = Scratch::new("prolog_bad_net");
        std::fs::write(f.path(), "network(bad\n").unwrap();
        assert!(fsm_read_prolog(f.path()).is_none());
        // A well-formed arc fact before any "network(" clause: NULL handle.
        let g = Scratch::new("prolog_bad_order");
        std::fs::write(g.path(), "arc(0, 0, 1, \"a\":\"b\").\n").unwrap();
        assert!(fsm_read_prolog(g.path()).is_none());
        // "final(" line with no space delimiter.
        let h = Scratch::new("prolog_bad_final");
        std::fs::write(h.path(), "network(n).\nfinal(0).\n").unwrap();
        assert!(fsm_read_prolog(h.path()).is_none());
    }

    // [spec:foma:sem:io.foma-write-prolog-fn+1/test]
    // [spec:foma:sem:fomalib.foma-write-prolog-fn/test]
    #[test]
    fn foma_write_prolog_outside_qmark_escape_fixed() {
        /* Craft an arc epsilon:"?" where the out-symbol number (3) > 2 but the
        in-symbol number (0) is NOT > 2. Wave 4 fix: the out-side escape now
        tests out_ > 2, so the literal "?" out-symbol IS escaped to "%?" —
        pin `"0":"%?"` (the C typo left it unescaped as `"0":"?"`). */
        let mut net = fsm_create("bug");
        net.arity = 2;
        add_sig(&mut net, &[(3, "?")]);
        net.states = vec![
            FsmState {
                state_no: 0,
                r#in: 0,
                out: 3,
                target: 1,
                final_state: 0,
                start_state: 1,
            },
            FsmState {
                state_no: 1,
                r#in: -1,
                out: -1,
                target: -1,
                final_state: 1,
                start_state: 0,
            },
            sent(),
        ];
        let f = Scratch::new("prologbug");
        foma_write_prolog(&mut net, Some(f.path())).expect("write prolog to scratch file");
        let s = std::fs::read_to_string(f.path()).unwrap();
        assert!(s.contains("arc(bug, 0, 1, \"0\":\"%?\")."), "got:\n{}", s);
        assert!(!s.contains("\"0\":\"?\")"));
    }

    // [spec:foma:sem:io.fsm-read-text-file-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-text-file-fn/test]
    // [spec:foma:sem:io.file-to-mem-fn+1/test]
    // [spec:foma:sem:fomalib.file-to-mem-fn/test]
    #[test]
    fn read_text_file_word_list() {
        let f = Scratch::new("text");
        {
            let mut file = File::create(f.path()).unwrap();
            /* blank lines skipped; final line without trailing newline kept */
            file.write_all(b"cat\ndog\n\nfish").unwrap();
        }
        let net = fsm_read_text_file(f.path()).unwrap();
        assert_eq!(drain_down(&net, "cat"), vec!["cat".to_string()]);
        assert_eq!(drain_down(&net, "dog"), vec!["dog".to_string()]);
        assert_eq!(drain_down(&net, "fish"), vec!["fish".to_string()]);
        assert!(drain_down(&net, "ca").is_empty());
    }

    // [spec:foma:sem:io.fsm-read-spaced-text-file-fn/test]
    // [spec:foma:sem:fomalib.fsm-read-spaced-text-file-fn/test]
    #[test]
    fn read_spaced_text_file_records() {
        let f = Scratch::new("spaced");
        {
            let mut file = File::create(f.path()).unwrap();
            /* record 1: single line → identity "foo"; record 2: two lines →
            transducer "ab":"cd"; record 3: "%0" → literal "0". */
            file.write_all(b"f o o\n\na b\nc d\n\n%0\n").unwrap();
        }
        let net = fsm_read_spaced_text_file(f.path()).unwrap();
        assert_eq!(drain_down(&net, "foo"), vec!["foo".to_string()]);
        assert_eq!(drain_down(&net, "ab"), vec!["cd".to_string()]);
        assert_eq!(drain_down(&net, "0"), vec!["0".to_string()]);
    }

    // [spec:foma:sem:io.fsm-read-text-file-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-read-text-file-fn+1/test]
    #[test]
    fn read_text_file_crlf_and_interior_nul() {
        /* CRLF endings contribute no '\r' to the word (C kept it) */
        let f = Scratch::new("crlf");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(b"cat\r\ndog\r\n").unwrap();
        }
        let net = fsm_read_text_file(f.path()).unwrap();
        assert_eq!(drain_down(&net, "cat"), vec!["cat".to_string()]);
        assert_eq!(drain_down(&net, "dog"), vec!["dog".to_string()]);
        assert!(drain_down(&net, "cat\r").is_empty());

        /* an interior NUL is an ordinary word character (C stopped reading there) */
        let g = Scratch::new("nul");
        {
            let mut file = File::create(g.path()).unwrap();
            file.write_all(b"a\0b\nc\n").unwrap();
        }
        let net = fsm_read_text_file(g.path()).unwrap();
        assert_eq!(drain_down(&net, "a\u{0}b"), vec!["a\u{0}b".to_string()]);
        assert_eq!(drain_down(&net, "c"), vec!["c".to_string()]);
        assert!(drain_down(&net, "a").is_empty());
    }

    // [spec:foma:sem:io.fsm-read-spaced-text-file-fn+1/test]
    // [spec:foma:sem:fomalib.fsm-read-spaced-text-file-fn+1/test]
    #[test]
    fn read_spaced_text_file_whitespace_and_crlf() {
        /* tabs split symbols like spaces (C split on ' ' only), and a "\r\n"
        line is a blank record separator (in C a one-char "\r" line) */
        let f = Scratch::new("spacedws");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(b"a\tb\r\nc d\r\n\r\ne\r\n").unwrap();
        }
        let net = fsm_read_spaced_text_file(f.path()).unwrap();
        assert_eq!(drain_down(&net, "ab"), vec!["cd".to_string()]);
        assert_eq!(drain_down(&net, "e"), vec!["e".to_string()]);
        assert!(drain_down(&net, "a\tb").is_empty());
    }

    // [spec:foma:sem:io.fsm-read-spaced-text-file-fn/test]
    // [spec:foma:sem:io.fsm-read-text-file-fn/test]
    #[test]
    fn spaced_and_text_readers_missing_file_none() {
        let mut p = std::env::temp_dir();
        p.push("foma_io_absent_txt_zzz");
        let _ = std::fs::remove_file(&p);
        assert!(fsm_read_spaced_text_file(p.to_str().unwrap()).is_none());
        assert!(fsm_read_text_file(p.to_str().unwrap()).is_none());
    }

    // [spec:foma:sem:io.file-to-mem-fn+1/test]
    // [spec:foma:sem:fomalib.file-to-mem-fn/test]
    #[test]
    fn file_to_mem_normal_short_and_bom() {
        /* normal read: content + trailing '\0' (Wave 4: Ok instead of Some) */
        let f = Scratch::new("f2m");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(b"abc\n").unwrap();
        }
        assert_eq!(file_to_mem(f.path()).unwrap(), b"abc\n\0");

        /* short (<4 byte) non-BOM file reads fine (the exact-match check_BOM
        cannot false-match a buffer shorter than a BOM) */
        let g = Scratch::new("f2m");
        {
            let mut file = File::create(g.path()).unwrap();
            file.write_all(b"hi").unwrap();
        }
        assert_eq!(file_to_mem(g.path()).unwrap(), b"hi\0");

        /* UTF-8 BOM → rejected outright as Err(Format) */
        let h = Scratch::new("f2m");
        {
            let mut file = File::create(h.path()).unwrap();
            file.write_all(&[0xEF, 0xBB, 0xBF, b'x']).unwrap();
        }
        assert!(matches!(file_to_mem(h.path()), Err(FomaError::Format(_))));

        /* Wave 4: an empty file no longer false-matches UTF-32BE; it reads as
        the lone terminating '\0' */
        let e = Scratch::new("f2m");
        {
            File::create(e.path()).unwrap();
        }
        assert_eq!(file_to_mem(e.path()).unwrap(), b"\0");

        /* missing file → Err(Io) */
        let mut p = std::env::temp_dir();
        p.push("foma_io_absent_f2m_zzz");
        let _ = std::fs::remove_file(&p);
        assert!(matches!(
            file_to_mem(p.to_str().unwrap()),
            Err(FomaError::Io(_))
        ));
    }

    // [spec:foma:def:io.bom/test]
    // [spec:foma:sem:io.check-bom-fn+1/test]
    #[test]
    fn check_bom_exact_match_no_nul_false_positives() {
        assert_eq!(check_BOM(&[0xEF, 0xBB, 0xBF]).unwrap().name, Some("UTF-8"));
        /* full 4-byte marks match exactly */
        assert_eq!(
            check_BOM(&[0xFF, 0xFE, 0x00, 0x00]).unwrap().name,
            Some("UTF-32LE")
        );
        assert_eq!(
            check_BOM(&[0x00, 0x00, 0xFE, 0xFF]).unwrap().name,
            Some("UTF-32BE")
        );
        /* Wave 4 fix: a lone leading '\0' no longer false-matches UTF-32BE */
        assert!(check_BOM(&[0x00, 0x41, 0x42, 0x43]).is_none());
        assert!(check_BOM(&[0x00]).is_none());
        /* Wave 4 fix: FF FE 00 <non-00> is UTF16-LE (not a UTF-32LE false match) */
        assert_eq!(
            check_BOM(&[0xFF, 0xFE, 0x00, 0x99]).unwrap().name,
            Some("UTF16-LE")
        );
        /* FF FE <non-NUL> → UTF16-LE */
        assert_eq!(
            check_BOM(&[0xFF, 0xFE, 0x41, 0x42]).unwrap().name,
            Some("UTF16-LE")
        );
        assert_eq!(
            check_BOM(&[0xFE, 0xFF, 0x41, 0x42]).unwrap().name,
            Some("UTF16-BE")
        );
        assert!(check_BOM(b"hello").is_none());
    }

    // [spec:foma:sem:io.io-get-regular-file-size-fn/test]
    // [spec:foma:sem:io.io-get-file-size-fn/test]
    #[test]
    fn file_sizes_regular_and_plain() {
        let f = Scratch::new("sz");
        {
            let mut file = File::create(f.path()).unwrap();
            file.write_all(b"hello\n").unwrap();
        }
        assert_eq!(io_get_regular_file_size(f.path()), 6);
        /* non-gzip → io_get_file_size returns the on-disk size */
        assert_eq!(io_get_file_size(f.path()), 6);
        /* failures return 0 (DEVIATION vs C NULL-deref) */
        assert_eq!(io_get_regular_file_size("/no/such/file/zzz"), 0);
        assert_eq!(io_get_file_size("/no/such/file/zzz"), 0);
    }

    // [spec:foma:sem:io.io-get-gz-file-size-fn/test]
    // [spec:foma:sem:io.io-get-file-size-fn/test]
    #[test]
    fn file_sizes_gzip_trailer() {
        let payload = b"hello world\n"; // 12 bytes uncompressed
        let f = Scratch::new("gzsz");
        {
            let file = File::create(f.path()).unwrap();
            let mut enc = GzEncoder::new(file, Compression::default());
            enc.write_all(payload).unwrap();
            enc.finish().unwrap();
        }
        /* ISIZE trailer == uncompressed length */
        assert_eq!(io_get_gz_file_size(f.path()), payload.len());
        /* gzip file → io_get_file_size delegates to the trailer size */
        assert_eq!(io_get_file_size(f.path()), payload.len());
        assert_eq!(io_get_gz_file_size("/no/such/file/zzz"), 0);
    }

    // [spec:foma:sem:io.io-gz-file-to-mem-fn/test]
    #[test]
    fn io_gz_file_to_mem_gzip_and_plain() {
        let payload = b"abcdef";
        /* gzip path */
        let g = Scratch::new("g2m");
        {
            let file = File::create(g.path()).unwrap();
            let mut enc = GzEncoder::new(file, Compression::default());
            enc.write_all(payload).unwrap();
            enc.finish().unwrap();
        }
        let mut hg = io_init();
        assert_eq!(io_gz_file_to_mem(&mut hg, g.path()), payload.len());
        assert_eq!(hg.io_buf.as_deref().unwrap(), b"abcdef\0");
        assert_eq!(hg.io_buf_ptr, 0);

        /* plain path (sniff-fallback) */
        let p = Scratch::new("g2m");
        {
            let mut file = File::create(p.path()).unwrap();
            file.write_all(payload).unwrap();
        }
        let mut hp = io_init();
        assert_eq!(io_gz_file_to_mem(&mut hp, p.path()), payload.len());
        assert_eq!(hp.io_buf.as_deref().unwrap(), b"abcdef\0");

        /* missing/empty file → 0 */
        let mut hm = io_init();
        assert_eq!(io_gz_file_to_mem(&mut hm, "/no/such/file/zzz"), 0);
    }

    // [spec:foma:sem:io.io-net-read-fn+3/test]
    #[test]
    fn io_net_read_bails_on_truncated_sigma_section() {
        let opts = &FomaOptions::default();
        // Serialize a real net to text, then drop everything from "##states##"
        // onward so the buffer ends inside the sigma section. io_net_read must
        // return a Format error instead of looping forever on the empty lines
        // io_gets yields at end-of-buffer.
        let net = fsm_parse_regex(opts, "a b", None, None).unwrap();
        let mut text: Vec<u8> = Vec::new();
        foma_net_print(&net, &mut text).expect("writing net to in-memory buffer");
        let s = String::from_utf8(text).unwrap();
        let cut = s.find("##states##").expect("net text has a states section");
        let mut iobh = io_init();
        iobh.io_buf = Some(s.as_bytes()[..cut].to_vec());
        iobh.io_buf_ptr = 0;
        assert!(matches!(io_net_read(&mut iobh), Err(FomaError::Format(_))));
    }

    // [spec:foma:sem:io.io-net-read-fn+3/test]
    #[test]
    fn io_net_read_empty_name_when_field_absent() {
        let opts = &FomaOptions::default();
        // A net with an empty name serializes a props line with no name field, so
        // io_net_read reads 12 tokens. The name is then empty; C's sscanf left the
        // buffer holding the whole props line, which became the net name.
        let mut net = fsm_parse_regex(opts, "a b", None, None).unwrap();
        net.name = String::new().into();
        let mut text: Vec<u8> = Vec::new();
        foma_net_print(&net, &mut text).expect("writing net to in-memory buffer");
        let mut iobh = io_init();
        iobh.io_buf = Some(text);
        iobh.io_buf_ptr = 0;
        let net2 = io_net_read(&mut iobh)
            .expect("valid net should parse")
            .expect("a net, not clean EOF");
        assert_eq!(net2.name, "");
    }

    // A sigma line with no separating space returns a Format error instead of
    // crashing (C's strstr(buf, " ") NULL-derefs on a spaceless line).
    // [spec:foma:sem:io.io-net-read-fn+3/test]
    #[test]
    fn io_net_read_bails_on_spaceless_sigma_line() {
        let opts = &FomaOptions::default();
        let net = fsm_parse_regex(opts, "a b", None, None).unwrap();
        let mut text: Vec<u8> = Vec::new();
        foma_net_print(&net, &mut text).expect("writing net to in-memory buffer");
        let s = String::from_utf8(text).unwrap();
        // Delete the separating space from the first sigma entry line ("N sym").
        let sig_start = s.find("##sigma##\n").expect("sigma header") + "##sigma##\n".len();
        let line_end = s[sig_start..].find('\n').expect("a sigma line") + sig_start;
        let bad_line = s[sig_start..line_end].replacen(' ', "", 1);
        let corrupted = format!("{}{}{}", &s[..sig_start], bad_line, &s[line_end..]);
        let mut iobh = io_init();
        iobh.io_buf = Some(corrupted.into_bytes());
        iobh.io_buf_ptr = 0;
        assert!(io_net_read(&mut iobh).is_err());
    }
}
