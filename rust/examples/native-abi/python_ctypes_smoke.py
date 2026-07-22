#!/usr/bin/env python3
"""Minimal ctypes smoke for PanGloss's length-delimited JSON ABI."""

import ctypes
import json
import pathlib
import sys


class Buffer(ctypes.Structure):
    _fields_ = [("data", ctypes.POINTER(ctypes.c_uint8)), ("len", ctypes.c_size_t), ("cap", ctypes.c_size_t)]


class Error(ctypes.Structure):
    _fields_ = [("code", ctypes.c_int32), ("_pad", ctypes.c_int32), ("message", Buffer)]


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: python_ctypes_smoke.py LIBRARY GRAMMAR_XML")
    lib = ctypes.CDLL(str(pathlib.Path(sys.argv[1]).resolve()))
    grammar = pathlib.Path(sys.argv[2]).read_bytes()
    handle = ctypes.c_void_p()
    error = Error()

    lib.hc_grammar_load.argtypes = [ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(Error)]
    lib.hc_grammar_load.restype = ctypes.c_int32
    lib.hc_grammar_free.argtypes = [ctypes.c_void_p]
    lib.hc_buf_free.argtypes = [ctypes.POINTER(Buffer)]
    source = (ctypes.c_uint8 * len(grammar)).from_buffer_copy(grammar)
    code = lib.hc_grammar_load(source, len(grammar), ctypes.byref(handle), ctypes.byref(error))
    if code != 0:
        message = ctypes.string_at(error.message.data, error.message.len).decode() if error.message.data else ""
        if error.message.data:
            lib.hc_buf_free(ctypes.byref(error.message))
        raise RuntimeError(f"grammar load failed ({code}): {message}")
    if error.message.data:
        lib.hc_buf_free(ctypes.byref(error.message))

    def call(name: str, request: object) -> object:
        fn = getattr(lib, name)
        fn.argtypes = [ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint8), ctypes.c_size_t, ctypes.POINTER(Buffer)]
        fn.restype = ctypes.c_int32
        encoded = json.dumps(request, separators=(",", ":")).encode()
        data = (ctypes.c_uint8 * len(encoded)).from_buffer_copy(encoded)
        out = Buffer()
        status = fn(handle, data, len(encoded), ctypes.byref(out))
        if status != 0 or not out.data:
            raise RuntimeError(f"{name} transport failure: {status}")
        try:
            envelope = json.loads(ctypes.string_at(out.data, out.len))
        finally:
            lib.hc_buf_free(ctypes.byref(out))
        return envelope

    try:
        catalog = call("hc_lexicon_catalog_json", {})
        signature = catalog["value"]["signatures"][0]["id"]
        gloss = call("hc_lexicon_set_gloss_language_json", {"glossLanguage": "en"})
        added = call("hc_lexicon_add_json", {
            "stem": "nupa", "gloss": "host smoke", "signatures": [signature],
            "expectedRevision": gloss["value"]["revision"],
        })
        entry_id = added["value"]["value"]["id"]
        analysis = call("hc_analyze_word_json", {"word": "nupasi"})
        assert any(item["provenance"].get("entryId") == entry_id for item in analysis["value"]["structured"])
        exported = call("hc_lexicon_export_json", {})["value"]
        removed = call("hc_lexicon_remove_json", {"id": entry_id, "expectedRevision": added["value"]["revision"]})
        assert removed["value"]["value"] is True
        assert not any(item["provenance"].get("entryId") == entry_id for item in call("hc_analyze_word_json", {"word": "nupasi"})["value"]["structured"])
        restored = call("hc_lexicon_import_json", {"document": exported})
        assert restored["value"]["changed"] is True
        assert any(item["provenance"].get("entryId") == entry_id for item in call("hc_analyze_word_json", {"word": "nupasi"})["value"]["structured"])
        conflict = call("hc_lexicon_update_json", {
            "id": entry_id, "stem": "nupa", "gloss": "changed", "signatures": [signature],
            "expectedRevision": "rev_0",
        })
        assert conflict["ok"] is False and conflict["error"]["code"] == "revision_conflict"
    finally:
        lib.hc_grammar_free(handle)


if __name__ == "__main__":
    main()
