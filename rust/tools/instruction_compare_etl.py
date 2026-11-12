"""Totals retired instructions per process id, and per thread in creation order, from a PMC-on-context-switch ETL; prints one JSON object, never names a thread or word.

usage: instruction_compare_etl.py <trace.etl> <pid> [<pid> ...]
"""
import ctypes as C
import ctypes.wintypes as W
import json
import struct
import sys

THREAD_PROVIDER_A = 0x3D6FA8D1
CSWITCH_OPCODE = 36
PMC_EXT_TYPE = 8


class GUID(C.Structure):
    _fields_ = [("a", C.c_uint32), ("b", C.c_uint16), ("c", C.c_uint16), ("d", C.c_ubyte * 8)]


class TZ(C.Structure):
    _fields_ = [("raw", C.c_ubyte * 172)]


class TraceLogfileHeader(C.Structure):
    _fields_ = [("BufferSize", C.c_uint32), ("Version", C.c_uint32), ("ProviderVersion", C.c_uint32), ("NumberOfProcessors", C.c_uint32),
                ("EndTime", C.c_int64), ("TimerResolution", C.c_uint32), ("MaximumFileSize", C.c_uint32), ("LogFileMode", C.c_uint32),
                ("BuffersWritten", C.c_uint32), ("StartBuffers", C.c_uint32), ("PointerSize", C.c_uint32), ("EventsLost", C.c_uint32),
                ("CpuSpeedInMHz", C.c_uint32), ("LoggerName", C.c_void_p), ("LogFileName", C.c_void_p), ("TimeZone", TZ),
                ("BootTime", C.c_int64), ("PerfFreq", C.c_int64), ("StartTime", C.c_int64), ("ReservedFlags", C.c_uint32), ("BuffersLost", C.c_uint32)]


class EventHeader(C.Structure):
    _fields_ = [("Size", C.c_uint16), ("HeaderType", C.c_uint16), ("Flags", C.c_uint16), ("EventProperty", C.c_uint16),
                ("ThreadId", C.c_uint32), ("ProcessId", C.c_uint32), ("TimeStamp", C.c_int64), ("ProviderId", GUID),
                ("Id", C.c_uint16), ("Version", C.c_ubyte), ("Channel", C.c_ubyte), ("Level", C.c_ubyte), ("Opcode", C.c_ubyte),
                ("Task", C.c_uint16), ("Keyword", C.c_uint64), ("ProcessorTime", C.c_uint64), ("ActivityId", GUID)]


class ExtItem(C.Structure):
    _fields_ = [("Reserved1", C.c_uint16), ("ExtType", C.c_uint16), ("Linkage", C.c_uint16), ("DataSize", C.c_uint16), ("DataPtr", C.c_uint64)]


class EventRecord(C.Structure):
    _fields_ = [("EventHeader", EventHeader), ("ProcessorNumber", C.c_ubyte), ("Alignment", C.c_ubyte), ("LoggerId", C.c_uint16),
                ("ExtendedDataCount", C.c_uint16), ("UserDataLength", C.c_uint16), ("ExtendedData", C.POINTER(ExtItem)),
                ("UserData", C.c_void_p), ("UserContext", C.c_void_p)]


CALLBACK = C.WINFUNCTYPE(None, C.POINTER(EventRecord))


class LogFile(C.Structure):
    _fields_ = [("LogFileName", W.LPWSTR), ("LoggerName", W.LPWSTR), ("CurrentTime", C.c_int64), ("BuffersRead", C.c_uint32),
                ("ProcessTraceMode", C.c_uint32), ("CurrentEvent", C.c_ubyte * 88), ("LogfileHeader", TraceLogfileHeader),
                ("BufferCallback", C.c_void_p), ("BufferSize", C.c_uint32), ("Filled", C.c_uint32), ("EventsLost", C.c_uint32),
                ("EventRecordCallback", CALLBACK), ("IsKernelTrace", C.c_uint32), ("Context", C.c_void_p)]


def totals(path, pids):
    wanted = set(pids)
    last, tid_pid = {}, {}
    instructions = {p: 0 for p in pids}
    state = {"switches": 0, "gaps_on_target": 0, "counters": 0, "callback_errors": 0}
    target_tids = set()
    thread_order = {p: [] for p in pids}
    thread_instructions = {}

    def on_event(record):
        try:
            handle_event(record)
        except Exception:
            state["callback_errors"] += 1

    def handle_event(record):
        r = record.contents
        h = r.EventHeader
        if h.ProviderId.a != THREAD_PROVIDER_A:
            return
        if h.Opcode in (1, 3) and r.UserDataLength >= 8:
            pid, tid = struct.unpack("<II", C.string_at(r.UserData, 8))
            tid_pid[tid] = pid
            if pid in wanted:
                target_tids.add(tid)
                if (pid, tid) not in thread_instructions:
                    thread_order[pid].append(tid)
                    thread_instructions[(pid, tid)] = 0
            return
        if h.Opcode != CSWITCH_OPCODE:
            return
        new, old = struct.unpack("<II", C.string_at(r.UserData, 8))
        counters = None
        for i in range(r.ExtendedDataCount):
            e = r.ExtendedData[i]
            if e.ExtType == PMC_EXT_TYPE:
                counters = struct.unpack(f"<{e.DataSize // 8}Q", C.string_at(e.DataPtr, e.DataSize))
        if counters is None:
            return
        state["switches"] += 1
        state["counters"] = len(counters)
        cpu = r.ProcessorNumber
        prev = last.get(cpu)
        last[cpu] = (counters, new)
        if prev is None:
            return
        prev_counters, prev_thread = prev
        if prev_thread != old:
            if old in target_tids or prev_thread in target_tids:
                state["gaps_on_target"] += 1
            return
        pid = tid_pid.get(old)
        if pid in wanted:
            instructions[pid] += counters[0] - prev_counters[0]
            thread_instructions[(pid, old)] = thread_instructions.get((pid, old), 0) + counters[0] - prev_counters[0]

    adv = C.windll.advapi32
    adv.OpenTraceW.restype = C.c_uint64
    adv.OpenTraceW.argtypes = [C.POINTER(LogFile)]
    adv.ProcessTrace.argtypes = [C.POINTER(C.c_uint64), C.c_uint32, C.c_void_p, C.c_void_p]
    adv.CloseTrace.argtypes = [C.c_uint64]
    log = LogFile()
    log.LogFileName = path
    log.ProcessTraceMode = 0x10000000 | 0x00001000
    keep = CALLBACK(on_event)
    log.EventRecordCallback = keep
    handle = adv.OpenTraceW(C.byref(log))
    if handle == 0xFFFFFFFFFFFFFFFF:
        raise SystemExit(f"OpenTrace failed {C.GetLastError()}")
    h = C.c_uint64(handle)
    rc = adv.ProcessTrace(C.byref(h), 1, None, None)
    adv.CloseTrace(handle)
    return {
        "rc": rc,
        "events_lost": log.LogfileHeader.EventsLost,
        "buffers_lost": log.LogfileHeader.BuffersLost,
        "switches": state["switches"],
        "counters": state["counters"],
        "gaps_on_target": state["gaps_on_target"],
        "callback_errors": state["callback_errors"],
        "threads": {str(p): sum(1 for t in target_tids if tid_pid.get(t) == p) for p in pids},
        "instructions": {str(p): v for p, v in instructions.items()},
        "thread_instructions": {str(p): [thread_instructions[(p, t)] for t in thread_order[p]] for p in pids},
    }


if __name__ == "__main__":
    print(json.dumps(totals(sys.argv[1], [int(x) for x in sys.argv[2:]])))
