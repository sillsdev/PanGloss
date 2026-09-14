using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;
using Newtonsoft.Json.Linq;
using SIL.LCModel;
using SIL.Machine.Morphology.HermitCrab;

namespace XampleProjector
{
	/// <summary>
	/// Structural + HC-engine proof that `project` on an authored project reproduces the fixture
	/// it was authored from. Reusable as a plain subcommand (no LCM/FieldWorks project touched --
	/// only the produced HC XML and the sibling XAMPLE files `project` wrote alongside it), so it
	/// can be driven from Rust later without going through `author` at all. Every structural
	/// expectation (rule/lex-entry shapes, the character table) is derived from the re-parsed
	/// grammar.xml itself, never hardcoded to one fixture; the HC-engine analysis counts are the
	/// one thing this command cannot derive (only an oracle knows the correct count for a word),
	/// so the caller supplies them via repeated <c>--expect WORD=COUNT</c>.
	/// </summary>
	internal static class VerifyParityCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--grammar", out var grammarPath) ||
				!ArgParser.TryGetOption(args, "--hc-xml", out var hcXmlPath) ||
				!ArgParser.TryGetOption(args, "--guid-map", out var guidMapPath))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}

			// The HC-engine check has nothing to check without at least one expectation --
			// silently running it with zero words would look like a pass while proving nothing.
			var expectations = new List<(string Word, int Count)>();
			foreach (var raw in ArgParser.GetOptions(args, "--expect"))
			{
				var eq = raw.IndexOf('=');
				if (eq <= 0 || eq == raw.Length - 1 || !int.TryParse(raw.Substring(eq + 1), out var count))
				{
					Console.Error.WriteLine("verify-parity: --expect \"{0}\" is not shaped WORD=COUNT.", raw);
					return ExitCodes.Usage;
				}
				expectations.Add((raw.Substring(0, eq), count));
			}
			if (expectations.Count == 0)
			{
				Console.Error.WriteLine("verify-parity: at least one --expect WORD=COUNT is required (the HC-engine check has no oracle-independent way to know a correct analysis count).");
				Program.WriteUsage();
				return ExitCodes.Usage;
			}

			GrammarModel grammar;
			try
			{
				grammar = GrammarParser.Parse(LoadXml(grammarPath));
			}
			catch (GrammarAuthorException ex)
			{
				Console.Error.WriteLine("Parity mismatch: grammar.xml no longer parses under the supported subset: {0}", ex.Message);
				return ExitCodes.ParityMismatch;
			}

			var guidMapDoc = JObject.Parse(File.ReadAllText(guidMapPath));
			Dictionary<string, Guid> guidMap;
			try
			{
				guidMap = ((JObject)guidMapDoc[Fields.GuidMap]).Properties()
					.ToDictionary(p => p.Name, p => Guid.Parse((string)p.Value));
			}
			catch (Exception ex) when (ex is FormatException || ex is NullReferenceException)
			{
				Console.Error.WriteLine("Parity mismatch: --guid-map \"{0}\" has no valid \"{1}\" object: {2}", guidMapPath, Fields.GuidMap, ex.Message);
				return ExitCodes.ParityMismatch;
			}
			var projectPathRelative = (string)guidMapDoc[Fields.ProjectPath];
			if (projectPathRelative == null)
			{
				Console.Error.WriteLine("Parity mismatch: --guid-map \"{0}\" has no \"{1}\"", guidMapPath, Fields.ProjectPath);
				return ExitCodes.ParityMismatch;
			}
			var projectPath = Path.Combine(Path.GetDirectoryName(Path.GetFullPath(guidMapPath)) ?? ".", projectPathRelative);

			var hcDoc = LoadXml(hcXmlPath);
			var report = new JObject();
			try
			{
				CheckMorphologicalRules(hcDoc, grammar, report);
				CheckLexicalEntries(hcDoc, grammar, report);
				CheckCharacterTable(hcDoc, grammar, report);
				CheckSlotOrder(hcDoc, grammar, report);
				CheckXampleFiles(hcXmlPath, grammar, report);
				CheckHcEngine(hcXmlPath, expectations, report);

				// The structural checks above read only grammar.xml and the HC-XML/XAMPLE files
				// `project` wrote; the guid map is only meaningful against the live authored
				// project, so it is the one check that has to open LCM.
				var lcmExit = FieldWorksSession.Run(fieldWorksDir, projectPath, (cache, logger) =>
				{
					CheckAuthoredProject(cache, grammar, guidMap, report);
					return ExitCodes.Ok;
				});
				if (lcmExit != ExitCodes.Ok)
					return lcmExit;
			}
			catch (ParityMismatchException ex)
			{
				Console.Error.WriteLine("Parity mismatch: {0}", ex.Message);
				return ExitCodes.ParityMismatch;
			}

			report[Fields.SchemaVersion] = SchemaVersion.Current;
			report["mode"] = "verify-parity";
			Console.WriteLine(report.ToString(Newtonsoft.Json.Formatting.Indented));
			return ExitCodes.Ok;
		}

		private sealed class ParityMismatchException : Exception
		{
			internal ParityMismatchException(string message) : base(message)
			{
			}
		}

		private static XDocument LoadXml(string path)
		{
			var settings = new XmlReaderSettings { DtdProcessing = DtdProcessing.Ignore };
			using (var reader = XmlReader.Create(path, settings))
				return XDocument.Load(reader);
		}

		private static XElement Stratum(XDocument hcDoc) => hcDoc.Root.Element("Language").Element("Strata").Element("Stratum");

		/// <summary>
		/// Expected InsertSegments is derived from each traced-back fixture subrule's own
		/// InsertShape, not hardcoded: HCLoader represents an affix's underlying shape with a
		/// literal "+" marking where it attaches (HCLoader.cs:2712) -- a PREFIX's shape round-trips
		/// with the marker trailing (fixture "x" -> "x+", confirmed empirically via a live
		/// author+project run). No suffix fixture has been round-tripped yet (see README's coverage
		/// table), so a leading marker ("+x") is this command's best-understood but UNCONFIRMED
		/// mirror of that same convention for a suffix: when a suffix subrule is checked,
		/// <c>report["insertConventionConfirmed"]</c> is set false and a console line names the
		/// gap, rather than a match reporting success indistinguishable from the confirmed prefix case.
		/// </summary>
		private static void CheckMorphologicalRules(XDocument hcDoc, GrammarModel grammar, JObject report)
		{
			var rules = Stratum(hcDoc).Element("MorphologicalRuleDefinitions")?.Elements("MorphologicalRule").ToList()
				?? new List<XElement>();
			var expectedCount = grammar.MorphologicalRules.Count;
			if (rules.Count != expectedCount)
				throw new ParityMismatchException($"expected {expectedCount} MorphologicalRule elements, found {rules.Count}");

			var insertConventionConfirmed = true;
			foreach (var rule in rules)
			{
				var ruleIdAttr = (string)rule.Attribute("id");
				var gloss = (string)rule.Element("Gloss");
				var fixtureRule = gloss != null ? grammar.MorphologicalRules.Values.FirstOrDefault(r => r.MorphemeId == gloss) : null;
				if (fixtureRule == null)
					throw new ParityMismatchException($"MorphologicalRule id=\"{ruleIdAttr}\": Gloss \"{gloss}\" does not trace back to a fixture MorphemeId");

				var producedSubrules = rule.Element("MorphologicalSubrules").Elements("MorphologicalSubrule").ToList();
				if (producedSubrules.Count != fixtureRule.Subrules.Count)
				{
					throw new ParityMismatchException(
						$"MorphologicalRule id=\"{ruleIdAttr}\" (fixture \"{fixtureRule.Id}\"): expected {fixtureRule.Subrules.Count} subrule(s), found {producedSubrules.Count}");
				}
				for (var i = 0; i < producedSubrules.Count; i++)
				{
					var fixtureSubrule = fixtureRule.Subrules[i];
					if (!fixtureSubrule.IsPrefix && insertConventionConfirmed)
					{
						insertConventionConfirmed = false;
						Console.WriteLine("verify-parity: suffix InsertSegments convention (\"+\" + shape) is UNCONFIRMED -- no suffix fixture has been round-tripped against a live author+project run yet (see README).");
					}
					var expectedInsert = fixtureSubrule.IsPrefix ? fixtureSubrule.InsertShape + "+" : "+" + fixtureSubrule.InsertShape;
					var insertShape = (string)producedSubrules[i].Element("MorphologicalOutput").Element("InsertSegments")?.Element("PhoneticShape");
					if (insertShape != expectedInsert)
					{
						throw new ParityMismatchException(
							$"MorphologicalRule id=\"{ruleIdAttr}\" subrule {i}: expected InsertSegments \"{expectedInsert}\" (fixture shape \"{fixtureSubrule.InsertShape}\" plus the morph-boundary marker), found \"{insertShape}\"");
					}
				}
			}
			report["morphologicalRuleCount"] = rules.Count;
			report["insertConventionConfirmed"] = insertConventionConfirmed;
		}

		private static void CheckLexicalEntries(XDocument hcDoc, GrammarModel grammar, JObject report)
		{
			var entries = Stratum(hcDoc).Element("LexicalEntries")?.Elements("LexicalEntry").ToList() ?? new List<XElement>();
			if (entries.Count != grammar.LexicalEntries.Count)
				throw new ParityMismatchException($"expected {grammar.LexicalEntries.Count} LexicalEntry elements, found {entries.Count}");
			foreach (var expected in grammar.LexicalEntries)
			{
				var expectedShape = expected.Allomorphs[expected.Allomorphs.Count - 1].Shape;
				var found = entries.Any(e => (string)e.Element("Allomorphs").Elements("Allomorph").First().Element("PhoneticShape") == expectedShape);
				if (!found)
					throw new ParityMismatchException($"no produced LexicalEntry has the expected shape \"{expectedShape}\"");
			}
			report["lexicalEntryCount"] = entries.Count;
		}

		private static void CheckCharacterTable(XDocument hcDoc, GrammarModel grammar, JObject report)
		{
			var table = hcDoc.Root.Element("Language").Element("CharacterDefinitionTable");
			var reps = table.Element("SegmentDefinitions").Elements("SegmentDefinition")
				.SelectMany(sd => sd.Element("Representations").Elements("Representation"))
				.Select(r => (string)r)
				.Distinct()
				.OrderBy(s => s, StringComparer.Ordinal)
				.ToList();
			var expected = grammar.Phonemes.SelectMany(p => p.Representations).Distinct().OrderBy(s => s, StringComparer.Ordinal).ToList();
			if (!reps.SequenceEqual(expected))
				throw new ParityMismatchException($"expected character table {{{string.Join(",", expected)}}}, found {{{string.Join(",", reps)}}}");
			report["segmentCount"] = reps.Count;
		}

		/// <summary>
		/// Confirms the slot order HCLoader.LoadAffixTemplate produces from PrefixSlotsRS: adding
		/// slots to PrefixSlotsRS in the fixture's own declaration order (innermost-first: P1..P12)
		/// comes back REVERSED in the produced AffixTemplate.Slots list -- proved directly by
		/// FieldWorks\Src\LexText\ParserCore\ParserCoreTests\HCLoaderTests.cs's own `AffixTemplate`
		/// test (:729-768): PrefixSlotsRS is built there as [prefixSlot1 (added 1st), prefixSlot2
		/// (added 2nd)], yet the loaded Language.Strata[0].AffixTemplates[0].Slots comes back as
		/// [suffixSlot, prefixSlot2, prefixSlot1] -- prefix slots present in the EXACT REVERSE of
		/// their PrefixSlotsRS insertion order. Each produced rule's Gloss is the traceable
		/// identifier here because HCLoader never lets a fixture force a literal HC MorphemeId (see
		/// docs research on liblcm authoring) -- `author` sets Gloss = the fixture's own MorphemeId
		/// whenever grammar.xml gives no separate Gloss, which is exactly this pilot's shape.
		/// </summary>
		private static void CheckSlotOrder(XDocument hcDoc, GrammarModel grammar, JObject report)
		{
			var template = Stratum(hcDoc).Element("AffixTemplates")?.Element("AffixTemplate");
			var expectedTemplate = grammar.AffixTemplates.Single();
			if (template == null)
				throw new ParityMismatchException("no AffixTemplate found in produced HC XML");

			var slots = template.Elements("Slot").ToList();
			if (slots.Count != expectedTemplate.Slots.Count)
				throw new ParityMismatchException($"expected {expectedTemplate.Slots.Count} slots, found {slots.Count}");
			foreach (var slot in slots)
			{
				if ((string)slot.Attribute("optional") != "true")
					throw new ParityMismatchException($"slot \"{(string)slot.Element("Name")}\" is not optional=\"true\"");
			}

			var rulesById = Stratum(hcDoc).Element("MorphologicalRuleDefinitions").Elements("MorphologicalRule")
				.ToDictionary(r => (string)r.Attribute("id"), r => (string)r.Element("Gloss"));
			var producedOrder = slots.Select(s =>
			{
				var ruleIds = ((string)s.Attribute("morphologicalRules")).Split(' ');
				if (ruleIds.Length != 1)
					throw new ParityMismatchException($"slot \"{(string)s.Element("Name")}\" references {ruleIds.Length} rules, expected 1 for this pilot fixture");
				return rulesById[ruleIds[0]];
			}).ToList();

			var declaredOrder = expectedTemplate.Slots
				.Select(s => grammar.MorphologicalRules[s.RuleIds.Single()].MorphemeId)
				.ToList();
			var reversedOrder = declaredOrder.AsEnumerable().Reverse().ToList();

			string orderingRule;
			if (producedOrder.SequenceEqual(declaredOrder))
				orderingRule = "declaration order (unreversed)";
			else if (producedOrder.SequenceEqual(reversedOrder))
				orderingRule = "exact reverse of declaration order (HCLoader reverses PrefixSlotsRS -- HCLoaderTests.cs:729-768)";
			else
			{
				throw new ParityMismatchException(
					$"slot order [{string.Join(",", producedOrder)}] matches neither declaration order [{string.Join(",", declaredOrder)}] " +
					$"nor its reverse [{string.Join(",", reversedOrder)}]");
			}

			report["slotCount"] = slots.Count;
			report["slotOrder"] = new JArray(producedOrder);
			report["slotOrderingRule"] = orderingRule;
		}

		private static void CheckXampleFiles(string hcXmlPath, GrammarModel grammar, JObject report)
		{
			var dir = Path.GetDirectoryName(Path.GetFullPath(hcXmlPath));
			var fileName = Path.GetFileName(hcXmlPath);
			if (!fileName.EndsWith(".hc.xml", StringComparison.Ordinal))
				throw new ParityMismatchException($"--hc-xml path \"{hcXmlPath}\" does not end in \".hc.xml\"");
			var database = fileName.Substring(0, fileName.Length - ".hc.xml".Length);

			var lexPath = Path.Combine(dir, database + "lex.txt");
			if (!File.Exists(lexPath))
				throw new ParityMismatchException($"expected XAMPLE lexicon file not found: {lexPath}");
			var lexText = File.ReadAllText(lexPath);
			var entryCount = Regex.Matches(lexText, @"^\\lx ", RegexOptions.Multiline).Count;
			// XAMPLE's lex.txt carries one \lx record per authored morph -- every affix rule's
			// subrule (each an MoAffixAllomorph) plus every lexical entry's allomorph.
			var expectedCount = grammar.MorphologicalRules.Values.Sum(r => r.Subrules.Count)
				+ grammar.LexicalEntries.Sum(e => e.Allomorphs.Count);
			if (entryCount != expectedCount)
				throw new ParityMismatchException($"expected {expectedCount} XAMPLE lex.txt entries (\\lx records), found {entryCount}");

			foreach (var suffix in new[] { "adctl.txt", "gram.txt" })
			{
				var path = Path.Combine(dir, database + suffix);
				if (!File.Exists(path) || new FileInfo(path).Length == 0)
					throw new ParityMismatchException($"expected non-empty XAMPLE file: {path}");
			}
			report["xampleLexEntryCount"] = entryCount;
		}

		private static void CheckHcEngine(string hcXmlPath, IReadOnlyList<(string Word, int Count)> expectations, JObject report)
		{
			var language = XmlLanguageLoader.Load(hcXmlPath);
			// Never assign any Morpher property beyond the constructor -- every expected.tsv in
			// machine/conformance was generated the same way (conformance/PROTOCOL.md section 8).
			var morpher = new Morpher(new TraceManager(), language);
			var counts = new JObject();
			foreach (var (word, expectedCount) in expectations)
			{
				var actual = morpher.ParseWord(word).Count();
				if (actual != expectedCount)
					throw new ParityMismatchException($"expected {expectedCount} analyses for \"{word}\", found {actual}");
				counts[word] = actual;
			}
			report["engineAnalysisCounts"] = counts;
		}

		/// <summary>
		/// The only check here that opens the authored LCM project rather than reading grammar.xml
		/// or the projected HC-XML/XAMPLE files: binds --guid-map to the live object graph so a
		/// swapped or emptied guid map is caught here, not silently accepted (see README).
		/// </summary>
		private static void CheckAuthoredProject(LcmCache cache, GrammarModel grammar, Dictionary<string, Guid> guidMap, JObject report)
		{
			var repo = cache.ServiceLocator.GetInstance<ICmObjectRepository>();

			ICmObject Resolve(string fixtureId, string expectedClassName)
			{
				if (!guidMap.TryGetValue(fixtureId, out var guid))
					throw new ParityMismatchException($"guidMap is missing fixture id \"{fixtureId}\" (expected a {expectedClassName})");
				if (!repo.TryGetObject(guid, out var obj))
					throw new ParityMismatchException($"guidMap[\"{fixtureId}\"] = {guid} does not resolve to any object in the authored project");
				if (obj.ClassName != expectedClassName)
					throw new ParityMismatchException($"guidMap[\"{fixtureId}\"] = {guid} resolves to a {obj.ClassName}, expected a {expectedClassName}");
				return obj;
			}

			// (a) every guidMap guid resolves to an object of the expected LCM class.
			foreach (var pos in grammar.PartsOfSpeech)
				Resolve(pos.Id, "PartOfSpeech");
			foreach (var phoneme in grammar.Phonemes)
				Resolve(phoneme.Id, "PhPhoneme");
			foreach (var marker in grammar.BoundaryMarkers)
				Resolve(marker.Id, "PhBdryMarker");
			foreach (var nc in grammar.NaturalClasses)
				Resolve(nc.Id, "PhNCSegments");
			foreach (var feature in grammar.MprFeatures)
				Resolve(feature.Id, "CmPossibility");
			foreach (var rule in grammar.MorphologicalRules.Values)
			{
				Resolve(rule.Id, "LexEntry");
				foreach (var subrule in rule.Subrules)
					Resolve(subrule.Id, "MoAffixAllomorph");
			}
			foreach (var template in grammar.AffixTemplates)
			{
				Resolve(template.Name, "MoInflAffixTemplate");
				foreach (var slot in template.Slots)
					Resolve(slot.Name, "MoInflAffixSlot");
			}
			foreach (var lexEntry in grammar.LexicalEntries)
			{
				Resolve(lexEntry.Id, "LexEntry");
				foreach (var allo in lexEntry.Allomorphs)
					Resolve(allo.Id, "MoStemAllomorph");
			}
			for (var i = 0; i < grammar.MorphemeCoOccurrenceRules.Count; i++)
				Resolve($"morphemeCoOccurrence[{i}]", "MoMorphAdhocProhib");
			for (var i = 0; i < grammar.AllomorphCoOccurrenceRules.Count; i++)
				Resolve($"allomorphCoOccurrence[{i}]", "MoAlloAdhocProhib");

			// (b) the template's PrefixSlotsRS/SuffixSlotsRS guid sequence equals the fixture's
			// slot declaration order, per direction, mapped through guidMap.
			foreach (var template in grammar.AffixTemplates)
			{
				var lcmTemplate = (IMoInflAffixTemplate)Resolve(template.Name, "MoInflAffixTemplate");
				bool SlotIsPrefix(SlotModel slot) => grammar.MorphologicalRules[slot.RuleIds[0]].Subrules[0].IsPrefix;

				var expectedPrefixOrder = template.Slots.Where(SlotIsPrefix).Select(s => guidMap[s.Name]).ToList();
				var actualPrefixOrder = lcmTemplate.PrefixSlotsRS.Select(s => s.Guid).ToList();
				if (!actualPrefixOrder.SequenceEqual(expectedPrefixOrder))
				{
					throw new ParityMismatchException(
						$"AffixTemplate \"{template.Name}\": PrefixSlotsRS guid order [{string.Join(",", actualPrefixOrder)}] " +
						$"does not match the fixture's declared slot order mapped through guidMap [{string.Join(",", expectedPrefixOrder)}]");
				}

				var expectedSuffixOrder = template.Slots.Where(s => !SlotIsPrefix(s)).Select(s => guidMap[s.Name]).ToList();
				var actualSuffixOrder = lcmTemplate.SuffixSlotsRS.Select(s => s.Guid).ToList();
				if (!actualSuffixOrder.SequenceEqual(expectedSuffixOrder))
				{
					throw new ParityMismatchException(
						$"AffixTemplate \"{template.Name}\": SuffixSlotsRS guid order [{string.Join(",", actualSuffixOrder)}] " +
						$"does not match the fixture's declared slot order mapped through guidMap [{string.Join(",", expectedSuffixOrder)}]");
				}
			}

			// (c) each slot rule's MoInflAffMsa.SlotsRC contains exactly its fixture slot's guid.
			foreach (var template in grammar.AffixTemplates)
			{
				foreach (var slot in template.Slots)
				{
					var slotGuid = guidMap[slot.Name];
					foreach (var ruleId in slot.RuleIds)
					{
						var entry = (ILexEntry)Resolve(ruleId, "LexEntry");
						var msa = (IMoInflAffMsa)entry.MorphoSyntaxAnalysesOC.First();
						var slotGuids = msa.SlotsRC.Select(s => s.Guid).ToList();
						if (slotGuids.Count != 1 || slotGuids[0] != slotGuid)
						{
							throw new ParityMismatchException(
								$"MorphologicalRule \"{ruleId}\": MoInflAffMsa.SlotsRC is [{string.Join(",", slotGuids)}], " +
								$"expected exactly [{slotGuid}] (slot \"{slot.Name}\")");
						}
					}
				}
			}

			// (d) each entry's LexemeFormOA + AlternateFormsOS guids match the fixture's allomorph
			// ordering rule (README "author's supported subset": LAST allomorph -> LexemeFormOA,
			// every earlier one -> AlternateFormsOS in order).
			foreach (var lexEntry in grammar.LexicalEntries)
			{
				var entry = (ILexEntry)Resolve(lexEntry.Id, "LexEntry");
				var expectedLexeme = guidMap[lexEntry.Allomorphs[lexEntry.Allomorphs.Count - 1].Id];
				if (entry.LexemeFormOA.Guid != expectedLexeme)
				{
					throw new ParityMismatchException(
						$"LexicalEntry \"{lexEntry.Id}\": LexemeFormOA guid {entry.LexemeFormOA.Guid} does not match the fixture's LAST allomorph guid {expectedLexeme}");
				}
				var expectedAlternates = lexEntry.Allomorphs.Take(lexEntry.Allomorphs.Count - 1).Select(a => guidMap[a.Id]).ToList();
				var actualAlternates = entry.AlternateFormsOS.Select(f => f.Guid).ToList();
				if (!actualAlternates.SequenceEqual(expectedAlternates))
				{
					throw new ParityMismatchException(
						$"LexicalEntry \"{lexEntry.Id}\": AlternateFormsOS guid order [{string.Join(",", actualAlternates)}] " +
						$"does not match the fixture's earlier-allomorph order [{string.Join(",", expectedAlternates)}]");
				}
			}

			report["guidMapVerifiedCount"] = guidMap.Count;
		}
	}
}
