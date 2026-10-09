using System.Reflection;
using System.Runtime.Loader;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Xml.Linq;
using SIL.Machine.Morphology;
using SIL.Machine.Morphology.HermitCrab;

internal static class Program
{
    private const string MachineDll = "SIL.Machine.dll";
    private const string HermitCrabDll = "SIL.Machine.Morphology.HermitCrab.dll";
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
    };

    private static int Main(string[] args)
    {
        var diagnose = args.Length == 4 && args[0] == "diagnose";
        var capture = args.Length == 5 && args[0] == "capture";
        if (!diagnose && !capture)
        {
            Console.Error.WriteLine("usage: capture <underdefined-root> <engine-dir> <output-root> <harness-source-dir> | diagnose <underdefined-root> <engine-dir> <harness-source-dir>");
            return 2;
        }

        try
        {
            var root = Path.GetFullPath(args[1]);
            var engineDirectory = Path.GetFullPath(args[2]);
            Bootstrap(
                root,
                engineDirectory,
                diagnose ? root : Path.GetFullPath(args[3]),
                Path.GetFullPath(diagnose ? args[3] : args[4]),
                diagnose
            );
            return 0;
        }
        catch (Exception exception)
        {
            Console.Error.WriteLine(exception);
            return 1;
        }
    }

    private static void Bootstrap(
        string root,
        string engineDirectory,
        string outputRoot,
        string sourceDirectory,
        bool diagnose
    )
    {
        using var manifest = JsonDocument.Parse(File.ReadAllText(Path.Combine(root, "engine-provenance.json")));
        var machineHash = VerifyPinnedDll(manifest.RootElement, engineDirectory, MachineDll);
        var hermitCrabHash = VerifyPinnedDll(manifest.RootElement, engineDirectory, HermitCrabDll);

        AssemblyLoadContext.Default.Resolving += (_, name) =>
        {
            if (string.IsNullOrEmpty(name.Name))
                return null;
            var path = Path.Combine(engineDirectory, name.Name + ".dll");
            return File.Exists(path) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(path) : null;
        };
        AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.Combine(engineDirectory, "SIL.Core.dll"));
        var pinnedMachine = AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.Combine(engineDirectory, MachineDll));
        var pinnedHermitCrab = AssemblyLoadContext.Default.LoadFromAssemblyPath(Path.Combine(engineDirectory, HermitCrabDll));
        RunLoaded(
            root,
            engineDirectory,
            outputRoot,
            sourceDirectory,
            diagnose,
            machineHash,
            hermitCrabHash,
            pinnedMachine,
            pinnedHermitCrab
        );
    }

    private static void RunLoaded(
        string root,
        string engineDirectory,
        string outputRoot,
        string sourceDirectory,
        bool diagnose,
        string machineHash,
        string hermitCrabHash,
        Assembly pinnedMachine,
        Assembly pinnedHermitCrab
    )
    {
        if (typeof(WordAnalysis).Assembly != pinnedMachine || typeof(Morpher).Assembly != pinnedHermitCrab)
            throw new InvalidOperationException("type references did not resolve to the preloaded pinned assemblies");
        VerifyLoadedAssembly(pinnedMachine, Path.Combine(engineDirectory, MachineDll), machineHash);
        VerifyLoadedAssembly(pinnedHermitCrab, Path.Combine(engineDirectory, HermitCrabDll), hermitCrabHash);

        var sourceHash = HashFile(Path.Combine(sourceDirectory, "Program.cs"));
        var projectHash = HashFile(Path.Combine(sourceDirectory, "UnderdefinedIdentityCapture.csproj"));
        var engineVersion = pinnedHermitCrab.GetName().Version?.ToString()
            ?? throw new InvalidOperationException("HermitCrab assembly has no version");
        if (engineVersion != "3.8.2.0")
            throw new InvalidOperationException($"unexpected HermitCrab assembly version: {engineVersion}");

        var states = LoadAndReplayAll(root);
        var failures = states.SelectMany(CheckConsistency).ToList();
        var ambiguity = states
            .SelectMany(state => state.Solutions.Count > 1
                ? AmbiguousMappings(state).Select(id => new ConsistencyMismatch(
                    state.CaseName, state.State, "<state>", "ambiguous-source-id", null,
                    id.Key, string.Join(" | ", id.Values),
                    "more than one global source-ID crosswalk projects every word's stored-analysis multiset equally"
                ))
                : Enumerable.Empty<ConsistencyMismatch>())
            .ToList();
        failures.AddRange(ambiguity);

        var summary = new
        {
            schemaVersion = 2,
            caseCount = states.Select(state => state.CaseName).Distinct(StringComparer.Ordinal).Count(),
            stateCount = states.Count,
            wordCount = states.Sum(state => state.Words.Count),
            mismatchCount = failures.Count,
            mismatchKinds = failures.GroupBy(mismatch => mismatch.Kind)
                .OrderBy(group => group.Key)
                .Select(group => new { kind = group.Key, count = group.Count() }),
            parseSolverAmbiguousMappings = states.Count(state => state.ParseSolverSolutionCount > 1),
            uniqueMappings = states.Count(state => state.Solutions.Count == 1),
            contradictoryMappings = states.Count(state => state.Solutions.Count == 0),
            ambiguousMappings = states.Count(state => state.Solutions.Count > 1),
            staticallyResolvedSourceIds = states.Sum(state => state.StaticEvidence.Count),
            mismatches = failures,
        };
        if (diagnose)
        {
            Console.WriteLine(JsonSerializer.Serialize(summary, JsonOptions));
            return;
        }

        if (states.Count != 36)
            throw new InvalidOperationException($"expected 36 case/state captures, found {states.Count}");
        if (failures.Count > 0)
            throw new InvalidOperationException("consistency proof failed:\n" + JsonSerializer.Serialize(failures, JsonOptions));

        var captures = states.Select(state => (
            RelativePath: Path.Combine(state.CaseName, "measurements", state.State, "hc-identity.json"),
            Capture: CreateIdentityCapture(
                state,
                engineVersion,
                sourceHash,
                projectHash,
                machineHash,
                hermitCrabHash
            )
        )).ToList();

        foreach (var (relativePath, _) in captures)
        {
            var path = Path.Combine(outputRoot, relativePath);
            if (File.Exists(path))
                throw new IOException($"refusing to overwrite existing capture: {path}");
        }
        foreach (var (relativePath, capture) in captures)
        {
            var path = Path.Combine(outputRoot, relativePath);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, JsonSerializer.Serialize(capture, JsonOptions) + Environment.NewLine);
        }

        Console.WriteLine($"Captured and cross-checked {captures.Count} case/state files with {captures.Sum(item => item.Capture.Words.Count)} word rows.");
        Console.WriteLine($"SIL.Machine.dll sha256={machineHash}");
        Console.WriteLine($"SIL.Machine.Morphology.HermitCrab.dll sha256={hermitCrabHash}");
        Console.WriteLine($"Program.cs sha256={sourceHash}");
    }

    private static List<ReplayState> LoadAndReplayAll(string root)
    {
        var states = new List<ReplayState>();
        foreach (var caseDirectory in Directory.EnumerateDirectories(root).Order(StringComparer.Ordinal))
        {
            var caseName = Path.GetFileName(caseDirectory);
            var measurementsDirectory = Path.Combine(caseDirectory, "measurements");
            if (!Directory.Exists(measurementsDirectory))
                continue;

            foreach (var stateDirectory in Directory.EnumerateDirectories(measurementsDirectory).Order(StringComparer.Ordinal))
            {
                var state = Path.GetFileName(stateDirectory);
                var xmlPath = Path.Combine(stateDirectory, "Probe.hc.xml");
                var wordsPath = Path.Combine(stateDirectory, "words.txt");
                var legacyPath = Path.Combine(stateDirectory, "hc.json");
                var invocationPath = Path.Combine(stateDirectory, "parse-hc.invocation.json");
                var responsePath = Path.Combine(stateDirectory, "response.json");
                if (!File.Exists(xmlPath) || !File.Exists(wordsPath) || !File.Exists(legacyPath)
                    || !File.Exists(invocationPath) || !File.Exists(responsePath))
                    throw new FileNotFoundException($"incomplete measurement inputs for {caseName}/{state}");

                var legacy = JsonSerializer.Deserialize<LegacyCapture>(File.ReadAllText(legacyPath), JsonOptions)
                    ?? throw new InvalidOperationException($"cannot read legacy capture {legacyPath}");
                using var response = JsonDocument.Parse(File.ReadAllText(responsePath));
                var projectRelativePath = response.RootElement.GetProperty("sourcePath").GetString()
                    ?? throw new InvalidOperationException($"response has no sourcePath for {caseName}/{state}");
                var sourceProjectPath = Path.GetFullPath(Path.Combine(stateDirectory, projectRelativePath));
                if (!File.Exists(sourceProjectPath))
                    throw new FileNotFoundException($"saved FieldWorks project for {caseName}/{state} is missing", sourceProjectPath);
                var categories = ReadPartOfSpeechSymbols(root, xmlPath, sourceProjectPath);
                var categoryKeys = categories.Symbols.ToDictionary(
                    symbol => symbol.Id,
                    symbol => symbol.StableGuid,
                    StringComparer.Ordinal
                );
                var words = File.ReadAllLines(wordsPath).Where(word => word.Length > 0).ToArray();
                var language = XmlLanguageLoader.Load(xmlPath);
                var morpher = new Morpher(new TraceManager(), language);
                var replayWords = new List<ReplayWord>(words.Length);

                if (words.Length != legacy.Words.Count)
                    throw new InvalidOperationException($"{caseName}/{state}: words.txt and hc.json row counts differ");

                for (var index = 0; index < words.Length; index++)
                {
                    var word = words[index];
                    var legacyWord = legacy.Words[index];
                    if (word != legacyWord.Word)
                        throw new InvalidOperationException($"{caseName}/{state}/{word}: words.txt and hc.json word order differs");

                    List<Word> parses;
                    string? engineError = null;
                    try
                    {
                        parses = morpher.ParseWord(word).ToList();
                    }
                    catch (Exception exception)
                    {
                        parses = [];
                        engineError = exception.GetType().FullName + ": " + exception.Message;
                    }

                    var analyses = new List<ReplayAnalysis>(parses.Count);
                    foreach (var parse in parses)
                    {
                        // ParseHcCommand stores every Morph; WordAnalysis deduplicates repeated allomorphs.
                        var storedMorphAllomorphs = parse.Morphs.Select(parse.GetAllomorph).ToArray();
                        var identityAllomorphs = parse.AllomorphsInMorphOrder.ToArray();
                        var rootIndex = Array.FindIndex(identityAllomorphs, allomorph => allomorph == parse.RootAllomorph);
                        var machineCategoryId = parse.SyntacticFeatureStruct.PartsOfSpeech().FirstOrDefault()?.ID;
                        var category = machineCategoryId is null
                            ? null
                            : categoryKeys.TryGetValue(machineCategoryId, out var stableCategory)
                                ? stableCategory
                                : throw new InvalidOperationException($"C# parse names unknown part-of-speech id {machineCategoryId}");
                        var identity = new WordAnalysis(
                            identityAllomorphs.Select(allomorph => allomorph.Morpheme), rootIndex, category
                        );
                        var machineMorphemeIds = identityAllomorphs
                            .Select(allomorph => ReadMachineMorphemeId(allomorph.Morpheme))
                            .ToArray();
                        var storedIds = storedMorphAllomorphs.Select(allomorph => new MachineStoredMorphIds(
                            ReadPropertyId(allomorph.Properties, "ID"),
                            ReadMachineMorphemeId(allomorph.Morpheme),
                            ReadOptionalPropertyId(allomorph.Morpheme.Properties, "InflTypeID")
                        )).ToArray();
                        analyses.Add(new ReplayAnalysis(
                            parse,
                            storedIds,
                            machineMorphemeIds,
                            machineCategoryId,
                            identity.RootMorphemeIndex,
                            identity.Category
                        ));
                    }
                    replayWords.Add(new ReplayWord(word, analyses, engineError));
                }

                var replayState = new ReplayState(
                    caseName,
                    state,
                    root,
                    xmlPath,
                    wordsPath,
                    legacyPath,
                    invocationPath,
                    sourceProjectPath,
                    legacy,
                    categories.Symbols,
                    categories.Evidence,
                    replayWords
                );
                replayState.Solutions = SolveCrosswalks(replayState);
                replayState.ParseSolverSolutionCount = replayState.Solutions.Count;
                if (replayState.Solutions.Count > 1)
                    ApplyStaticCrosswalkEvidence(replayState);
                states.Add(replayState);
            }
        }
        return states;
    }

    private static IEnumerable<ConsistencyMismatch> CheckConsistency(ReplayState state)
    {
        var result = new List<ConsistencyMismatch>(state.StaticFailures);
        for (var index = 0; index < state.Words.Count; index++)
        {
            var replay = state.Words[index];
            var legacy = state.Legacy.Words[index];
            if (legacy.ProjectionAgrees != true)
                result.Add(Mismatch(state, replay.Word, "original-projection-disagrees", "projectionAgrees=true", $"projectionAgrees={legacy.ProjectionAgrees}",
                    "the original ParseHcCommand capture recorded a HCLoader versus XmlLanguageLoader count/error disagreement"));
            if (legacy.EngineError != replay.EngineError)
                result.Add(Mismatch(state, replay.Word, "engine-error", legacy.EngineError ?? "<null>", replay.EngineError ?? "<null>",
                    "the direct XmlLanguageLoader replay status differs from the original ParserCore/HCLoader capture"));
            if (legacy.ProjectedEngineError != replay.EngineError)
                result.Add(Mismatch(state, replay.Word, "projected-engine-error", legacy.ProjectedEngineError ?? "<null>", replay.EngineError ?? "<null>",
                    "the replay status differs from ParseHcCommand's saved XML-projector status"));
            var replayCount = replay.EngineError is null ? replay.Analyses.Count : (int?)null;
            if (legacy.ProjectedAnalysisCount != replayCount)
                result.Add(Mismatch(state, replay.Word, "projected-analysis-count", legacy.ProjectedAnalysisCount?.ToString() ?? "<null>", replayCount?.ToString() ?? "<null>",
                    "the replay count differs from ParseHcCommand's saved XML-projector count"));
            if (legacy.Analyses.Count != replay.Analyses.Count)
                result.Add(Mismatch(state, replay.Word, "analysis-count", legacy.Analyses.Count.ToString(), replay.Analyses.Count.ToString(),
                    "the ParserCore/HCLoader key count differs from the pinned XML replay count"));

            if (state.Solutions.Count == 1)
            {
                var projected = replay.Analyses
                    .Select(analysis => ProjectStoredKey(analysis, state.Solutions[0]))
                    .ToList();
                var expected = legacy.Analyses.Select(analysis => analysis.Morphemes).ToList();
                if (!MultisetEqual(expected, projected))
                    result.Add(Mismatch(state, replay.Word, "stored-key-multiset", KeyMultisetSummary(expected), KeyMultisetSummary(projected),
                        "the unique case/state crosswalk does not produce the original stored-key multiset for this word"));
            }
        }

        if (state.Solutions.Count == 0)
            if (state.ParseSolverSolutionCount == 0)
                result.Add(Mismatch(state, "<state>", "contradictory-source-id-crosswalk", "one globally consistent assignment", "no assignment",
                    "no source-ID mapping and per-word analysis matching makes every word's projected-key multiset equal the ParserCore/HCLoader capture"));
        return result;
    }

    private static List<SourceCrosswalk> SolveCrosswalks(ReplayState state)
    {
        var solutionsBySignature = new Dictionary<string, SourceCrosswalk>(StringComparer.Ordinal);
        SearchWords(state, 0, new SourceCrosswalk(), solutionsBySignature);
        return solutionsBySignature.Values.ToList();
    }

    private static void ApplyStaticCrosswalkEvidence(ReplayState state)
    {
        try
        {
            var hcEvidence = ReadHcSourceEvidence(state.XmlPath);
            var fieldWorksEvidence = ReadFieldWorksSourceEvidence(state.SourceProjectPath);
            var ambiguousIds = state.Solutions
                .SelectMany(solution => solution.Values.Keys)
                .Distinct()
                .Select(key => (
                    Key: key,
                    Candidates: state.Solutions
                        .Select(solution => solution.Values.GetValueOrDefault(key))
                        .OfType<string>()
                        .Distinct(StringComparer.Ordinal)
                        .Order(StringComparer.Ordinal)
                        .ToArray()
                ))
                .Where(item => item.Candidates.Length > 1)
                .OrderBy(item => item.Key.Kind, StringComparer.Ordinal)
                .ThenBy(item => item.Key.Id, StringComparer.Ordinal)
                .ToList();
            var resolved = new Dictionary<SourceId, string>();
            var evidence = new List<SourceCrosswalkEvidence>();

            foreach (var item in ambiguousIds.Where(item => item.Key.Kind == "allomorph"))
            {
                if (!hcEvidence.Allomorphs.TryGetValue(item.Key.Id, out var xmlAllomorph))
                {
                    AddStaticFailure(state, item.Key, item.Candidates, "HC XML has no matching allomorph ID evidence");
                    continue;
                }
                var matches = item.Candidates
                    .Select(guid => MatchAllomorph(guid, xmlAllomorph, fieldWorksEvidence))
                    .Where(match => match is not null)
                    .Cast<MatchedAllomorph>()
                    .ToList();
                if (matches.Count != 1)
                {
                    AddStaticFailure(state, item.Key, item.Candidates,
                        $"form/gloss evidence retained {matches.Count} of {item.Candidates.Length} parse-solver candidates");
                    continue;
                }
                resolved[item.Key] = matches[0].Object.Guid;
                evidence.Add(CreateAllomorphEvidence(state, item.Key.Id, item.Candidates, xmlAllomorph, matches[0]));
            }

            foreach (var item in ambiguousIds.Where(item => item.Key.Kind == "morpheme"))
            {
                if (!hcEvidence.EntriesByMorphemeId.TryGetValue(item.Key.Id, out var xmlEntry))
                {
                    AddStaticFailure(state, item.Key, item.Candidates, "HC XML has no matching lexical-entry ID and gloss evidence");
                    continue;
                }

                var pairedAllomorphs = state.Words
                    .SelectMany(word => word.Analyses)
                    .SelectMany(analysis => analysis.StoredMorphIds)
                    .Where(morph => morph.MorphemeId == item.Key.Id)
                    .Select(morph => morph.AllomorphId)
                    .Distinct(StringComparer.Ordinal)
                    .Order(StringComparer.Ordinal)
                    .ToList();
                if (pairedAllomorphs.Count == 0)
                {
                    AddStaticFailure(state, item.Key, item.Candidates, "no parsed allomorph is paired with this morpheme ID");
                    continue;
                }

                var pairedMatches = new List<MatchedAllomorph>();
                foreach (var allomorphId in pairedAllomorphs)
                {
                    var parseCandidates = state.Solutions
                        .Select(solution => solution.Values.GetValueOrDefault(new SourceId("allomorph", allomorphId)))
                        .OfType<string>()
                        .Distinct(StringComparer.Ordinal)
                        .ToArray();
                    if (!hcEvidence.Allomorphs.TryGetValue(allomorphId, out var pairedXml)
                        || parseCandidates.Length == 0)
                        continue;
                    var matches = parseCandidates
                        .Select(guid => MatchAllomorph(guid, pairedXml, fieldWorksEvidence))
                        .Where(match => match is not null)
                        .Cast<MatchedAllomorph>()
                        .ToList();
                    if (matches.Count == 1)
                        pairedMatches.Add(matches[0]);
                }
                if (pairedMatches.Count != pairedAllomorphs.Count)
                {
                    AddStaticFailure(state, item.Key, item.Candidates,
                        "paired allomorph form/gloss evidence did not identify exactly one owner entry for every observed pairing");
                    continue;
                }

                var candidateMatches = item.Candidates
                    .Select(guid => MatchMorpheme(guid, xmlEntry, pairedMatches, fieldWorksEvidence))
                    .Where(match => match is not null)
                    .Cast<MatchedMorpheme>()
                    .ToList();
                if (candidateMatches.Count != 1)
                {
                    AddStaticFailure(state, item.Key, item.Candidates,
                        $"entry sense-gloss and paired form evidence retained {candidateMatches.Count} of {item.Candidates.Length} parse-solver candidates");
                    continue;
                }

                resolved[item.Key] = candidateMatches[0].Object.Guid;
                evidence.Add(CreateMorphemeEvidence(
                    state,
                    item.Key.Id,
                    item.Candidates,
                    xmlEntry,
                    candidateMatches[0],
                    pairedMatches
                ));
            }

            foreach (var item in ambiguousIds.Where(item => item.Key.Kind is not "allomorph" and not "morpheme"))
                AddStaticFailure(state, item.Key, item.Candidates, "no saved-file static evidence rule exists for this source-ID kind");

            if (state.StaticFailures.Count > 0)
                return;

            var constrained = state.Solutions
                .Where(solution => resolved.All(pair => solution.Values.GetValueOrDefault(pair.Key) == pair.Value))
                .ToList();
            if (constrained.Count == 0)
            {
                state.StaticFailures.Add(Mismatch(
                    state,
                    "<state>",
                    "static-evidence-contradicts-parse-solver",
                    "static form/gloss mappings included in the parse-solver assignments",
                    "no matching assignment",
                    "the saved Probe.hc.xml and FieldWorks project evidence selects a mapping excluded by the all-word parse constraints"
                ));
                return;
            }

            state.Solutions = constrained;
            state.StaticEvidence = evidence;
        }
        catch (Exception exception)
        {
            state.StaticFailures.Add(Mismatch(
                state,
                "<state>",
                "static-evidence-read-error",
                "readable HC XML and saved FieldWorks evidence",
                exception.GetType().Name + ": " + exception.Message,
                "static evidence could not be parsed and therefore cannot resolve an ambiguous source ID"
            ));
        }
    }

    private static void AddStaticFailure(ReplayState state, SourceId key, IReadOnlyList<string> candidates, string cause)
    {
        state.StaticFailures.Add(Mismatch(
            state,
            "<state>",
            "static-evidence-unresolved-source-id",
            $"one candidate for {key.Kind}:{key.Id}",
            string.Join(" | ", candidates),
            cause
        ));
    }

    private static HcSourceEvidence ReadHcSourceEvidence(string path)
    {
        var document = XDocument.Load(path, LoadOptions.SetLineInfo);
        var entries = new Dictionary<string, XmlEntryEvidence>(StringComparer.Ordinal);
        var allomorphs = new Dictionary<string, XmlAllomorphEvidence>(StringComparer.Ordinal);
        foreach (var entry in document.Descendants("LexicalEntry"))
        {
            var entryId = PropertyElement(entry, "ID");
            var glossElement = entry.Element("Gloss");
            if (entryId is null || glossElement is null)
                continue;
            var entryEvidence = new XmlEntryEvidence(
                entryId.Value.Trim(),
                entryId,
                ReadTextEvidence(glossElement, path)
            );
            entries[entryEvidence.SourceId] = entryEvidence;
            foreach (var allomorph in entry.Elements("Allomorphs").Elements("Allomorph"))
            {
                var id = PropertyElement(allomorph, "ID");
                var form = allomorph.Element("PhoneticShape");
                if (id is null || form is null)
                    continue;
                var item = new XmlAllomorphEvidence(
                    id.Value.Trim(),
                    id,
                    ReadTextEvidence(form, path),
                    entryEvidence
                );
                allomorphs[item.SourceId] = item;
            }
        }
        return new HcSourceEvidence(entries, allomorphs);
    }

    private static FieldWorksSourceEvidence ReadFieldWorksSourceEvidence(string path)
    {
        var document = XDocument.Load(path, LoadOptions.SetLineInfo);
        var sensesByOwner = document.Descendants("rt")
            .Where(element => (string?)element.Attribute("class") == "LexSense")
            .SelectMany(element => element.Elements("Gloss").SelectMany(gloss => ReadTextEvidenceMany(gloss, path))
                .Select(gloss => (Owner: (string?)element.Attribute("ownerguid"), Gloss: gloss)))
            .Where(item => item.Owner is not null)
            .GroupBy(item => item.Owner!, StringComparer.OrdinalIgnoreCase)
            .ToDictionary(group => group.Key, group => (IReadOnlyList<XmlTextEvidence>)group.Select(item => item.Gloss).ToList(), StringComparer.OrdinalIgnoreCase);
        var objects = new Dictionary<string, FieldWorksObjectEvidence>(StringComparer.OrdinalIgnoreCase);
        foreach (var element in document.Descendants("rt"))
        {
            var guid = (string?)element.Attribute("guid");
            var className = (string?)element.Attribute("class");
            if (guid is null || className is null)
                continue;
            var forms = element.Element("Form") is { } form
                ? ReadTextEvidenceMany(form, path)
                : [];
            var ownerGuid = (string?)element.Attribute("ownerguid");
            objects[guid] = new FieldWorksObjectEvidence(
                guid,
                className,
                ownerGuid,
                GetLine(element),
                ReadLineText(path, GetLine(element)),
                forms,
                ownerGuid is not null && sensesByOwner.TryGetValue(ownerGuid, out var glosses) ? glosses : []
            );
        }
        return new FieldWorksSourceEvidence(objects);
    }

    private static XElement? PropertyElement(XElement element, string propertyName)
    {
        return element.Elements("Properties")
            .SelectMany(properties => properties.Elements("Property"))
            .FirstOrDefault(property => (string?)property.Attribute("name") == propertyName);
    }

    private static XmlTextEvidence ReadTextEvidence(XElement element, string path)
    {
        var leaf = element.DescendantsAndSelf()
            .FirstOrDefault(candidate => !candidate.HasElements && !string.IsNullOrWhiteSpace(candidate.Value))
            ?? element;
        return new XmlTextEvidence(element.Value.Trim(), GetLine(leaf), ReadLineText(path, GetLine(leaf)));
    }

    private static IReadOnlyList<XmlTextEvidence> ReadTextEvidenceMany(XElement element, string path)
    {
        var leaves = element.DescendantsAndSelf()
            .Where(candidate => !candidate.HasElements && !string.IsNullOrWhiteSpace(candidate.Value))
            .ToList();
        return leaves.Count == 0
            ? [ReadTextEvidence(element, path)]
            : leaves.Select(leaf => new XmlTextEvidence(leaf.Value.Trim(), GetLine(leaf), ReadLineText(path, GetLine(leaf)))).ToList();
    }

    private static int GetLine(XObject element)
    {
        return element is System.Xml.IXmlLineInfo info && info.HasLineInfo()
            ? info.LineNumber
            : throw new InvalidOperationException("static evidence element has no XML line information");
    }

    private static string ReadLineText(string path, int line)
    {
        var text = File.ReadLines(path).Skip(line - 1).FirstOrDefault();
        return text ?? throw new InvalidOperationException($"line {line} is absent from {path}");
    }

    private static MatchedAllomorph? MatchAllomorph(
        string candidateGuid,
        XmlAllomorphEvidence xml,
        FieldWorksSourceEvidence fieldWorks
    )
    {
        if (!fieldWorks.Objects.TryGetValue(candidateGuid, out var candidate)
            || candidate.ClassName is not "MoStemAllomorph" and not "MoAffixAllomorph")
            return null;
        var form = candidate.Forms.FirstOrDefault(value => value.Value == xml.Form.Value);
        var sense = candidate.SenseGlosses.FirstOrDefault(value => value.Value == xml.Entry.Gloss.Value);
        return form is null || sense is null ? null : new MatchedAllomorph(candidate, form, sense);
    }

    private static MatchedMorpheme? MatchMorpheme(
        string candidateGuid,
        XmlEntryEvidence xml,
        IReadOnlyList<MatchedAllomorph> pairedAllomorphs,
        FieldWorksSourceEvidence fieldWorks
    )
    {
        if (!fieldWorks.Objects.TryGetValue(candidateGuid, out var candidate)
            || !candidate.ClassName.EndsWith("Msa", StringComparison.Ordinal)
            || pairedAllomorphs.Any(allomorph => !string.Equals(
                allomorph.Object.OwnerGuid,
                candidate.OwnerGuid,
                StringComparison.OrdinalIgnoreCase)))
            return null;
        var sense = candidate.SenseGlosses.FirstOrDefault(value => value.Value == xml.Gloss.Value);
        return sense is null ? null : new MatchedMorpheme(candidate, sense);
    }

    private static SourceCrosswalkEvidence CreateAllomorphEvidence(
        ReplayState state,
        string sourceId,
        IReadOnlyList<string> parseCandidates,
        XmlAllomorphEvidence xml,
        MatchedAllomorph fieldWorks
    )
    {
        return new SourceCrosswalkEvidence(
            "allomorph",
            sourceId,
            fieldWorks.Object.Guid,
            parseCandidates,
            "unique FieldWorks allomorph Form and entry sense Gloss match; selected GUID is a parse-solver candidate",
            RelativePath(state.RootPath, state.XmlPath),
            HashFile(state.XmlPath),
            GetLine(xml.SourceIdElement),
            ReadLineText(state.XmlPath, GetLine(xml.SourceIdElement)),
            xml.Form.Value,
            xml.Form.Line,
            xml.Form.LineText,
            xml.Entry.Gloss.Value,
            xml.Entry.Gloss.Line,
            xml.Entry.Gloss.LineText,
            RelativePath(state.RootPath, state.SourceProjectPath),
            HashFile(state.SourceProjectPath),
            fieldWorks.Object.ClassName,
            fieldWorks.Object.Line,
            fieldWorks.Object.LineText,
            fieldWorks.Form.Value,
            fieldWorks.Form.Line,
            fieldWorks.Form.LineText,
            fieldWorks.SenseGloss.Value,
            fieldWorks.SenseGloss.Line,
            fieldWorks.SenseGloss.LineText,
            []
        );
    }

    private static SourceCrosswalkEvidence CreateMorphemeEvidence(
        ReplayState state,
        string sourceId,
        IReadOnlyList<string> parseCandidates,
        XmlEntryEvidence xml,
        MatchedMorpheme fieldWorks,
        IReadOnlyList<MatchedAllomorph> pairedAllomorphs
    )
    {
        return new SourceCrosswalkEvidence(
            "morpheme",
            sourceId,
            fieldWorks.Object.Guid,
            parseCandidates,
            "unique entry sense Gloss match and shared entry with the paired Form-matched allomorph; selected GUID is a parse-solver candidate",
            RelativePath(state.RootPath, state.XmlPath),
            HashFile(state.XmlPath),
            GetLine(xml.SourceIdElement),
            ReadLineText(state.XmlPath, GetLine(xml.SourceIdElement)),
            null,
            null,
            null,
            xml.Gloss.Value,
            xml.Gloss.Line,
            xml.Gloss.LineText,
            RelativePath(state.RootPath, state.SourceProjectPath),
            HashFile(state.SourceProjectPath),
            fieldWorks.Object.ClassName,
            fieldWorks.Object.Line,
            fieldWorks.Object.LineText,
            null,
            null,
            null,
            fieldWorks.SenseGloss.Value,
            fieldWorks.SenseGloss.Line,
            fieldWorks.SenseGloss.LineText,
            pairedAllomorphs.Select(allomorph => new PairedAllomorphEvidence(
                allomorph.Object.Guid,
                allomorph.Object.ClassName,
                allomorph.Object.Line,
                allomorph.Object.LineText,
                allomorph.Form.Value,
                allomorph.Form.Line,
                allomorph.Form.LineText,
                allomorph.SenseGloss.Value,
                allomorph.SenseGloss.Line,
                allomorph.SenseGloss.LineText
            )).ToList()
        );
    }

    private static string RelativePath(string root, string path)
    {
        return Path.GetRelativePath(root, path).Replace(Path.DirectorySeparatorChar, '/');
    }

    private static void SearchWords(
        ReplayState state,
        int wordIndex,
        SourceCrosswalk crosswalk,
        IDictionary<string, SourceCrosswalk> solutions
    )
    {
        if (wordIndex == state.Words.Count)
        {
            var signature = crosswalk.Signature();
            solutions.TryAdd(signature, crosswalk);
            return;
        }

        var replay = state.Words[wordIndex];
        var legacy = state.Legacy.Words[wordIndex];
        if (replay.Analyses.Count != legacy.Analyses.Count)
            return;
        SearchWordAnalyses(state, wordIndex, 0, new bool[legacy.Analyses.Count], crosswalk, solutions);
    }

    private static void SearchWordAnalyses(
        ReplayState state,
        int wordIndex,
        int analysisIndex,
        bool[] usedLegacy,
        SourceCrosswalk crosswalk,
        IDictionary<string, SourceCrosswalk> solutions
    )
    {
        if (wordIndex == state.Words.Count)
        {
            var signature = crosswalk.Signature();
            solutions.TryAdd(signature, crosswalk);
            return;
        }

        var replay = state.Words[wordIndex];
        var legacyWord = state.Legacy.Words[wordIndex];
        if (analysisIndex == replay.Analyses.Count)
        {
            SearchWords(state, wordIndex + 1, crosswalk, solutions);
            return;
        }

        var raw = replay.Analyses[analysisIndex];
        var triedLegacyKeys = new HashSet<string>(StringComparer.Ordinal);
        for (var legacyIndex = 0; legacyIndex < legacyWord.Analyses.Count; legacyIndex++)
        {
            if (usedLegacy[legacyIndex])
                continue;
            var oldAnalysis = legacyWord.Analyses[legacyIndex];
            var keySignature = StoredKeySignature(oldAnalysis.Morphemes);
            if (!triedLegacyKeys.Add(keySignature))
                continue;
            if (!TryExtendCrosswalk(raw, oldAnalysis, crosswalk, out var extended))
                continue;

            var equivalentIndex = Array.FindIndex(usedLegacy, candidate => !candidate);
            // Select the first unused row with this key so duplicate legacy rows do not create factorial branches.
            for (var candidateIndex = 0; candidateIndex < legacyWord.Analyses.Count; candidateIndex++)
            {
                if (!usedLegacy[candidateIndex]
                    && StoredKeySignature(legacyWord.Analyses[candidateIndex].Morphemes) == keySignature)
                {
                    equivalentIndex = candidateIndex;
                    break;
                }
            }
            usedLegacy[equivalentIndex] = true;
            SearchWordAnalyses(state, wordIndex, analysisIndex + 1, usedLegacy, extended, solutions);
            usedLegacy[equivalentIndex] = false;
        }
    }

    private static bool TryExtendCrosswalk(
        ReplayAnalysis replay,
        LegacyAnalysis legacy,
        SourceCrosswalk current,
        out SourceCrosswalk extended
    )
    {
        extended = current;
        if (replay.StoredMorphIds.Count != legacy.Morphemes.Count)
            return false;
        var candidate = current.Clone();
        for (var index = 0; index < replay.StoredMorphIds.Count; index++)
        {
            var raw = replay.StoredMorphIds[index];
            var stable = legacy.Morphemes[index];
            if ((raw.InflectionTypeId is null) != (stable.InflectionTypeGuid is null))
                return false;
            if (!candidate.TryAdd("allomorph", raw.AllomorphId, stable.AllomorphGuid)
                || !candidate.TryAdd("morpheme", raw.MorphemeId, stable.MsaGuid))
                return false;
            if (raw.InflectionTypeId is not null
                && !candidate.TryAdd("inflection-type", raw.InflectionTypeId, stable.InflectionTypeGuid!))
                return false;
        }
        extended = candidate;
        return true;
    }

    private static IdentityCapture CreateIdentityCapture(
        ReplayState state,
        string engineVersion,
        string sourceHash,
        string projectHash,
        string machineHash,
        string hermitCrabHash
    )
    {
        var crosswalk = state.Solutions.Single();
        var capturedWords = state.Words.Select(replay => new CapturedWord(
            replay.Word,
            replay.Analyses.Select(analysis =>
            {
                var key = ProjectStoredKey(analysis, crosswalk);
                var stableMorphemes = analysis.MachineMorphemeIds
                    .Select(id => crosswalk.Get("morpheme", id))
                    .ToArray();
                return new CapturedAnalysis(
                    key,
                    new MachineIdentity(stableMorphemes, analysis.RootIndex, analysis.Category),
                    analysis.MachineMorphemeIds,
                    analysis.MachineCategoryId,
                    analysis.StoredMorphIds
                );
            }).ToList(),
            replay.EngineError
        )).ToList();

        return new IdentityCapture(
            3,
            "underdefined-machine-word-analysis/v2",
            state.CaseName,
            state.State,
            engineVersion,
            state.Categories,
            new CaptureProvenance(
                sourceHash,
                projectHash,
                machineHash,
                hermitCrabHash,
                HashFile(state.XmlPath),
                HashFile(state.WordsPath),
                HashFile(state.LegacyPath),
                HashFile(state.InvocationPath),
                state.ParseSolverSolutionCount,
                RelativePath(state.RootPath, state.SourceProjectPath),
                HashFile(state.SourceProjectPath),
                state.CategoryEvidence,
                state.StaticEvidence
            ),
            capturedWords
        );
    }

    private static IReadOnlyList<StoredMorph> ProjectStoredKey(ReplayAnalysis replay, SourceCrosswalk crosswalk)
    {
        return replay.StoredMorphIds.Select(ids => new StoredMorph(
            crosswalk.Get("allomorph", ids.AllomorphId),
            crosswalk.Get("morpheme", ids.MorphemeId),
            ids.InflectionTypeId is null ? null : crosswalk.Get("inflection-type", ids.InflectionTypeId)
        )).ToList();
    }

    private static bool MultisetEqual(
        IEnumerable<IReadOnlyList<StoredMorph>> left,
        IEnumerable<IReadOnlyList<StoredMorph>> right
    )
    {
        return KeyMultiset(left).OrderBy(pair => pair.Key, StringComparer.Ordinal)
            .SequenceEqual(KeyMultiset(right).OrderBy(pair => pair.Key, StringComparer.Ordinal));
    }

    private static Dictionary<string, int> KeyMultiset(IEnumerable<IReadOnlyList<StoredMorph>> keys)
    {
        var result = new Dictionary<string, int>(StringComparer.Ordinal);
        foreach (var key in keys)
        {
            var signature = StoredKeySignature(key);
            result[signature] = result.GetValueOrDefault(signature) + 1;
        }
        return result;
    }

    private static string StoredKeySignature(IEnumerable<StoredMorph> key)
    {
        return string.Join("\u001f", key.Select(morph =>
            $"{morph.AllomorphGuid}\u001e{morph.MsaGuid}\u001e{morph.InflectionTypeGuid ?? "<null>"}"));
    }

    private static string KeyMultisetSummary(IEnumerable<IReadOnlyList<StoredMorph>> keys)
    {
        return JsonSerializer.Serialize(KeyMultiset(keys).OrderBy(pair => pair.Key, StringComparer.Ordinal), JsonOptions);
    }

    private static IEnumerable<(string Key, IReadOnlyList<string> Values)> AmbiguousMappings(ReplayState state)
    {
        var allKeys = state.Solutions.SelectMany(solution => solution.Values.Keys).Distinct().OrderBy(key => key.Kind, StringComparer.Ordinal).ThenBy(key => key.Id, StringComparer.Ordinal);
        foreach (var key in allKeys)
        {
            var values = state.Solutions.Select(solution => solution.Values.GetValueOrDefault(key) ?? "<unmapped>")
                .Distinct(StringComparer.Ordinal).Order(StringComparer.Ordinal).ToArray();
            if (values.Length > 1)
                yield return ($"{key.Kind}:{key.Id}", values);
        }
    }

    private static ConsistencyMismatch Mismatch(
        ReplayState state,
        string word,
        string kind,
        string expected,
        string actual,
        string cause
    ) => new(state.CaseName, state.State, word, kind, null, expected, actual, cause);

    private static string VerifyPinnedDll(JsonElement manifest, string engineDirectory, string fileName)
    {
        var expected = manifest.GetProperty("binaries").GetProperty(fileName).GetProperty("sha256").GetString()
            ?? throw new InvalidOperationException($"missing hash pin for {fileName}");
        var actual = HashFile(Path.Combine(engineDirectory, fileName));
        if (!string.Equals(expected, actual, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException($"{fileName} hash mismatch: expected {expected}, found {actual}");
        return actual;
    }

    private static void VerifyLoadedAssembly(Assembly assembly, string expectedPath, string expectedHash)
    {
        var actualPath = Path.GetFullPath(assembly.Location);
        if (!string.Equals(actualPath, Path.GetFullPath(expectedPath), StringComparison.Ordinal))
            throw new InvalidOperationException($"loaded {assembly.GetName().Name} from unexpected path {actualPath}");
        if (!string.Equals(HashFile(actualPath), expectedHash, StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException($"loaded {assembly.GetName().Name} hash changed during capture");
    }

    private static PartOfSpeechCrosswalk ReadPartOfSpeechSymbols(
        string rootPath,
        string xmlPath,
        string sourceProjectPath
    )
    {
        var xmlDocument = XDocument.Load(xmlPath, LoadOptions.SetLineInfo);
        var projectDocument = XDocument.Load(sourceProjectPath, LoadOptions.SetLineInfo);
        var fieldWorksPartsOfSpeech = projectDocument.Descendants("rt")
            .Where(element => (string?)element.Attribute("class") == "PartOfSpeech")
            .Select(element => new FieldWorksPartOfSpeechSource(
                (string?)element.Attribute("guid")
                    ?? throw new InvalidOperationException("FieldWorks PartOfSpeech has no GUID"),
                element,
                ReadTextEvidenceMany(element.Element("Name")
                    ?? throw new InvalidOperationException("FieldWorks PartOfSpeech has no Name"), sourceProjectPath),
                ReadTextEvidenceMany(element.Element("Abbreviation")
                    ?? throw new InvalidOperationException("FieldWorks PartOfSpeech has no Abbreviation"), sourceProjectPath)
            ))
            .ToList();
        var symbols = new List<PartOfSpeechSymbol>();
        var evidence = new List<PartOfSpeechCrosswalkEvidence>();

        foreach (var element in xmlDocument.Descendants("PartsOfSpeech").Elements("PartOfSpeech"))
        {
            var idAttribute = element.Attribute("id")
                ?? throw new InvalidOperationException("HC XML PartOfSpeech has no id");
            var nameElement = element.Element("Name")
                ?? throw new InvalidOperationException("HC XML PartOfSpeech has no Name");
            var machineId = idAttribute.Value.Trim();
            var name = ReadTextEvidence(nameElement, xmlPath);
            var matches = fieldWorksPartsOfSpeech
                .Select(candidate => new
                {
                    Candidate = candidate,
                    Name = candidate.Names.FirstOrDefault(item => item.Value == name.Value),
                    Abbreviation = candidate.Abbreviations.FirstOrDefault(item => item.Value == name.Value),
                })
                .Where(match => match.Name is not null && match.Abbreviation is not null)
                .ToList();
            if (matches.Count != 1)
                throw new InvalidOperationException(
                    $"HC POS {machineId}:{name.Value} matched {matches.Count} FieldWorks PartOfSpeech objects by Name and Abbreviation"
                );

            var match = matches[0];
            var matchedName = match.Name
                ?? throw new InvalidOperationException("unique FieldWorks POS match lost its Name evidence");
            var matchedAbbreviation = match.Abbreviation
                ?? throw new InvalidOperationException("unique FieldWorks POS match lost its Abbreviation evidence");
            var stableGuid = match.Candidate.Guid;
            symbols.Add(new PartOfSpeechSymbol(machineId, name.Value, stableGuid));
            evidence.Add(new PartOfSpeechCrosswalkEvidence(
                machineId,
                name.Value,
                stableGuid,
                RelativePath(rootPath, xmlPath),
                HashFile(xmlPath),
                GetLine(idAttribute),
                ReadLineText(xmlPath, GetLine(idAttribute)),
                name.Line,
                name.LineText,
                RelativePath(rootPath, sourceProjectPath),
                HashFile(sourceProjectPath),
                "PartOfSpeech",
                GetLine(match.Candidate.Element),
                ReadLineText(sourceProjectPath, GetLine(match.Candidate.Element)),
                matchedName.Line,
                matchedName.LineText,
                matchedAbbreviation.Line,
                matchedAbbreviation.LineText,
                matches.Select(candidate => candidate.Candidate.Guid).ToList()
            ));
        }

        return new PartOfSpeechCrosswalk(symbols, evidence);
    }

    private static string ReadMachineMorphemeId(SIL.Machine.Morphology.HermitCrab.Morpheme morpheme)
    {
        if (!string.IsNullOrEmpty(morpheme.Id))
            return morpheme.Id;
        if (morpheme.Properties.TryGetValue("ID", out var sourceId) && sourceId is not null)
            return sourceId.ToString()!;
        throw new InvalidOperationException("parsed C# morpheme has no stable XML or source object id");
    }

    private static string ReadPropertyId(IDictionary<string, object> properties, string propertyName)
    {
        if (properties.TryGetValue(propertyName, out var value) && value is not null)
            return value.ToString()!;
        throw new InvalidOperationException($"parsed C# source object has no {propertyName} property");
    }

    private static string? ReadOptionalPropertyId(IDictionary<string, object> properties, string propertyName)
    {
        return properties.TryGetValue(propertyName, out var value) && value is not null ? value.ToString() : null;
    }

    private static string HashFile(string path) => Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant();

    private sealed class ReplayState(
        string caseName,
        string state,
        string rootPath,
        string xmlPath,
        string wordsPath,
        string legacyPath,
        string invocationPath,
        string sourceProjectPath,
        LegacyCapture legacy,
        IReadOnlyList<PartOfSpeechSymbol> categories,
        IReadOnlyList<PartOfSpeechCrosswalkEvidence> categoryEvidence,
        IReadOnlyList<ReplayWord> words
    )
    {
        public string CaseName { get; } = caseName;
        public string State { get; } = state;
        public string RootPath { get; } = rootPath;
        public string XmlPath { get; } = xmlPath;
        public string WordsPath { get; } = wordsPath;
        public string LegacyPath { get; } = legacyPath;
        public string InvocationPath { get; } = invocationPath;
        public string SourceProjectPath { get; } = sourceProjectPath;
        public LegacyCapture Legacy { get; } = legacy;
        public IReadOnlyList<PartOfSpeechSymbol> Categories { get; } = categories;
        public IReadOnlyList<PartOfSpeechCrosswalkEvidence> CategoryEvidence { get; } = categoryEvidence;
        public IReadOnlyList<ReplayWord> Words { get; } = words;
        public List<SourceCrosswalk> Solutions { get; set; } = [];
        public int ParseSolverSolutionCount { get; set; }
        public List<SourceCrosswalkEvidence> StaticEvidence { get; set; } = [];
        public List<ConsistencyMismatch> StaticFailures { get; } = [];
    }

    private sealed record ReplayWord(string Word, IReadOnlyList<ReplayAnalysis> Analyses, string? EngineError);

    private sealed record ReplayAnalysis(
        Word Parse,
        IReadOnlyList<MachineStoredMorphIds> StoredMorphIds,
        IReadOnlyList<string> MachineMorphemeIds,
        string? MachineCategoryId,
        int RootIndex,
        string? Category
    );

    private sealed record PartOfSpeechCrosswalk(
        IReadOnlyList<PartOfSpeechSymbol> Symbols,
        IReadOnlyList<PartOfSpeechCrosswalkEvidence> Evidence
    );

    private sealed record FieldWorksPartOfSpeechSource(
        string Guid,
        XElement Element,
        IReadOnlyList<XmlTextEvidence> Names,
        IReadOnlyList<XmlTextEvidence> Abbreviations
    );

    private sealed record XmlTextEvidence(string Value, int Line, string LineText);

    private sealed record XmlEntryEvidence(string SourceId, XElement SourceIdElement, XmlTextEvidence Gloss);

    private sealed record XmlAllomorphEvidence(
        string SourceId,
        XElement SourceIdElement,
        XmlTextEvidence Form,
        XmlEntryEvidence Entry
    );

    private sealed record HcSourceEvidence(
        IReadOnlyDictionary<string, XmlEntryEvidence> EntriesByMorphemeId,
        IReadOnlyDictionary<string, XmlAllomorphEvidence> Allomorphs
    );

    private sealed record FieldWorksObjectEvidence(
        string Guid,
        string ClassName,
        string? OwnerGuid,
        int Line,
        string LineText,
        IReadOnlyList<XmlTextEvidence> Forms,
        IReadOnlyList<XmlTextEvidence> SenseGlosses
    );

    private sealed record FieldWorksSourceEvidence(
        IReadOnlyDictionary<string, FieldWorksObjectEvidence> Objects
    );

    private sealed record MatchedAllomorph(
        FieldWorksObjectEvidence Object,
        XmlTextEvidence Form,
        XmlTextEvidence SenseGloss
    );

    private sealed record MatchedMorpheme(
        FieldWorksObjectEvidence Object,
        XmlTextEvidence SenseGloss
    );

    private sealed class SourceCrosswalk
    {
        public Dictionary<SourceId, string> Values { get; } = new();

        public SourceCrosswalk Clone()
        {
            var clone = new SourceCrosswalk();
            foreach (var pair in Values)
                clone.Values.Add(pair.Key, pair.Value);
            return clone;
        }

        public bool TryAdd(string kind, string id, string stableKey)
        {
            var key = new SourceId(kind, id);
            if (Values.TryGetValue(key, out var previous))
                return previous == stableKey;
            Values.Add(key, stableKey);
            return true;
        }

        public string Get(string kind, string id) => Values.TryGetValue(new SourceId(kind, id), out var key)
            ? key
            : throw new InvalidOperationException($"no solved {kind} crosswalk entry for source id {id}");

        public string Signature() => string.Join("\u001f", Values.OrderBy(pair => pair.Key.Kind, StringComparer.Ordinal)
            .ThenBy(pair => pair.Key.Id, StringComparer.Ordinal)
            .Select(pair => $"{pair.Key.Kind}\u001e{pair.Key.Id}\u001e{pair.Value}"));
    }

    private readonly record struct SourceId(string Kind, string Id);
}

internal sealed class LegacyCapture
{
    [JsonPropertyName("words")]
    public List<LegacyWord> Words { get; set; } = [];
}

internal sealed class LegacyWord
{
    [JsonPropertyName("word")]
    public string Word { get; set; } = "";
    [JsonPropertyName("analyses")]
    public List<LegacyAnalysis> Analyses { get; set; } = [];
    [JsonPropertyName("engineError")]
    public string? EngineError { get; set; }
    [JsonPropertyName("projectedEngineError")]
    public string? ProjectedEngineError { get; set; }
    [JsonPropertyName("projectedAnalysisCount")]
    public int? ProjectedAnalysisCount { get; set; }
    [JsonPropertyName("projectionAgrees")]
    public bool? ProjectionAgrees { get; set; }
}

internal sealed class LegacyAnalysis
{
    [JsonPropertyName("morphemes")]
    public List<StoredMorph> Morphemes { get; set; } = [];
}

internal sealed record StoredMorph(
    [property: JsonPropertyName("allomorphGuid")] string AllomorphGuid,
    [property: JsonPropertyName("msaGuid")] string MsaGuid,
    [property: JsonPropertyName("inflectionTypeGuid")] string? InflectionTypeGuid
);

internal sealed record PartOfSpeechSymbol(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("stableGuid")] string StableGuid
);

internal sealed record PartOfSpeechCrosswalkEvidence(
    [property: JsonPropertyName("machineId")] string MachineId,
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("stableGuid")] string StableGuid,
    [property: JsonPropertyName("hcXmlPath")] string HcXmlPath,
    [property: JsonPropertyName("hcXmlSha256")] string HcXmlSha256,
    [property: JsonPropertyName("hcXmlIdLine")] int HcXmlIdLine,
    [property: JsonPropertyName("hcXmlIdLineText")] string HcXmlIdLineText,
    [property: JsonPropertyName("hcXmlNameLine")] int HcXmlNameLine,
    [property: JsonPropertyName("hcXmlNameLineText")] string HcXmlNameLineText,
    [property: JsonPropertyName("fieldWorksProjectPath")] string FieldWorksProjectPath,
    [property: JsonPropertyName("fieldWorksProjectSha256")] string FieldWorksProjectSha256,
    [property: JsonPropertyName("fieldWorksCandidateClass")] string FieldWorksCandidateClass,
    [property: JsonPropertyName("fieldWorksCandidateLine")] int FieldWorksCandidateLine,
    [property: JsonPropertyName("fieldWorksCandidateLineText")] string FieldWorksCandidateLineText,
    [property: JsonPropertyName("fieldWorksNameLine")] int FieldWorksNameLine,
    [property: JsonPropertyName("fieldWorksNameLineText")] string FieldWorksNameLineText,
    [property: JsonPropertyName("fieldWorksAbbreviationLine")] int FieldWorksAbbreviationLine,
    [property: JsonPropertyName("fieldWorksAbbreviationLineText")] string FieldWorksAbbreviationLineText,
    [property: JsonPropertyName("matchingCandidateGuids")] IReadOnlyList<string> MatchingCandidateGuids
);

internal sealed record MachineIdentity(
    [property: JsonPropertyName("morphemes")] IReadOnlyList<string> Morphemes,
    [property: JsonPropertyName("rootIndex")] int RootIndex,
    [property: JsonPropertyName("category")] string? Category
);

internal sealed record CapturedAnalysis(
    [property: JsonPropertyName("storedAnalysisKey")] IReadOnlyList<StoredMorph> StoredAnalysisKey,
    [property: JsonPropertyName("identity")] MachineIdentity Identity,
    [property: JsonPropertyName("machineMorphemeIds")] IReadOnlyList<string> MachineMorphemeIds,
    [property: JsonPropertyName("machineCategoryId")] string? MachineCategoryId,
    [property: JsonPropertyName("machineStoredMorphIds")] IReadOnlyList<MachineStoredMorphIds> MachineStoredMorphIds
);

internal sealed record MachineStoredMorphIds(
    [property: JsonPropertyName("allomorphId")] string AllomorphId,
    [property: JsonPropertyName("morphemeId")] string MorphemeId,
    [property: JsonPropertyName("inflectionTypeId")] string? InflectionTypeId
);

internal sealed record CapturedWord(
    [property: JsonPropertyName("word")] string Word,
    [property: JsonPropertyName("analyses")] IReadOnlyList<CapturedAnalysis> Analyses,
    [property: JsonPropertyName("engineError")] string? EngineError
);

internal sealed record CaptureProvenance(
    [property: JsonPropertyName("harnessSourceSha256")] string HarnessSourceSha256,
    [property: JsonPropertyName("harnessProjectSha256")] string HarnessProjectSha256,
    [property: JsonPropertyName("silMachineSha256")] string SilMachineSha256,
    [property: JsonPropertyName("hermitCrabSha256")] string HermitCrabSha256,
    [property: JsonPropertyName("inputXmlSha256")] string InputXmlSha256,
    [property: JsonPropertyName("inputWordsSha256")] string InputWordsSha256,
    [property: JsonPropertyName("legacyCaptureSha256")] string LegacyCaptureSha256,
    [property: JsonPropertyName("inputInvocationSha256")] string InputInvocationSha256,
    [property: JsonPropertyName("parseSolverSolutionCount")] int ParseSolverSolutionCount,
    [property: JsonPropertyName("sourceProjectPath")] string SourceProjectPath,
    [property: JsonPropertyName("sourceProjectSha256")] string SourceProjectSha256,
    [property: JsonPropertyName("partOfSpeechCrosswalkEvidence")] IReadOnlyList<PartOfSpeechCrosswalkEvidence> PartOfSpeechCrosswalkEvidence,
    [property: JsonPropertyName("staticCrosswalkEvidence")] IReadOnlyList<SourceCrosswalkEvidence> StaticCrosswalkEvidence
);

internal sealed record PairedAllomorphEvidence(
    [property: JsonPropertyName("guid")] string Guid,
    [property: JsonPropertyName("className")] string ClassName,
    [property: JsonPropertyName("candidateLine")] int CandidateLine,
    [property: JsonPropertyName("candidateLineText")] string CandidateLineText,
    [property: JsonPropertyName("form")] string Form,
    [property: JsonPropertyName("formLine")] int FormLine,
    [property: JsonPropertyName("formLineText")] string FormLineText,
    [property: JsonPropertyName("senseGloss")] string SenseGloss,
    [property: JsonPropertyName("senseGlossLine")] int SenseGlossLine,
    [property: JsonPropertyName("senseGlossLineText")] string SenseGlossLineText
);

internal sealed record SourceCrosswalkEvidence(
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("sourceId")] string SourceId,
    [property: JsonPropertyName("stableGuid")] string StableGuid,
    [property: JsonPropertyName("parseSolverCandidates")] IReadOnlyList<string> ParseSolverCandidates,
    [property: JsonPropertyName("resolution")] string Resolution,
    [property: JsonPropertyName("hcXmlPath")] string HcXmlPath,
    [property: JsonPropertyName("hcXmlSha256")] string HcXmlSha256,
    [property: JsonPropertyName("hcXmlIdLine")] int HcXmlIdLine,
    [property: JsonPropertyName("hcXmlIdLineText")] string HcXmlIdLineText,
    [property: JsonPropertyName("hcXmlForm")] string? HcXmlForm,
    [property: JsonPropertyName("hcXmlFormLine")] int? HcXmlFormLine,
    [property: JsonPropertyName("hcXmlFormLineText")] string? HcXmlFormLineText,
    [property: JsonPropertyName("hcXmlGloss")] string HcXmlGloss,
    [property: JsonPropertyName("hcXmlGlossLine")] int HcXmlGlossLine,
    [property: JsonPropertyName("hcXmlGlossLineText")] string HcXmlGlossLineText,
    [property: JsonPropertyName("fieldWorksProjectPath")] string FieldWorksProjectPath,
    [property: JsonPropertyName("fieldWorksProjectSha256")] string FieldWorksProjectSha256,
    [property: JsonPropertyName("fieldWorksCandidateClass")] string FieldWorksCandidateClass,
    [property: JsonPropertyName("fieldWorksCandidateLine")] int FieldWorksCandidateLine,
    [property: JsonPropertyName("fieldWorksCandidateLineText")] string FieldWorksCandidateLineText,
    [property: JsonPropertyName("fieldWorksForm")] string? FieldWorksForm,
    [property: JsonPropertyName("fieldWorksFormLine")] int? FieldWorksFormLine,
    [property: JsonPropertyName("fieldWorksFormLineText")] string? FieldWorksFormLineText,
    [property: JsonPropertyName("fieldWorksSenseGloss")] string FieldWorksSenseGloss,
    [property: JsonPropertyName("fieldWorksSenseGlossLine")] int FieldWorksSenseGlossLine,
    [property: JsonPropertyName("fieldWorksSenseGlossLineText")] string FieldWorksSenseGlossLineText,
    [property: JsonPropertyName("pairedAllomorphs")] IReadOnlyList<PairedAllomorphEvidence> PairedAllomorphs
);

internal sealed record IdentityCapture(
    [property: JsonPropertyName("schemaVersion")] int SchemaVersion,
    [property: JsonPropertyName("comparisonProfile")] string ComparisonProfile,
    [property: JsonPropertyName("caseName")] string CaseName,
    [property: JsonPropertyName("state")] string State,
    [property: JsonPropertyName("engineVersion")] string EngineVersion,
    [property: JsonPropertyName("partOfSpeechSymbols")] IReadOnlyList<PartOfSpeechSymbol> PartOfSpeechSymbols,
    [property: JsonPropertyName("provenance")] CaptureProvenance Provenance,
    [property: JsonPropertyName("words")] IReadOnlyList<CapturedWord> Words
);

internal sealed record ConsistencyMismatch(
    [property: JsonPropertyName("caseName")] string CaseName,
    [property: JsonPropertyName("state")] string State,
    [property: JsonPropertyName("word")] string Word,
    [property: JsonPropertyName("kind")] string Kind,
    [property: JsonPropertyName("analysisIndex")] int? AnalysisIndex,
    [property: JsonPropertyName("expected")] string Expected,
    [property: JsonPropertyName("actual")] string Actual,
    [property: JsonPropertyName("cause")] string Cause
);
