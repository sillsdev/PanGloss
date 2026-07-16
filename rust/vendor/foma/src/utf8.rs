//! Literal port of foma/utf8.c (Wave 2, bug-for-bug).
//!
//! These functions do byte-level C-string work; they operate on byte
//! buffers (`&[u8]` / `&mut Vec<u8>`) so that the C's byte semantics —
//! including UTF-8-corrupting reversal and CESU-8-like surrogate output —
//! are reproducible. Writing `'\0'` at position i in C corresponds to
//! `truncate(i)` on the buffer; reading the terminating NUL corresponds
//! to reading an implicit 0 at index `len`.

// [spec:foma:def:utf8.escape-string-fn]
// [spec:foma:sem:utf8.escape-string-fn]
// [spec:foma:def:fomalibconf.escape-string-fn]
// [spec:foma:sem:fomalibconf.escape-string-fn]
// Byte semantics preserved (wire compat): a backslash is inserted before
// every occurrence of `chr`. When there is nothing to escape the C returns
// the caller's own pointer; here an owned copy of the input is returned.
pub fn escape_string(string: &[u8], chr: u8) -> Vec<u8> {
    let count = string.iter().filter(|&&b| b == chr).count();
    if count == 0 {
        return string.to_vec();
    }
    let mut out: Vec<u8> = Vec::with_capacity(string.len() + count);
    for &b in string {
        if b == chr {
            out.push(b'\\');
        }
        out.push(b);
    }
    out
}

/* Removes initial and final quote, and decodes the string if it contains special chars */
// [spec:foma:def:utf8.dequote-string-fn]
// [spec:foma:sem:utf8.dequote-string-fn]
// [spec:foma:def:fomalibconf.dequote-string-fn]
// [spec:foma:sem:fomalibconf.dequote-string-fn]
pub fn dequote_string(s: &mut Vec<u8>) {
    let len: i32 = s.len() as i32;
    /* C: *s reads the NUL terminator of an empty string (test fails) */
    if len > 0 && s[0] == 0x22 && s[(len - 1) as usize] == 0x22 {
        let mut i: i32 = 1;
        let mut j: i32 = 0;
        while i < len - 1 {
            s[j as usize] = s[i as usize];
            i += 1;
            j += 1;
        }
        s.truncate(j as usize); /* C: *(s+j) = '\0' */
        decode_quoted(s);
    }
}

/* Decode quoted strings. This includes: */
/* Changing \uXXXX sequences to their unicode equivalents */

// [spec:foma:def:utf8.decode-quoted-fn]
// [spec:foma:sem:utf8.decode-quoted-fn+1]
// [spec:foma:def:fomalibconf.decode-quoted-fn]
// [spec:foma:sem:fomalibconf.decode-quoted-fn+1]
// Wave 4 fix: on a malformed lead byte utf8skip returns -1, so the C copied
// zero bytes and neither cursor advanced (infinite loop). Here the copy is
// forced to advance by at least one byte (lossy), so decoding terminates.
pub fn decode_quoted(s: &mut Vec<u8>) {
    let len: i32 = s.len() as i32;
    let mut i: i32 = 0;
    let mut j: i32 = 0;
    while i < len {
        if s[i as usize] == 0x5c
            && len - i > 5
            && s[(i + 1) as usize] == 0x75
            && is_hex4(&s[(i + 2) as usize..])
        {
            let unistr: Vec<u8> = hex4_to_utf8(&s[(i + 2) as usize..])
                .expect("4 hex digits parse to a codepoint <= 0xFFFF");
            /* C: for (unistr=...; *unistr; j++, unistr++) — copy up to the
            NUL. The \u0000 escape yields the single byte 0, which the
            *unistr test rejects immediately, so the escape is deleted
            from the output (per the sem rule). */
            let mut u: usize = 0;
            while u < unistr.len() && unistr[u] != 0 {
                s[j as usize] = unistr[u];
                j += 1;
                u += 1;
            }
            i += 6;
        } else {
            /* Copy one whole UTF-8 character. utf8skip+1 is its width; a
            malformed lead byte yields 0, which the C looped on forever —
            force at least one byte so both cursors advance (lossy). */
            let mut skip: i32 = (utf8skip(&s[i as usize..]) + 1).max(1);
            while skip > 0 {
                s[j as usize] = s[i as usize];
                i += 1;
                j += 1;
                skip -= 1;
            }
        }
    }
    s.truncate(j as usize); /* C: *(s+j) = *(s+i) copies the terminating NUL */
}

