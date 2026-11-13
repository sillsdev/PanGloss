"""Totals retired instructions per process id (its last process in the trace), per thread in creation order, and for the parse phase that `batch --phase-marks` delimits, from a PMC-on-context-switch ETL; prints one JSON object, never names a thread or word.

usage: instruction_compare_etl.py <trace.etl> <pid> [<pid> ...]
"""
import ctypes as C
import ctypes.wintypes as W
import json
import struct
import sys

PROCESS_PROVIDER_A = 0x3D6FA8D0
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


# WINFUNCTYPE exists only on Windows; Tally is plain Python, so tests can import this module elsewhere.
CALLBACK = C.WINFUNCTYPE(None, C.POINTER(EventRecord)) if hasattr(C, "WINFUNCTYPE") else C.c_void_p


class LogFile(C.Structure):
    _fields_ = [("LogFileName", W.LPWSTR), ("LoggerName", W.LPWSTR), ("CurrentTime", C.c_int64), ("BuffersRead", C.c_uint32),
                ("ProcessTraceMode", C.c_uint32), ("CurrentEvent", C.c_ubyte * 88), ("LogfileHeader", TraceLogfileHeader),
                ("BufferCallback", C.c_void_p), ("BufferSize", C.c_uint32), ("Filled", C.c_uint32), ("EventsLost", C.c_uint32),
                ("EventRecordCallback", CALLBACK), ("IsKernelTrace", C.c_uint32), ("Context", C.c_void_p)]


class Tally:
    """Per-thread instruction totals from context-switch counter snapshots, kept for the last process that held each wanted pid.

    Windows reuses a pid once no handle to its process is open, so an earlier process in the trace can share a measured pid.
    instruction-compare.ps1 and pmc-setup.ps1 hold each run's handle until the trace stops, so a run is its pid's last process.
    A context-switch gap counts against a process only when that process turns out to be its pid's last one.

    `batch --phase-marks` starts and joins one empty thread before the first word and one after the last, so they are the last
    two threads the process creates. The instructions its other threads run between the first marker's exit and the second
    marker's first switch-in are its parse phase (`parse_instructions`), counted when the trace is reported.
    """

    def __init__(self, pids):
        self.wanted = set(pids)
        self.pids = list(pids)
        self.last = {}  # cpu -> (counters, thread switched in)
        self.tid_owner = {}  # tid -> (pid, generation) of the thread that last started with that tid
        self.generation = {p: 0 for p in pids}  # process starts seen for each wanted pid
        self.thread_order = {}  # (pid, generation) -> tids in creation order
        self.thread_instructions = {}  # (pid, generation, tid) -> instructions
        self.switches = {}  # (pid, generation) -> (thread out, thread in, its instructions since it came in or None after a gap), in order
        self.gaps = []  # per gap, the measured owners (pid, generation) of the threads it touched
        self.state = {"switches": 0, "gaps_on_target": 0, "counters": 0, "callback_errors": 0, "earlier_processes": 0}

    def process_start(self, pid):
        if pid in self.wanted:
            self.generation[pid] += 1

    def thread_start(self, pid, tid):
        owner = (pid, self.generation.get(pid, 0))
        self.tid_owner[tid] = owner
        if pid in self.wanted and owner + (tid,) not in self.thread_instructions:
            self.thread_order.setdefault(owner, []).append(tid)
            self.thread_instructions[owner + (tid,)] = 0

    def _measured_owner(self, tid):
        owner = self.tid_owner.get(tid)
        return owner if owner is not None and owner[0] in self.wanted else None

    def cswitch(self, cpu, new, old, counters):
        self.state["switches"] += 1
        self.state["counters"] = len(counters)
        prev = self.last.get(cpu)
        self.last[cpu] = (counters, new)
        if prev is None:
            return
        old_owner = self._measured_owner(old)
        new_owner = self._measured_owner(new)
        delta = None
        if prev[1] == old:
            if old_owner is not None:
                delta = counters[0] - prev[0][0]
                self.thread_instructions[old_owner + (old,)] += delta
        else:
            touched = tuple(o for o in {old_owner, self._measured_owner(prev[1])} if o is not None)
            if touched:
                self.gaps.append(touched)
        for owner in {old_owner, new_owner} - {None}:
            self.switches.setdefault(owner, []).append((old, new, delta))

    def _parse_instructions(self, owner):
        order = self.thread_order.get(owner, [])
        if len(order) < 3:
            return 0, 0
        begin, end = order[-2], order[-1]
        phase = 0  # 0 before the begin marker has run, 1 in the parse, 2 once the end marker runs
        came_in_phase = {}
        total = 0
        for old, new, delta in self.switches.get(owner, []):
            if delta is not None and came_in_phase.get(old) == 1 and old not in (begin, end):
                total += delta
            if old == begin and phase == 0:
                phase = 1
            if new == end and phase == 1:
                phase = 2
            came_in_phase[new] = phase
        return total, phase

    def report(self):
        final = {p: (p, self.generation[p]) for p in self.pids}
        self.state["earlier_processes"] = len({(pid, gen) for (pid, gen, _tid) in self.thread_instructions if (pid, gen) != final[pid]})
        self.state["gaps_on_target"] = sum(1 for owners in self.gaps if any(o == final[o[0]] for o in owners))
        order = {p: self.thread_order.get(final[p], []) for p in self.pids}
        per_thread = {p: [self.thread_instructions[final[p] + (t,)] for t in order[p]] for p in self.pids}
        parse = {p: self._parse_instructions(final[p]) for p in self.pids}
        return dict(self.state, **{
            "threads": {str(p): len(order[p]) for p in self.pids},
            "instructions": {str(p): sum(per_thread[p]) for p in self.pids},
            "thread_instructions": {str(p): per_thread[p] for p in self.pids},
            "parse_instructions": {str(p): parse[p][0] for p in self.pids},
            "phase": {str(p): parse[p][1] for p in self.pids},
        })


def totals(path, pids):
    tally = Tally(pids)

    def on_event(record):
        try:
            handle_event(record)
        except Exception:
            tally.state["callback_errors"] += 1

    def handle_event(record):
        r = record.contents
        h = r.EventHeader
        if h.ProviderId.a == PROCESS_PROVIDER_A:
            # Process start and rundown: UniqueProcessKey (8 bytes on x64), then ProcessId.
            if h.Opcode in (1, 3) and r.UserDataLength >= 12:
                (pid,) = struct.unpack("<I", C.string_at(r.UserData + 8, 4))
                tally.process_start(pid)
            return
        if h.ProviderId.a != THREAD_PROVIDER_A:
            return
        if h.Opcode in (1, 3) and r.UserDataLength >= 8:
            pid, tid = struct.unpack("<II", C.string_at(r.UserData, 8))
            tally.thread_start(pid, tid)
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
        tally.cswitch(r.ProcessorNumber, new, old, counters)

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
    return dict(tally.report(), rc=rc, events_lost=log.LogfileHeader.EventsLost, buffers_lost=log.LogfileHeader.BuffersLost)


if __name__ == "__main__":
    print(json.dumps(totals(sys.argv[1], [int(x) for x in sys.argv[2:]])))
