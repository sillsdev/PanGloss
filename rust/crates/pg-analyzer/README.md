# Segmentation

`analyze` accepts original text, LDML or a version 1 JSON profile, and optional non-overlapping styled or writing-system runs. It returns the chosen profile and original word surfaces with half-open code-point, UTF-8 byte, and UTF-16 code-unit ranges. A run can supply a different profile; Chapter Number and Verse Number exclude every character. The default Unicode category data is version 14.0 and can be regenerated with `tools/generate_unicode.py` from the official [Unicode 14.0.0 UCD `UnicodeData.txt`](https://www.unicode.org/Public/14.0.0/ucd/UnicodeData.txt) (`python3 tools/generate_unicode.py /path/to/UnicodeData.txt`); the generator verifies its SHA-256. LibLCM's pinned ICU 70 corresponds to Unicode 14.0.

The LDML reader accepts main exemplar character sets containing literal characters, scalar escapes (`u`, `U`, and braced `x`), whitespace-delimited ranges, and strings (whose first scalar is the override, as in LibLCM). It rejects unsupported UnicodeSet operations rather than silently dropping them. JSON profiles contain `version`, `source_id`, `source_sha256`, and `word_forming`. A caller without LDML can round-trip the profile returned by an LDML analysis. The hash is over the original UTF-8 LDML bytes.

The WordMaker differential harness and its reference limitations are documented in `rust/dotnet-harness/WordMakerParity/README.md`.