/* Replace equal length substrings in s */
// [spec:foma:def:utf8.streqrep-fn]
// [spec:foma:sem:utf8.streqrep-fn+1]
// [spec:foma:def:fomalibconf.streqrep-fn]
// [spec:foma:sem:fomalibconf.streqrep-fn+1]
// C returns `s`; here the buffer is mutated in place.
// [spec:foma:sem:utf8.streqrep-fn+1] the scan advances past each replacement, so
// it always terminates and replaces non-overlapping occurrences left to right. C
// restarted from the beginning after every replacement, so old == new (or an
// oldstring that still matches after substitution) looped forever. An empty
// oldstring (matches everywhere) and a newstring shorter than oldstring (the C
// memcpy read past its end) are both treated as no-ops rather than hanging/panicking.
pub fn replace_equal_len(s: &mut [u8], oldstring: &[u8], newstring: &[u8]) {
    let len: usize = oldstring.len();
    if len == 0 || newstring.len() < len {
        return;
    }

    let mut start: usize = 0;
    while start + len <= s.len() {
        if &s[start..start + len] == oldstring {
            /* C: memcpy(ptr, newstring, len) */
            s[start..start + len].copy_from_slice(&newstring[..len]);
            start += len; /* advance past the replacement — no re-match, no hang */
        } else {
            start += 1;
        }
    }
}

// [spec:foma:def:utf8.ishexstr-fn]
// [spec:foma:sem:utf8.ishexstr-fn]
// [spec:foma:def:fomalibconf.ishexstr-fn]
// [spec:foma:sem:fomalibconf.ishexstr-fn]
pub fn is_hex4(str: &[u8]) -> bool {
    let mut i: i32 = 0;
    while i < 4 {
        /* C compares (signed) char, so bytes >= 0x80 are negative and fail
        every range; index len stands in for the NUL terminator (itself
        failing, so C never reads past an early NUL either). */
        let c: i32 = if (i as usize) < str.len() {
            str[i as usize] as i8 as i32
        } else {
            0
        };
        if (c > 0x2f && c < 0x3a) || (c > 0x40 && c < 0x47) || (c > 0x60 && c < 0x67) {
            i += 1;
            continue;
        }
        return false;
    }
    true
}

/* Checks if the next character in the string is a combining character     */
/* according to Unicode 7.0                                                */
/* i.e. codepoints 0300-036F  Combining Diacritical Marks                  */
/*                 1AB0-1ABE  Combining Diacritical Marks Extended         */
/*                 1DC0-1DFF  Combining Diacritical Marks Supplement       */
/*                 20D0-20F0  Combining Diacritical Marks for Symbols      */
/*                 FE20-FE2D  Combining Half Marks                         */
/* Returns number of bytes of char. representation, or 0 if not combining  */

// [spec:foma:def:utf8.utf8iscombining-fn]
// [spec:foma:sem:utf8.utf8iscombining-fn]
// [spec:foma:def:fomalibconf.utf8iscombining-fn]
// [spec:foma:sem:fomalibconf.utf8iscombining-fn]
pub fn is_combining(s: &[u8]) -> i32 {
    /* Index >= len stands in for the C string's NUL terminator */
    let s0: u8 = if !s.is_empty() { s[0] } else { 0 };
    let s1: u8 = if s.len() > 1 { s[1] } else { 0 };
    if s0 == 0 || s1 == 0 {
        return 0;
    }
    if !(s0 == 0xcc || s0 == 0xcd || s0 == 0xe1 || s0 == 0xe2 || s0 == 0xef) {
        return 0;
    }
    /* 0300-036F */
    if s0 == 0xcc && (0x80..=0xbf).contains(&s1) {
        return 2;
    }
    if s0 == 0xcd && (0x80..=0xaf).contains(&s1) {
        return 2;
    }
    let s2: u8 = if s.len() > 2 { s[2] } else { 0 };
    if s2 == 0 {
        return 0;
    }
    /* 1AB0-1ABE */
    if s0 == 0xe1 && s1 == 0xaa && (0xb0..=0xbe).contains(&s2) {
        return 3;
    }
    /* 1DC0-1DFF */
    if s0 == 0xe1 && s1 == 0xb7 && (0x80..=0xbf).contains(&s2) {
        return 3;
    }
    /* 20D0-20F0 */
    if s0 == 0xe2 && s1 == 0x83 && (0x90..=0xb0).contains(&s2) {
        return 3;
    }
    /* FE20-FE2D */
    if s0 == 0xef && s1 == 0xb8 && (0xa0..=0xad).contains(&s2) {
        return 3;
    }
    0
}

