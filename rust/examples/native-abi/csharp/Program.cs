using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;

internal static class Native
{
    private const string Library = "pangloss_ffi";
    [StructLayout(LayoutKind.Sequential)] internal struct Buffer { internal IntPtr Data; internal nuint Len; internal nuint Cap; }
    [StructLayout(LayoutKind.Sequential)] internal struct Error { internal int Code; internal int Pad; internal Buffer Message; }
    [DllImport(Library)] internal static extern int hc_grammar_load(byte[] xml, nuint len, out IntPtr handle, out Error error);
    [DllImport(Library)] internal static extern void hc_grammar_free(IntPtr handle);
    [DllImport(Library)] internal static extern void hc_buf_free(ref Buffer buffer);
    [DllImport(Library)] internal static extern int hc_lexicon_catalog_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_lexicon_set_gloss_language_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_lexicon_add_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_analyze_word_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_lexicon_export_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_lexicon_remove_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_lexicon_import_json(IntPtr h, byte[] json, nuint len, out Buffer output);
    [DllImport(Library)] internal static extern int hc_lexicon_update_json(IntPtr h, byte[] json, nuint len, out Buffer output);

    internal static void Configure(string path) => NativeLibrary.SetDllImportResolver(typeof(Native).Assembly,
        (_, _, _) => NativeLibrary.Load(Path.GetFullPath(path)));
}

internal static class Program
{
    private delegate int JsonCall(IntPtr handle, byte[] request, nuint len, out Native.Buffer output);

    private static JsonDocument Call(IntPtr handle, JsonCall fn, object request)
    {
        byte[] bytes = JsonSerializer.SerializeToUtf8Bytes(request);
        int status = fn(handle, bytes, (nuint)bytes.Length, out Native.Buffer output);
        if (status != 0 || output.Data == IntPtr.Zero) throw new InvalidOperationException($"JSON transport failure: {status}");
        try
        {
            byte[] result = new byte[(int)output.Len];
            Marshal.Copy(output.Data, result, 0, result.Length);
            return JsonDocument.Parse(result);
        }
        finally { Native.hc_buf_free(ref output); }
    }

    private static JsonElement Value(JsonDocument document) => document.RootElement.GetProperty("value");
    private static bool HasSupplied(JsonDocument analysis, string id) =>
        Value(analysis).GetProperty("structured").EnumerateArray().Any(item =>
            item.GetProperty("provenance").TryGetProperty("entryId", out var found) && found.GetString() == id);

    private static int Main(string[] args)
    {
        if (args.Length != 2) { Console.Error.WriteLine("usage: PanGlossNativeSmoke LIBRARY GRAMMAR_XML"); return 2; }
        Native.Configure(args[0]);
        byte[] xml = File.ReadAllBytes(args[1]);
        int status = Native.hc_grammar_load(xml, (nuint)xml.Length, out IntPtr grammar, out Native.Error error);
        if (status != 0)
        {
            string message = error.Message.Data == IntPtr.Zero ? "" : Marshal.PtrToStringUTF8(error.Message.Data, (int)error.Message.Len)!;
            if (error.Message.Data != IntPtr.Zero) Native.hc_buf_free(ref error.Message);
            throw new InvalidOperationException($"grammar load failed ({status}): {message}");
        }
        if (error.Message.Data != IntPtr.Zero) Native.hc_buf_free(ref error.Message);
        try
        {
            using JsonDocument catalog = Call(grammar, Native.hc_lexicon_catalog_json, new { });
            string signature = Value(catalog).GetProperty("signatures")[0].GetProperty("id").GetString()!;
            using JsonDocument gloss = Call(grammar, Native.hc_lexicon_set_gloss_language_json, new { glossLanguage = "en" });
            string revision = Value(gloss).GetProperty("revision").GetString()!;
            using JsonDocument added = Call(grammar, Native.hc_lexicon_add_json, new { stem = "nupa", gloss = "host smoke", signatures = new[] { signature }, expectedRevision = revision });
            string id = Value(added).GetProperty("value").GetProperty("id").GetString()!;
            string addRevision = Value(added).GetProperty("revision").GetString()!;
            using JsonDocument first = Call(grammar, Native.hc_analyze_word_json, new { word = "nupasi" });
            if (!HasSupplied(first, id)) throw new InvalidOperationException("inflected supplied analysis missing");
            using JsonDocument exported = Call(grammar, Native.hc_lexicon_export_json, new { });
            JsonElement document = Value(exported).Clone();
            using JsonDocument removed = Call(grammar, Native.hc_lexicon_remove_json, new { id, expectedRevision = addRevision });
            if (!Value(removed).GetProperty("value").GetBoolean()) throw new InvalidOperationException("remove failed");
            using JsonDocument absent = Call(grammar, Native.hc_analyze_word_json, new { word = "nupasi" });
            if (HasSupplied(absent, id)) throw new InvalidOperationException("hard-removed entry still parses");
            using JsonDocument imported = Call(grammar, Native.hc_lexicon_import_json, new { document });
            using JsonDocument restored = Call(grammar, Native.hc_analyze_word_json, new { word = "nupasi" });
            if (!HasSupplied(restored, id)) throw new InvalidOperationException("import did not restore entry");
            using JsonDocument conflict = Call(grammar, Native.hc_lexicon_update_json, new { id, stem = "nupa", gloss = "changed", signatures = new[] { signature }, expectedRevision = "rev_0" });
            if (conflict.RootElement.GetProperty("ok").GetBoolean() || conflict.RootElement.GetProperty("error").GetProperty("code").GetString() != "revision_conflict")
                throw new InvalidOperationException("revision conflict missing");
            return 0;
        }
        finally { Native.hc_grammar_free(grammar); }
    }
}
