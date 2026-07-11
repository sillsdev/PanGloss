// .NET 10 P/Invoke harness for hc-ffi's `hermit_crab.dll` (plan §4.2, M8). Proves the C ABI is
// genuinely callable from managed code with real P/Invoke marshalling — struct layout, calling
// convention, UTF-8 string handling — not just from Rust's own test harness. See the .csproj for
// why this stands in for the (unreachable on this machine) net48 FieldWorks-style host.
//
// Usage: HcFfiHarness <path-to-hermit_crab.dll> <indonesian-hc.xml> <indonesian-words.txt>

using System.Runtime.InteropServices;
using System.Text;

if (args.Length < 3)
{
    Console.Error.WriteLine("usage: HcFfiHarness <hermit_crab.dll path> <grammar.xml> <words.txt>");
    return 1;
}

string dllPath = Path.GetFullPath(args[0]);
string grammarPath = args[1];
string wordsPath = args[2];

// Load the native library from an explicit path rather than relying on PATH/probing — mirrors
// plan §4.2's note that net48 needs an explicit LoadLibrary-style resolution; NativeLibrary.Load
// with a full path is the .NET 10 equivalent and keeps this harness independent of CWD/PATH.
NativeLibrary.SetDllImportResolver(typeof(Native).Assembly, (name, assembly, searchPath) =>
    name == Native.LibraryName ? NativeLibrary.Load(dllPath) : IntPtr.Zero);

Console.WriteLine($"[harness] loading native library: {dllPath}");
int abiVersion = Native.hc_abi_version();
Console.WriteLine($"[harness] hc_abi_version() = {abiVersion}");

byte[] xmlBytes = File.ReadAllBytes(grammarPath);
string[] words = File.ReadAllLines(wordsPath).Select(l => l.Trim()).Where(l => l.Length > 0).ToArray();
Console.WriteLine($"[harness] grammar: {grammarPath} ({xmlBytes.Length} bytes); words: {words.Length}");

IntPtr handle;
Native.HcError err;
unsafe
{
    fixed (byte* xmlPtr = xmlBytes)
    {
        int code = Native.hc_grammar_load(xmlPtr, (UIntPtr)xmlBytes.Length, out handle, out err);
        if (code != 0)
        {
            Console.Error.WriteLine($"[harness] hc_grammar_load FAILED: code={code}, message={Native.ReadMessage(err.Message)}");
            Native.hc_buf_free(ref err.Message);
            return 1;
        }
    }
}
Native.hc_buf_free(ref err.Message); // HcResultBuf::EMPTY on success — hc_buf_free must no-op cleanly
Console.WriteLine($"[harness] hc_grammar_load OK, handle=0x{handle:X}");

// --- Error-path sanity check: deliberately malformed UTF-8 must round-trip a real error message
// through the nested HcError.Message struct, proving the nested-struct marshalling (not just the
// top-level HcResultBuf) is laid out correctly. ---
{
    byte[] badUtf8 = { 0xFF, 0xFE, 0x00 };
    unsafe
    {
        fixed (byte* p = badUtf8)
        {
            int code = Native.hc_grammar_load(p, (UIntPtr)badUtf8.Length, out _, out var badErr);
            string msg = Native.ReadMessage(badErr.Message);
            Console.WriteLine($"[harness] deliberate bad-UTF-8 grammar load: code={code}, message={msg}");
            if (code == 0 || msg.Length == 0)
            {
                Console.Error.WriteLine("[harness] expected a nonzero code and a non-empty message for invalid UTF-8 input");
                Native.hc_buf_free(ref badErr.Message);
                return 1;
            }
            Native.hc_buf_free(ref badErr.Message);
        }
    }
}

// --- hc_parse_word: one call per word, on the harness's own thread. ---
int wordsWithAnalyses = 0, totalAnalyses = 0, cappedWords = 0, invalidShapeWords = 0;
byte[][] wordBytesUtf8 = words.Select(Encoding.UTF8.GetBytes).ToArray();