// [spec:foma:def:utf8.utf8skip-fn]
// [spec:foma:sem:utf8.utf8skip-fn]
// [spec:foma:def:fomalibconf.utf8skip-fn]
// [spec:foma:sem:fomalibconf.utf8skip-fn]
pub fn utf8skip(str: &[u8]) -> i32 {
    /* C: s = (unsigned char)(unsigned int) (*str) — an empty slice stands
    in for pointing at the NUL terminator (byte 0, classified as ASCII). */
    let s: u8 = if !str.is_empty() { str[0] } else { 0 };
    if s < 0x80 {
        return 0;
    }
    if (s & 0xe0) == 0xc0 {
        return 1;
    }
    if (s & 0xf0) == 0xe0 {
        return 2;
    }
    if (s & 0xf8) == 0xf0 {
        return 3;
    }
    -1
}

// [spec:foma:def:utf8.utf8code16tostr-fn]
// [spec:foma:sem:utf8.utf8code16tostr-fn]
// [spec:foma:def:fomalibconf.utf8code16tostr-fn]
// [spec:foma:sem:fomalibconf.utf8code16tostr-fn]
pub fn hex4_to_utf8(str: &[u8]) -> Option<Vec<u8>> {
    let codepoint: i32 = (parse_hex_byte(str) << 8) + parse_hex_byte(&str[2..]);
    codepoint_to_utf8(codepoint)
}

// [spec:foma:def:utf8.int2utf8str-fn]
// [spec:foma:sem:utf8.int2utf8str-fn]
// Codepoints >= 0x10000 (no 4-byte support) return None; every other value
// yields the encoded bytes without a trailing NUL (codepoint 0 yields the
// single byte 0, which consumers treating the result as a C string see as
// empty). Negative codepoints fall into the < 0x80 branch (truncated byte).
pub fn codepoint_to_utf8(codepoint: i32) -> Option<Vec<u8>> {
    let mut value: Vec<u8> = Vec::with_capacity(5);

    if codepoint < 0x80 {
        /* Negative codepoints fall in here and produce a garbage
        (truncated) byte, as in C */
        value.push(codepoint as u8);
        Some(value)
    } else if codepoint < 0x800 {
        value.push(0xc0 | ((codepoint >> 6) as u8));
        value.push(0x80 | ((codepoint & 0x3f) as u8));
        Some(value)
    } else if codepoint < 0x10000 {
        value.push(0xe0 | ((codepoint >> 12) as u8));
        value.push(0x80 | (((codepoint >> 6) & 0x3f) as u8));
        value.push(0x80 | ((codepoint & 0x3f) as u8));
        Some(value)
    } else {
        None
    }
}

