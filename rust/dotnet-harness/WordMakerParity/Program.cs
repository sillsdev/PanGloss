using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using SIL.LCModel.Core.KernelInterfaces;
using SIL.LCModel.Core.Text;
using SIL.LCModel.Core.WritingSystems;
using SIL.LCModel.DomainServices;
using MachineRange = SIL.Machine.Annotations.Range<int>;
using SIL.Machine.Tokenization;
using SIL.WritingSystems;

if (args.Length != 2 || string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("LIBLCM_REVISION")) || string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("MACHINE_REVISION")))
    throw new ArgumentException("Usage: WordMakerParity cases.json output.json; set LIBLCM_REVISION and MACHINE_REVISION to the checked-out source revisions");
Icu.Wrapper.ConfineIcuVersions(70, 70);
// The public initializer uses a shared machine cache. Isolate the offline oracle from it.
var sldrDirectory = Path.Combine(Path.GetTempPath(), "wordmaker-sldr-" + Guid.NewGuid());
Directory.CreateDirectory(sldrDirectory);
typeof(Sldr).GetMethod("Initialize", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Static, null, new[] { typeof(bool), typeof(string) }, null)!
    .Invoke(null, new object[] { true, sldrDirectory });
try
{
var revision = Environment.GetEnvironmentVariable("LIBLCM_REVISION")!;
var machineRevision = Environment.GetEnvironmentVariable("MACHINE_REVISION")!;
var cases = JsonSerializer.Deserialize<List<Case>>(File.ReadAllText(args[0]), new JsonSerializerOptions { PropertyNameCaseInsensitive = true })!;
var results = new List<object>();
foreach (var item in cases)
{
    var manager = new WritingSystemManager();
    var ws = manager.Set("en");
    using (var reader = System.Xml.XmlReader.Create(new StringReader(item.Ldml)))
        new LdmlDataMapper(null).Read(reader, ws);
    var profiles = new Dictionary<string, CoreWritingSystemDefinition>();
    var builder = TsStringUtils.MakeString(item.Text, ws.Handle).GetBldr();
    foreach (var style in item.Styles)
    {
        var start = Utf16(item.Text, style.Start);
        var end = Utf16(item.Text, style.End);
        if (!string.IsNullOrEmpty(style.Style))
            builder.SetStrPropValue(start, end, (int)FwTextPropType.ktptNamedStyle, style.Style);
        if (style.Profile != null)
        {
            var key = JsonSerializer.Serialize(style.Profile);
            if (!profiles.TryGetValue(key, out var second))
            {
                second = manager.Set($"qaa-x-run{profiles.Count}");
                var secondChars = new CharacterSetDefinition("main");
                foreach (var ch in style.Profile.WordForming) secondChars.Characters.Add(ch);
                second.CharacterSets.Add(secondChars);
                profiles.Add(key, second);
            }
            builder.SetIntPropValues(start, end, (int)FwTextPropType.ktptWs, 0, second.Handle);
        }
    }
    var maker = new WordMaker(builder.GetString(), manager);
    var words = new List<Slice>();
    while (maker.NextWord(out var start, out var end) != null)
        words.Add(new Slice(item.Text.Substring(start, end - start), start, end));
    var machine = new LatinWordTokenizer().TokenizeAsRanges(item.Text, MachineRange.Create(0, item.Text.Length))
        .Select(r => new Slice(item.Text.Substring(r.Start, r.Length), r.Start, r.End)).ToList();
    results.Add(new { name = item.Name, profile_sha256 = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(item.Ldml))).ToLowerInvariant(), run_profile_hashes = item.Styles.Where(s => s.Profile != null).Select(s => s.Profile!.SourceSha256), words, machine });
}
File.WriteAllText(args[1], JsonSerializer.Serialize(new { liblcm_revision = revision, machine_revision = machineRevision, icu_version = Icu.Wrapper.IcuVersion, unicode_version = Icu.Wrapper.UnicodeVersion, cases = results }, new JsonSerializerOptions { WriteIndented = true, PropertyNamingPolicy = JsonNamingPolicy.CamelCase }));
}
finally { Sldr.Cleanup(); Directory.Delete(sldrDirectory, recursive: true); }
static int Utf16(string text, int scalars) => text.EnumerateRunes().Take(scalars).Sum(r => r.Utf16SequenceLength);
record Case(string Name, string Text, string Ldml, string Additions, List<StyleRun> Styles);
record StyleRun(int Start, int End, string Style, Profile? Profile);
record Profile(int Version, [property: System.Text.Json.Serialization.JsonPropertyName("source_id")] string SourceId, [property: System.Text.Json.Serialization.JsonPropertyName("word_forming")] List<string> WordForming, [property: System.Text.Json.Serialization.JsonPropertyName("source_sha256")] string SourceSha256);
record Slice(string Text, int Start, int End);