foreach (byte[] wb in wordBytesUtf8)
{
    unsafe
    {
        fixed (byte* wp = wb)
        {
            int code = Native.hc_parse_word(handle, wp, (UIntPtr)wb.Length, out Native.HcResultBuf outBuf);
            if (code != 0)
            {
                Console.Error.WriteLine($"[harness] hc_parse_word FAILED: code={code}");
                return 1;
            }
            var decoded = Native.DecodeBuffer(outBuf);
            Native.hc_buf_free(ref outBuf);
            var w = decoded.Single();
            if (w.InvalidShape) invalidShapeWords++;
            if (w.Capped) cappedWords++;
            if (w.Analyses.Count > 0) wordsWithAnalyses++;
            totalAnalyses += w.Analyses.Count;
        }
    }
}
Console.WriteLine(
    $"[harness] hc_parse_word x{words.Length}: {wordsWithAnalyses} words with >=1 analysis, " +
    $"{totalAnalyses} total analyses, {cappedWords} capped, {invalidShapeWords} invalid-shape");

// --- hc_parse_batch: one call for the whole corpus, 4 threads — exercises the ABI's batch
// marshalling (an array of HcStr, each an independent (ptr,len) into pinned managed memory). ---
GCHandle[] pins = wordBytesUtf8.Select(b => GCHandle.Alloc(b, GCHandleType.Pinned)).ToArray();
try
{
    var hcStrs = new Native.HcStr[wordBytesUtf8.Length];
    for (int i = 0; i < wordBytesUtf8.Length; i++)
    {
        hcStrs[i] = new Native.HcStr
        {
            Ptr = pins[i].AddrOfPinnedObject(),
            Len = (UIntPtr)wordBytesUtf8[i].Length,
        };
    }

    unsafe
    {
        fixed (Native.HcStr* hp = hcStrs)
        {
            int code = Native.hc_parse_batch(handle, hp, (UIntPtr)hcStrs.Length, 4, out Native.HcResultBuf batchOut);
            if (code != 0)
            {
                Console.Error.WriteLine($"[harness] hc_parse_batch FAILED: code={code}");
                return 1;
            }
            var decoded = Native.DecodeBuffer(batchOut);
            Native.hc_buf_free(ref batchOut);
            int batchWordsWithAnalyses = decoded.Count(w => w.Analyses.Count > 0);
            int batchTotalAnalyses = decoded.Sum(w => w.Analyses.Count);
            Console.WriteLine(
                $"[harness] hc_parse_batch(n={decoded.Count}, threads=4): {batchWordsWithAnalyses} words with " +
                $">=1 analysis, {batchTotalAnalyses} total analyses");

            if (decoded.Count != words.Length || batchWordsWithAnalyses != wordsWithAnalyses || batchTotalAnalyses != totalAnalyses)
            {
                Console.Error.WriteLine("[harness] hc_parse_batch summary disagrees with the per-word hc_parse_word pass");
                return 1;
            }

            // Print one fully-decoded sample analysis to show the numeric fields marshal correctly.
            var sample = decoded.FirstOrDefault(w => w.Analyses.Count > 0);
            if (sample.Analyses is { Count: > 0 })
            {
                var a = sample.Analyses[0];
                Console.WriteLine(
                    $"[harness] sample analysis: pos_id={(a.PosId.HasValue ? a.PosId.Value.ToString() : "none")}, " +
                    $"root_morpheme_index={a.RootMorphemeIndex}, morpheme_ids=[{string.Join(",", a.MorphemeIds)}]");
            }
        }
    }
}
finally
{
    foreach (var pin in pins) pin.Free();
}

Native.hc_grammar_free(handle);
Console.WriteLine("[harness] hc_grammar_free OK — all calls succeeded, ABI is callable from managed .NET code");
return 0;

