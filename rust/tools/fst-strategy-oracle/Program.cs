using System.Collections;
using System.Reflection;
using System.Security.Cryptography;
using System.Text.Json;
using SIL.Machine.Morphology.HermitCrab;

if (args.Length != 4)
    throw new ArgumentException("usage: fst-strategy-oracle <grammar.xml> <words.txt> <out.jsonl> <machine-pin>");
// The pinned loader owns XML identity. Its private maps expose source IDs without a second loader.
const BindingFlags flags = BindingFlags.Instance | BindingFlags.NonPublic;
var loader = Activator.CreateInstance(typeof(XmlLanguageLoader), flags, null, new object?[] { args[0], null }, null)!;
var language = (Language)typeof(XmlLanguageLoader).GetMethod("Load", flags)!.Invoke(loader, null)!;
Dictionary<object, string> SourceMap(string field) {
    var map = (IDictionary)typeof(XmlLanguageLoader).GetField(field, flags)!.GetValue(loader)!;
    return map.Keys.Cast<object>().ToDictionary(key => map[key]!, key => (string)key, ReferenceEqualityComparer.Instance);
}
var morphemeKeys = SourceMap("_morphemes");
var allomorphKeys = SourceMap("_allomorphs");
var morpher = new Morpher(new TraceManager(), language);
using var writer = new StreamWriter(args[2]) { AutoFlush = true };
writer.WriteLine(JsonSerializer.Serialize(new {
    machine_pin = args[3], source_sha256 = Convert.ToHexStringLower(SHA256.HashData(File.ReadAllBytes(args[0]))),
    words_sha256 = Convert.ToHexStringLower(SHA256.HashData(File.ReadAllBytes(args[1])))
}));
var words = File.ReadLines(args[1]).Select(word => word.Trim()).Where(word => word.Length != 0);
var index = 0;
foreach (var word in words)
{
    writer.WriteLine(JsonSerializer.Serialize(new { word_id = index, word, status = "started" }));
    try
    {
        var results = morpher.ParseWord(word, out _, false).ToArray();
        var analyses = results.Select(result => {
            var allomorphs = result.AllomorphsInMorphOrder.ToArray();
            return new {
                morphemes = allomorphs.Select(allomorph => morphemeKeys[allomorph.Morpheme]).ToArray(),
                allomorphs = allomorphs.Select(allomorph => allomorphKeys[allomorph]).ToArray(),
                rootIndex = Array.FindIndex(allomorphs, allomorph => allomorph == result.RootAllomorph),
                category = result.SyntacticFeatureStruct.PartsOfSpeech().FirstOrDefault()?.ID
            };
        }).ToArray();
        writer.WriteLine(JsonSerializer.Serialize(new { word_id = index, word, status = "complete", analyses }));
    }
    catch (InvalidShapeException)
    {
        writer.WriteLine(JsonSerializer.Serialize(new { word_id = index, word, status = "invalid_shape", analyses = Array.Empty<object>() }));
    }
    index++;
}
