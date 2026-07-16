//! foma/iface.c Wave-4 split: read/write/save/load file commands.
//! See iface/mod.rs.
use super::*;

// [spec:foma:def:iface.iface-load-defined-fn]
// [spec:foma:sem:iface.iface-load-defined-fn]
// [spec:foma:def:foma.iface-load-defined-fn]
// [spec:foma:sem:foma.iface-load-defined-fn]
pub fn iface_load_defined(session: &mut Session, filename: &str) {
    // C: load_defined(g_defines, filename); the registry is the session's
    // init'd dummy head. The library reader is now silent and returns a Result;
    // the user-facing progress line and any error live here.
    println!("Loading definitions from {filename}.");
    if let Err(e) = load_defined(&mut session.defines, filename) {
        eprintln!("{e}");
    }
}

// [spec:foma:def:iface.iface-read-att-fn]
// [spec:foma:sem:iface.iface-read-att-fn]
// [spec:foma:def:foma.iface-read-att-fn]
// [spec:foma:sem:foma.iface-read-att-fn]
pub fn iface_read_att(session: &mut Session, filename: &str) -> bool {
    println!("Reading AT&T file: {}", filename);
    match read_att(&session.opts, filename) {
        None => {
            eprint!("{}: ", filename);
            perror("Error opening file");
            false
        }
        Some(tempnet) => {
            session.stack_add(tempnet);
            true
        }
    }
}

// [spec:foma:def:iface.iface-read-prolog-fn]
// [spec:foma:sem:iface.iface-read-prolog-fn]
// [spec:foma:def:foma.iface-read-prolog-fn]
// [spec:foma:sem:foma.iface-read-prolog-fn]
pub fn iface_read_prolog(session: &mut Session, filename: &str) -> bool {
    println!("Reading prolog: {}", filename);
    match fsm_read_prolog(filename) {
        None => {
            eprint!("{}: ", filename);
            perror("Error opening file");
            false
        }
        Some(tempnet) => {
            session.stack_add(tempnet);
            true
        }
    }
}

// [spec:foma:def:iface.iface-read-spaced-text-fn]
// [spec:foma:sem:iface.iface-read-spaced-text-fn]
// [spec:foma:def:foma.iface-read-spaced-text-fn]
// [spec:foma:sem:foma.iface-read-spaced-text-fn]
pub fn iface_read_spaced_text(session: &mut Session, filename: &str) -> bool {
    match fsm_read_spaced_text_file(filename) {
        None => {
            eprint!("{}: ", filename);
            perror("File error");
            false
        }
        Some(net) => {
            session.stack_add(fsm_topsort(fsm_minimize(&session.opts, net)));
            true
        }
    }
}

// [spec:foma:def:iface.iface-read-text-fn]
// [spec:foma:sem:iface.iface-read-text-fn]
// [spec:foma:def:foma.iface-read-text-fn]
// [spec:foma:sem:foma.iface-read-text-fn]
pub fn iface_read_text(session: &mut Session, filename: &str) -> bool {
    match fsm_read_text_file(filename) {
        None => {
            eprint!("{}: ", filename);
            perror("File error");
            false
        }
        Some(net) => {
            session.stack_add(fsm_topsort(fsm_minimize(&session.opts, net)));
            true
        }
    }
}

// [spec:foma:def:iface.iface-save-defined-fn]
// [spec:foma:sem:iface.iface-save-defined-fn]
// [spec:foma:def:foma.iface-save-defined-fn]
// [spec:foma:sem:foma.iface-save-defined-fn]
pub fn iface_save_defined(session: &mut Session, filename: &str) {
    // save_defined(g_defines, filename). C printed "No defined networks.\n"
    // when g_defines was NULL (possible only before main's init); the session
    // registry always exists, so that branch is gone. The library writer is now
    // silent and returns a Result; the user-facing progress line and any error
    // live here.
    println!("Writing definitions to file {filename}.");
    if let Err(e) = save_defined(&mut session.defines, filename) {
        eprintln!("{e}");
    }
}

// [spec:foma:def:iface.iface-write-att-fn]
// [spec:foma:sem:iface.iface-write-att-fn]
// [spec:foma:def:foma.iface-write-att-fn]
// [spec:foma:sem:foma.iface-write-att-fn]
pub fn iface_write_att(session: &mut Session, filename: Option<&str>) -> bool {
    if !iface_stack_check(session, 1) {
        return false;
    }
    let Some(top) = session.stack_find_top() else {
        return false;
    };
    let mut outfile: Output = match filename {
        None => Output::Stdout(std::io::stdout()),
        Some(name) => {
            println!("Writing AT&T file: {}", name);
            match File::create(name) {
                Ok(f) => Output::File(f),
                Err(_) => {
                    eprint!("{}: ", name);
                    perror("File error opening.");
                    return false;
                }
            }
        }
    };
    // C ignored net_print_att's return; a write failure (broken pipe on stdout,
    // disk full on a file) is now reported and turns into a false result.
    if let Err(e) =
        session.stack_entry_fsm_with_opts(top, |opts, f| net_print_att(opts, f, &mut outfile))
    {
        eprintln!("{e}");
        return false;
    }
    // fclose only when filename != NULL; stdout is not closed. Both drop here.
    true
}

// [spec:foma:def:iface.iface-write-prolog-fn]
// [spec:foma:sem:iface.iface-write-prolog-fn]
// [spec:foma:def:foma.iface-write-prolog-fn]
// [spec:foma:sem:foma.iface-write-prolog-fn]
pub fn iface_write_prolog(session: &mut Session, filename: Option<&str>) {
    if iface_stack_check(session, 1) {
        let Some(top) = session.stack_find_top() else {
            return;
        };
        // C printed "Writing prolog to file '…'." from foma_write_prolog itself;
        // that user-facing progress now lives here (the library fn is silent and
        // returns a Result). A file-create failure prints the error instead of
        // C's silent stdout fallback.
        if let Some(name) = filename {
            println!("Writing prolog to file '{name}'.");
        }
        let result = session.stack_entry_fsm(top, |f| foma_write_prolog(f, filename));
        if let Err(e) = result {
            eprintln!("{e}");
        }
    }
}