// [spec:foma:def:utf8.hexstrtoint-fn]
// [spec:foma:sem:utf8.hexstrtoint-fn]
// C: file-static. No validation — callers must pre-check with ishexstr;
// garbage in gives garbage out.
pub(crate) fn parse_hex_byte(str: &[u8]) -> i32 {
    let mut hex: i32;

    /* C reads (signed) char values; reproduce with `as i8 as i32` */
    let c0: i32 = str[0] as i8 as i32;
    let c1: i32 = str[1] as i8 as i32;
    if c0 > 0x60 {
        hex = (c0 - 0x57) << 4;
    } else if c0 > 0x40 {
        hex = (c0 - 0x37) << 4;
    } else {
        hex = (c0 - 0x30) << 4;
    }
    if c1 > 0x60 {
        hex += c1 - 0x57;
    } else if c1 > 0x40 {
        hex += c1 - 0x37;
    } else {
        hex += c1 - 0x30;
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    // utf8skip: lead-byte classification, empty slice → 0 (NUL), stray
    // continuation bytes and invalid 0xF8-0xFF → -1.
    // [spec:foma:sem:utf8.utf8skip-fn/test]
    // [spec:foma:sem:fomalibconf.utf8skip-fn/test]
    #[test]
    fn test_utf8skip() {
        assert_eq!(utf8skip(&[]), 0); // empty stands in for the NUL (byte 0, ASCII)
        assert_eq!(utf8skip(&[0x00]), 0);
        assert_eq!(utf8skip(&[0x41]), 0); // 'A'
        assert_eq!(utf8skip(&[0x7f]), 0); // last ASCII
        // continuation bytes 0x80-0xBF are stray leads → -1
        assert_eq!(utf8skip(&[0x80]), -1);
        assert_eq!(utf8skip(&[0xbf]), -1);
        // 2-byte leads 0xC0-0xDF → 1
        assert_eq!(utf8skip(&[0xc0]), 1);
        assert_eq!(utf8skip(&[0xc3]), 1);
        assert_eq!(utf8skip(&[0xdf]), 1);
        // 3-byte leads 0xE0-0xEF → 2
        assert_eq!(utf8skip(&[0xe0]), 2);
        assert_eq!(utf8skip(&[0xef]), 2);
        // 4-byte leads 0xF0-0xF7 → 3
        assert_eq!(utf8skip(&[0xf0]), 3);
        assert_eq!(utf8skip(&[0xf7]), 3);
        // invalid 0xF8-0xFF → -1
        assert_eq!(utf8skip(&[0xf8]), -1);
        assert_eq!(utf8skip(&[0xff]), -1);
    }

    // utf8iscombining: exact range boundaries per Unicode 7.0; fast reject,
    // NUL guards, 2-byte (return 2) and 3-byte (return 3) blocks.
    // [spec:foma:sem:utf8.utf8iscombining-fn/test]
    // [spec:foma:sem:fomalibconf.utf8iscombining-fn/test]
    #[test]
    fn test_is_combining() {
        // NUL / short guards
        assert_eq!(is_combining(&[]), 0);
        assert_eq!(is_combining(&[0xcc]), 0); // s1 == 0 (NUL)
        // fast reject: lead not in {CC,CD,E1,E2,EF}
        assert_eq!(is_combining(b"ab"), 0);
        assert_eq!(is_combining(&[0xce, 0x80]), 0);
        // U+0300-036F: 0xCC + 0x80..=0xBF → 2
        assert_eq!(is_combining(&[0xcc, 0x80]), 2);
        assert_eq!(is_combining(&[0xcc, 0xbf]), 2);
        assert_eq!(is_combining(&[0xcc, 0x7f]), 0); // below range
        assert_eq!(is_combining(&[0xcc, 0xc0]), 0); // above range
        // 0xCD + 0x80..=0xAF → 2
        assert_eq!(is_combining(&[0xcd, 0x80]), 2);
        assert_eq!(is_combining(&[0xcd, 0xaf]), 2);
        assert_eq!(is_combining(&[0xcd, 0xb0]), 0); // above range
        // U+1AB0-1ABE: 0xE1 0xAA + 0xB0..=0xBE → 3
        assert_eq!(is_combining(&[0xe1, 0xaa, 0xb0]), 3);
        assert_eq!(is_combining(&[0xe1, 0xaa, 0xbe]), 3);
        assert_eq!(is_combining(&[0xe1, 0xaa, 0xaf]), 0);
        assert_eq!(is_combining(&[0xe1, 0xaa, 0xbf]), 0);
        // U+1DC0-1DFF: 0xE1 0xB7 + 0x80..=0xBF → 3
        assert_eq!(is_combining(&[0xe1, 0xb7, 0x80]), 3);
        assert_eq!(is_combining(&[0xe1, 0xb7, 0xbf]), 3);
        // U+20D0-20F0: 0xE2 0x83 + 0x90..=0xB0 → 3
        assert_eq!(is_combining(&[0xe2, 0x83, 0x90]), 3);
        assert_eq!(is_combining(&[0xe2, 0x83, 0xb0]), 3);
        assert_eq!(is_combining(&[0xe2, 0x83, 0x8f]), 0);
        assert_eq!(is_combining(&[0xe2, 0x83, 0xb1]), 0);
        // U+FE20-FE2D: 0xEF 0xB8 + 0xA0..=0xAD → 3
        assert_eq!(is_combining(&[0xef, 0xb8, 0xa0]), 3);
        assert_eq!(is_combining(&[0xef, 0xb8, 0xad]), 3);
        assert_eq!(is_combining(&[0xef, 0xb8, 0x9f]), 0);
        assert_eq!(is_combining(&[0xef, 0xb8, 0xae]), 0);
        // three-byte lead but s2 == 0 (NUL) → 0
        assert_eq!(is_combining(&[0xe1, 0xaa]), 0);
    }

    // escape_string: backslash-escapes each `chr`; identity (owned copy) when
    // no occurrences; the escape char can itself be escaped.
    // [spec:foma:sem:utf8.escape-string-fn/test]
    // [spec:foma:sem:fomalibconf.escape-string-fn/test]
    #[test]
    fn test_escape_string() {
        // identity path (j == 0): content preserved
        assert_eq!(escape_string(b"abc", b'"'), b"abc".to_vec());
        // single occurrence: a " c → a \ " c
        assert_eq!(
            escape_string(&[0x61, 0x22, 0x63], b'"'),
            vec![0x61, 0x5c, 0x22, 0x63]
        );
        // two occurrences: a " b " c
        assert_eq!(
            escape_string(&[0x61, 0x22, 0x62, 0x22, 0x63], b'"'),
            vec![0x61, 0x5c, 0x22, 0x62, 0x5c, 0x22, 0x63]
        );
        // escaping the backslash itself: a \ b → a \ \ b
        assert_eq!(
            escape_string(&[0x61, 0x5c, 0x62], b'\\'),
            vec![0x61, 0x5c, 0x5c, 0x62]
        );
    }

    // dequote_string: strips matching leading/trailing '"' then decodes; lone
    // '"' (same byte satisfies both tests) → empty; non-quoted → untouched.
    // [spec:foma:sem:utf8.dequote-string-fn/test]
    // [spec:foma:sem:fomalibconf.dequote-string-fn/test]
    #[test]
    fn test_dequote_string() {
        // not quoted on both ends → untouched
        let mut s = b"abc".to_vec();
        dequote_string(&mut s);
        assert_eq!(s, b"abc".to_vec());
        // "abc" → abc
        let mut s = vec![0x22, 0x61, 0x62, 0x63, 0x22];
        dequote_string(&mut s);
        assert_eq!(s, b"abc".to_vec());
        // lone '"' → empty (same byte is both first and last)
        let mut s = vec![0x22];
        dequote_string(&mut s);
        assert_eq!(s, Vec::<u8>::new());
        // empty stays empty (len == 0 fails the *s NUL read)
        let mut s = Vec::<u8>::new();
        dequote_string(&mut s);
        assert_eq!(s, Vec::<u8>::new());
        // interior \u escape is decoded: "A" → A
        let mut s = vec![0x22, 0x5c, 0x75, 0x30, 0x30, 0x34, 0x31, 0x22];
        dequote_string(&mut s);
        assert_eq!(s, vec![0x41]);
    }

    // decode_quoted: backslash-u XXXX decoding, U+0000 deletion, surrogate CESU-8,
    // and the ">5 bytes remaining" gate. Wave 4 fix — a malformed lead byte
    // (utf8skip == -1) no longer hangs; it is copied through one byte (lossy)
    // and decoding terminates.
    // [spec:foma:sem:utf8.decode-quoted-fn+1/test]
    // [spec:foma:sem:fomalibconf.decode-quoted-fn+1/test]
    #[test]
    fn test_decode_quoted() {
        // no escapes → unchanged
        let mut s = b"hello".to_vec();
        decode_quoted(&mut s);
        assert_eq!(s, b"hello".to_vec());
        // A → A
        let mut s = vec![0x5c, 0x75, 0x30, 0x30, 0x34, 0x31];
        decode_quoted(&mut s);
        assert_eq!(s, vec![0x41]);
        // mixed: a A b → aAb
        let mut s = vec![0x61, 0x5c, 0x75, 0x30, 0x30, 0x34, 0x31, 0x62];
        decode_quoted(&mut s);
        assert_eq!(s, vec![0x61, 0x41, 0x62]);
        // U+0000 escape -> deleted (leading NUL terminates conversion)
        let mut s = vec![0x5c, 0x75, 0x30, 0x30, 0x30, 0x30];
        decode_quoted(&mut s);
        assert_eq!(s, Vec::<u8>::new());
        // surrogate half \uD800 → CESU-8-like 3 bytes, encoded as-is
        let mut s = vec![0x5c, 0x75, 0x44, 0x38, 0x30, 0x30];
        decode_quoted(&mut s);
        assert_eq!(s, vec![0xed, 0xa0, 0x80]);
        // only 5 bytes remain (len-i == 5, not > 5) → NOT decoded, copied raw
        let mut s = vec![0x5c, 0x75, 0x30, 0x34, 0x31];
        decode_quoted(&mut s);
        assert_eq!(s, vec![0x5c, 0x75, 0x30, 0x34, 0x31]);
        // Wave 4: a malformed lead byte no longer hangs — copied through one
        // byte (lossy advance) and decoding terminates.
        let mut s = vec![0xff, 0x41];
        decode_quoted(&mut s);
        assert_eq!(s, vec![0xff, 0x41]);
    }

    // streqrep: equal-length replacement of every match; no-match identity.
    // [spec:foma:sem:utf8.streqrep-fn+1/test]
    // [spec:foma:sem:fomalibconf.streqrep-fn+1/test]
    #[test]
    fn test_replace_equal_len() {
        // no match → identity
        let mut s = b"hello".to_vec();
        replace_equal_len(&mut s, b"xy", b"za");
        assert_eq!(s, b"hello".to_vec());
        // single replacement
        let mut s = b"cat".to_vec();
        replace_equal_len(&mut s, b"a", b"o");
        assert_eq!(s, b"cot".to_vec());
        // all occurrences replaced (new text does not re-match old)
        let mut s = b"abcabc".to_vec();
        replace_equal_len(&mut s, b"bc", b"XY");
        assert_eq!(s, b"aXYaXY".to_vec());
    }

    // The scan advances past each replacement, so cases that made the C loop
    // never terminate (or read past newstring) all halt safely.
    // [spec:foma:sem:utf8.streqrep-fn+1/test]
    // [spec:foma:sem:fomalibconf.streqrep-fn+1/test]
    #[test]
    fn replace_equal_len_always_terminates() {
        // old == new: every 'a' rewritten to 'a' — must terminate, not hang.
        let mut s = b"banana".to_vec();
        replace_equal_len(&mut s, b"a", b"a");
        assert_eq!(s, b"banana".to_vec());
        // empty oldstring matches everywhere: treated as a no-op.
        let mut s = b"xyz".to_vec();
        replace_equal_len(&mut s, b"", b"q");
        assert_eq!(s, b"xyz".to_vec());
        // newstring shorter than oldstring: no-op (C read past newstring's end).
        let mut s = b"abcabc".to_vec();
        replace_equal_len(&mut s, b"ab", b"x");
        assert_eq!(s, b"abcabc".to_vec());
    }

    // ishexstr: 1 iff four bytes are ASCII hex digits; short input fails at the
    // stand-in NUL; signed-char compare rejects bytes >= 0x80.
    // [spec:foma:sem:utf8.ishexstr-fn/test]
    // [spec:foma:sem:fomalibconf.ishexstr-fn/test]
    #[test]
    fn test_is_hex4() {
        assert!(is_hex4(b"0041"));
        assert!(is_hex4(b"DEAD"));
        assert!(is_hex4(b"beef"));
        assert!(is_hex4(b"09af"));
        // boundaries just outside each range
        assert!(!(is_hex4(b"123:"))); // ':' == 0x3a, above '9'
        assert!(!(is_hex4(b"12G4"))); // 'G' == 0x47, above 'F'
        assert!(!(is_hex4(b"aaag"))); // 'g' == 0x67, above 'f'
        assert!(!(is_hex4(b"aa`a"))); // '`' == 0x60, below 'a'
        // short input: index >= len stands in for NUL (byte 0) → fail
        assert!(!(is_hex4(b"12")));
        // byte >= 0x80 is negative as signed char → fails
        assert!(!(is_hex4(&[0x30, 0x30, 0x30, 0xff])));
    }

    // hexstrtoint: two hex bytes → 0..255; signed-char sign-extension yields
    // garbage (negative) results for bytes >= 0x80.
    // [spec:foma:sem:utf8.hexstrtoint-fn/test]
    #[test]
    fn test_parse_hex_byte() {
        assert_eq!(parse_hex_byte(b"41"), 0x41);
        assert_eq!(parse_hex_byte(b"ff"), 0xff);
        assert_eq!(parse_hex_byte(b"FF"), 0xff);
        assert_eq!(parse_hex_byte(b"A0"), 0xa0);
        assert_eq!(parse_hex_byte(b"00"), 0x00);
        // signed-char: 0x80 as i8 == -128 → (-128 - 0x30) << 4 == -2816
        assert_eq!(parse_hex_byte(&[0x80, 0x30]), -2816);
    }

    // int2utf8str: 1/2/3-byte encodings; None for codepoints >= 0x10000;
    // negative codepoints fall into the <0x80 branch producing a garbage byte.
    // [spec:foma:sem:utf8.int2utf8str-fn/test]
    #[test]
    fn test_codepoint_to_utf8() {
        assert_eq!(codepoint_to_utf8(0x41), Some(vec![0x41]));
        assert_eq!(codepoint_to_utf8(0x00), Some(vec![0x00]));
        assert_eq!(codepoint_to_utf8(0x7f), Some(vec![0x7f]));
        // 2-byte range boundaries
        assert_eq!(codepoint_to_utf8(0x80), Some(vec![0xc2, 0x80]));
        assert_eq!(codepoint_to_utf8(0xe9), Some(vec![0xc3, 0xa9])); // é
        assert_eq!(codepoint_to_utf8(0x7ff), Some(vec![0xdf, 0xbf]));
        // 3-byte range boundaries
        assert_eq!(codepoint_to_utf8(0x800), Some(vec![0xe0, 0xa0, 0x80]));
        assert_eq!(codepoint_to_utf8(0x20ac), Some(vec![0xe2, 0x82, 0xac])); // €
        assert_eq!(codepoint_to_utf8(0xffff), Some(vec![0xef, 0xbf, 0xbf]));
        // >= 0x10000 → None (no 4-byte support)
        assert_eq!(codepoint_to_utf8(0x10000), None);
        assert_eq!(codepoint_to_utf8(0x10348), None);
        // negative → <0x80 branch, truncated garbage byte
        assert_eq!(codepoint_to_utf8(-1), Some(vec![0xff]));
    }

    // utf8code16tostr: four hex digits → BMP codepoint → UTF-8; surrogates are
    // NOT special-cased (CESU-8-like output); never None (max 0xFFFF < 0x10000).
    // [spec:foma:sem:utf8.utf8code16tostr-fn/test]
    // [spec:foma:sem:fomalibconf.utf8code16tostr-fn/test]
    #[test]
    fn test_hex4_to_utf8() {
        assert_eq!(hex4_to_utf8(b"0041"), Some(vec![0x41])); // 'A'
        assert_eq!(hex4_to_utf8(b"00E9"), Some(vec![0xc3, 0xa9])); // é
        assert_eq!(hex4_to_utf8(b"20AC"), Some(vec![0xe2, 0x82, 0xac])); // €
        assert_eq!(hex4_to_utf8(b"FFFF"), Some(vec![0xef, 0xbf, 0xbf]));
        // surrogate half encoded as-is (CESU-8-like), not rejected
        assert_eq!(hex4_to_utf8(b"D800"), Some(vec![0xed, 0xa0, 0x80]));
    }
}