internal static class Native
{
    public const string LibraryName = "hermit_crab";

    [StructLayout(LayoutKind.Sequential)]
    public struct HcResultBuf
    {
        public IntPtr Data;
        public UIntPtr Len;
        public UIntPtr Cap;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct HcError
    {
        public int Code;
        public int Pad;
        public HcResultBuf Message;
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct HcStr
    {
        public IntPtr Ptr;
        public UIntPtr Len;
    }

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    public static extern int hc_abi_version();

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    public static unsafe extern int hc_grammar_load(byte* xmlUtf8, UIntPtr len, out IntPtr handle, out HcError err);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    public static extern void hc_grammar_free(IntPtr handle);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    public static unsafe extern int hc_parse_word(IntPtr handle, byte* wordUtf8, UIntPtr len, out HcResultBuf outBuf);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    public static unsafe extern int hc_parse_batch(IntPtr handle, HcStr* words, UIntPtr n, int maxThreads, out HcResultBuf outBuf);

    [DllImport(LibraryName, CallingConvention = CallingConvention.Cdecl)]
    public static extern void hc_buf_free(ref HcResultBuf buf);

    public static string ReadMessage(HcResultBuf buf)
    {
        if (buf.Data == IntPtr.Zero || buf.Len == UIntPtr.Zero) return string.Empty;
        var bytes = new byte[(int)buf.Len];
        Marshal.Copy(buf.Data, bytes, 0, bytes.Length);
        return Encoding.UTF8.GetString(bytes);
    }

    public readonly record struct DecodedAnalysis(uint? PosId, int RootMorphemeIndex, List<uint> MorphemeIds);
    public readonly record struct DecodedWord(bool InvalidShape, bool Capped, List<DecodedAnalysis> Analyses);

    /// Decodes the exact wire format documented in `hc-ffi/src/buffer.rs`'s module docs: a
    /// little-endian, length-prefixed buffer — magic (u32) + word_count (u32), then per word a
    /// status/capped/reserved/analysis_count header followed by analysis records
    /// (pos_id: i32, root_morpheme_index: i32, morpheme_count: u32, morpheme_ids: u32[]).
    public static List<DecodedWord> DecodeBuffer(HcResultBuf buf)
    {
        var bytes = new byte[(int)buf.Len];
        if (bytes.Length > 0) Marshal.Copy(buf.Data, bytes, 0, bytes.Length);
        int pos = 0;

        uint ReadU32()
        {
            uint v = BitConverter.ToUInt32(bytes, pos);
            pos += 4;
            return v;
        }
        int ReadI32() => (int)ReadU32();
        byte ReadU8() => bytes[pos++];
        ushort ReadU16()
        {
            ushort v = BitConverter.ToUInt16(bytes, pos);
            pos += 2;
            return v;
        }

        const uint Magic = 0x4843_5246;
        uint magic = ReadU32();
        if (magic != Magic) throw new InvalidDataException($"bad magic: 0x{magic:X}");
        uint wordCount = ReadU32();

        var words = new List<DecodedWord>((int)wordCount);
        for (int w = 0; w < wordCount; w++)
        {
            byte status = ReadU8();
            byte capped = ReadU8();
            _ = ReadU16(); // reserved padding
            uint analysisCount = ReadU32();
            var analyses = new List<DecodedAnalysis>((int)analysisCount);
            for (int a = 0; a < analysisCount; a++)
            {
                int posIdRaw = ReadI32();
                int rootIdx = ReadI32();
                uint morphCount = ReadU32();
                var ids = new List<uint>((int)morphCount);
                for (int m = 0; m < morphCount; m++) ids.Add(ReadU32());
                analyses.Add(new DecodedAnalysis(posIdRaw < 0 ? null : (uint)posIdRaw, rootIdx, ids));
            }
            words.Add(new DecodedWord(status == 1, capped == 1, analyses));
        }
        return words;
    }
}
