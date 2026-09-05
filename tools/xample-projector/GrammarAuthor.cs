using System;
using System.Collections.Generic;
using System.Linq;
using SIL.LCModel;
using SIL.LCModel.Core.Text;
using SIL.LCModel.DomainServices;
using SIL.LCModel.Infrastructure;

namespace XampleProjector
{
	/// <summary>Everything `author` produced: every created object's guid, and per-class counts.</summary>
	internal sealed class AuthorResult
	{
		internal readonly Dictionary<string, Guid> GuidMap = new Dictionary<string, Guid>();
		internal readonly Dictionary<string, int> Authored = new Dictionary<string, int>();

		internal void Note(string fixtureId, Guid guid, string className)
		{
			// Backstop, not the primary check -- GrammarParser's document-global id scan is what
			// should catch a collision before Author() ever runs; this only fires if that scan
			// missed one, and a plain indexer write here would silently overwrite the earlier entry.
			if (GuidMap.ContainsKey(fixtureId))
				throw new GrammarAuthorException($"duplicate GuidMap key \"{fixtureId}\" (already mapped to {GuidMap[fixtureId]}, now {guid}) -- GrammarParser should have refused this id as a duplicate");
			GuidMap[fixtureId] = guid;
			NoteCountOnly(className);
		}

		/// <summary>Counts an authored object that has no fixture id of its own (e.g. the
		/// auto-added "+" boundary marker) -- still a real object `author` created, so it must
		/// still show up in the response's "authored" counts.</summary>
		internal void NoteCountOnly(string className)
		{
			Authored[className] = Authored.TryGetValue(className, out var n) ? n + 1 : 1;
		}
	}

	/// <summary>
	/// Builds LCM objects from an already-validated <see cref="GrammarModel"/>. Nothing here refuses
	/// -- every construct it touches already passed <see cref="GrammarParser"/>'s checks, so this
	/// class only has to know HOW to represent a supported construct, never whether it can.
	/// </summary>
	internal static class GrammarAuthor
	{
		/// <summary>Text lookups an environment string needs, keyed by fixture id -- independent of LCM guids.</summary>
		private sealed class EnvCtx
		{
			internal LcmCache Cache;
			internal IReadOnlyDictionary<string, string> NaturalClassAbbreviations;
			internal IReadOnlyDictionary<string, string> SegmentRepresentations;
		}

		internal static AuthorResult Author(LcmCache cache, GrammarModel grammar, int xampleMaxPrefixes, int xampleMaxSuffixes, int xampleMaxAnalyses)
		{
			var result = new AuthorResult();
			var envCtx = new EnvCtx
			{
				Cache = cache,
				NaturalClassAbbreviations = grammar.NaturalClasses.ToDictionary(c => c.Id, c => c.Name),
				SegmentRepresentations = grammar.Phonemes.ToDictionary(p => p.Id, p => p.Representations[0]),
			};
			NonUndoableUnitOfWorkHelper.Do(cache.ActionHandlerAccessor, () =>
			{
				// CreateCacheWithNewBlankLangProj's DefaultVernacularWritingSystem setter only
				// updates CurrentVernacularWritingSystems, never the base VernacularWritingSystems
				// collection -- unlike analysis, which BootstrapWritingSystems populates via
				// AddToCurrentAnalysisWritingSystems (both lists). Without this, reopening the
				// project (LcmCache.CreateCacheFromExistingData) NREs inside
				// BackendProvider.BootstrapExtantSystem, which reads LangProject.VernWss.
				// Confirmed against HCLoaderTests.cs:129's own identical fixup.
				cache.ServiceLocator.WritingSystems.VernacularWritingSystems.Add(cache.ServiceLocator.WritingSystems.DefaultVernacularWritingSystem);

				var posMap = CreatePartsOfSpeech(cache, grammar, result);
				var segMap = CreatePhonemes(cache, grammar, result);
				CreateBoundaryMarkers(cache, grammar, result);
				var natClassMap = CreateNaturalClasses(cache, grammar, segMap, result);
				var mprMap = CreateMprFeatures(cache, grammar, result);

				var ruleMsaMap = new Dictionary<string, IMoInflAffMsa>();
				var ruleEntryMap = new Dictionary<string, ILexEntry>();
				var alloFormMap = new Dictionary<string, IMoForm>();
				CreateAffixRules(cache, grammar, posMap, mprMap, ruleMsaMap, ruleEntryMap, alloFormMap, result, envCtx);

				var stemEntryMsaMap = new Dictionary<string, IMoStemMsa>();
				CreateStemEntries(cache, grammar, posMap, mprMap, stemEntryMsaMap, alloFormMap, result, envCtx);

				CreateAffixTemplatesAndSlots(cache, grammar, posMap, ruleMsaMap, result);
				CreateCoOccurrenceRules(cache, grammar, ruleMsaMap, stemEntryMsaMap, alloFormMap, result);

				SetParserParameters(cache, grammar, xampleMaxPrefixes, xampleMaxSuffixes, xampleMaxAnalyses);

				// Every construct GrammarParser produced must land in result.GuidMap by now --
				// see ReconcileAuthored's own doc comment for why this is driven off grammar's
				// collections rather than the LCM side just created.
				ReconcileAuthored(grammar, result);
			});
			return result;
		}

		private static Dictionary<string, IPartOfSpeech> CreatePartsOfSpeech(LcmCache cache, GrammarModel grammar, AuthorResult result)
		{
			var factory = cache.ServiceLocator.GetInstance<IPartOfSpeechFactory>();
			var map = new Dictionary<string, IPartOfSpeech>();
			foreach (var pos in grammar.PartsOfSpeech)
			{
				var lcmPos = factory.Create();
				cache.LanguageProject.PartsOfSpeechOA.PossibilitiesOS.Add(lcmPos);
				lcmPos.Name.SetAnalysisDefaultWritingSystem(pos.Name);
				lcmPos.Abbreviation.SetAnalysisDefaultWritingSystem(pos.Name);
				map[pos.Id] = lcmPos;
				result.Note(pos.Id, lcmPos.Guid, "PartOfSpeech");
			}
			return map;
		}

		private static Dictionary<string, IPhPhoneme> CreatePhonemes(LcmCache cache, GrammarModel grammar, AuthorResult result)
		{
			var phonemeSet = cache.ServiceLocator.GetInstance<IPhPhonemeSetFactory>().Create();
			cache.LanguageProject.PhonologicalDataOA.PhonemeSetsOS.Add(phonemeSet);

			var factory = cache.ServiceLocator.GetInstance<IPhPhonemeFactory>();
			var codeFactory = cache.ServiceLocator.GetInstance<IPhCodeFactory>();
			var map = new Dictionary<string, IPhPhoneme>();
			foreach (var phoneme in grammar.Phonemes)
			{
				var lcmPhoneme = factory.Create();
				phonemeSet.PhonemesOC.Add(lcmPhoneme);
				lcmPhoneme.Name.SetVernacularDefaultWritingSystem(phoneme.Representations[0]);
				// The factory auto-creates one PhCode; additional representations get their own.
				lcmPhoneme.CodesOS[0].Representation.SetVernacularDefaultWritingSystem(phoneme.Representations[0]);
				foreach (var extra in phoneme.Representations.Skip(1))
				{
					var code = codeFactory.Create();
					lcmPhoneme.CodesOS.Add(code);
					code.Representation.SetVernacularDefaultWritingSystem(extra);
				}
				map[phoneme.Id] = lcmPhoneme;
				result.Note(phoneme.Id, lcmPhoneme.Guid, "PhPhoneme");
			}
			return map;
		}

		private static void CreateBoundaryMarkers(LcmCache cache, GrammarModel grammar, AuthorResult result)
		{
			var phonemeSet = cache.LanguageProject.PhonologicalDataOA.PhonemeSetsOS[0];
			var codeFactory = cache.ServiceLocator.GetInstance<IPhCodeFactory>();
			void CreateMarker(string representation, string fixtureId)
			{
				var wellKnownGuid = representation == "+" ? LangProjectTags.kguidPhRuleMorphBdry : LangProjectTags.kguidPhRuleWordBdry;
				var lcmMarker = cache.ServiceLocator.GetInstance<IPhBdryMarkerFactory>().Create(wellKnownGuid, phonemeSet);
				var tss = TsStringUtils.MakeString(representation, cache.DefaultAnalWs);
				lcmMarker.Name.set_String(cache.DefaultAnalWs, tss);
				var code = codeFactory.Create();
				lcmMarker.CodesOS.Add(code);
				code.Representation.set_String(cache.DefaultAnalWs, tss);
				if (fixtureId != null)
					result.Note(fixtureId, lcmMarker.Guid, "PhBdryMarker");
				else
					result.NoteCountOnly("PhBdryMarker");
			}

			foreach (var marker in grammar.BoundaryMarkers)
				CreateMarker(marker.Representation, marker.Id);

			// HCLoader.LoadCharacterDefinitionTable indexes the character table by "+" unconditionally
			// (HCLoader.cs:2712, m_morphBdry = m_table["+"]) even when the fixture declares no
			// BoundaryDefinitions at all -- so a "+" marker must always exist, fixture or not.
			if (grammar.BoundaryMarkers.All(m => m.Representation != "+"))
				CreateMarker("+", fixtureId: null);
		}

		private static Dictionary<string, IPhNCSegments> CreateNaturalClasses(LcmCache cache, GrammarModel grammar, Dictionary<string, IPhPhoneme> segMap, AuthorResult result)
		{
			var factory = cache.ServiceLocator.GetInstance<IPhNCSegmentsFactory>();
			var map = new Dictionary<string, IPhNCSegments>();
			foreach (var nc in grammar.NaturalClasses)
			{
				var lcmClass = factory.Create();
				cache.LanguageProject.PhonologicalDataOA.NaturalClassesOS.Add(lcmClass);
				lcmClass.Name.SetAnalysisDefaultWritingSystem(nc.Name);
				// Fixtures carry no separate abbreviation -- environments resolve a class by
				// Abbreviation (HCLoader), so the authored Abbreviation must be this same text,
				// the only identifying text the fixture actually supplies.
				lcmClass.Abbreviation.SetAnalysisDefaultWritingSystem(nc.Name);
				foreach (var segId in nc.SegmentIds)
					lcmClass.SegmentsRC.Add(segMap[segId]);
				map[nc.Id] = lcmClass;
				result.Note(nc.Id, lcmClass.Guid, "PhNCSegments");
			}
			return map;
		}

		private static Dictionary<string, ICmPossibility> CreateMprFeatures(LcmCache cache, GrammarModel grammar, AuthorResult result)
		{
			var factory = cache.ServiceLocator.GetInstance<ICmPossibilityFactory>();
			var map = new Dictionary<string, ICmPossibility>();
			foreach (var feature in grammar.MprFeatures)
			{
				var possibility = factory.Create();
				cache.LanguageProject.MorphologicalDataOA.ProdRestrictOA.PossibilitiesOS.Add(possibility);
				possibility.Name.SetAnalysisDefaultWritingSystem(feature.Text);
				possibility.Abbreviation.SetAnalysisDefaultWritingSystem(feature.Text);
				map[feature.Id] = possibility;
				result.Note(feature.Id, possibility.Guid, "ProdRestrict");
			}
			return map;
		}

		private static void CreateAffixRules(LcmCache cache, GrammarModel grammar, Dictionary<string, IPartOfSpeech> posMap,
			Dictionary<string, ICmPossibility> mprMap, Dictionary<string, IMoInflAffMsa> ruleMsaMap, Dictionary<string, ILexEntry> ruleEntryMap,
			Dictionary<string, IMoForm> alloFormMap, AuthorResult result, EnvCtx envCtx)
		{
			var entryFactory = cache.ServiceLocator.GetInstance<ILexEntryFactory>();
			var affixAllomorphFactory = cache.ServiceLocator.GetInstance<IMoAffixAllomorphFactory>();
			var morphTypeRepo = cache.ServiceLocator.GetInstance<IMoMorphTypeRepository>();
			var prefixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphPrefix);
			var suffixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphSuffix);

			foreach (var rule in grammar.MorphologicalRules.Values)
			{
				var pos = posMap[rule.RequiredPartOfSpeechId];
				var isPrefix = rule.Subrules[0].IsPrefix;
				var gloss = rule.Gloss ?? rule.MorphemeId ?? rule.Id;

				var msa = new SandboxGenericMSA { MsaType = MsaType.kInfl, MainPOS = pos };
				var firstSubrule = rule.Subrules[0];
				var entry = entryFactory.Create(isPrefix ? prefixType : suffixType,
					TsStringUtils.MakeString(firstSubrule.InsertShape, cache.DefaultVernWs), gloss, msa);

				var inflMsa = (IMoInflAffMsa)entry.MorphoSyntaxAnalysesOC.First();
				foreach (var mprId in firstSubrule.RequiredMprFeatureIds)
					inflMsa.FromProdRestrictRC.Add(mprMap[mprId]);

				var firstAllo = (IMoAffixAllomorph)entry.LexemeFormOA;
				AttachEnvironments(envCtx, firstAllo.PhoneEnvRC, firstSubrule.RequiredEnvironments);
				alloFormMap[firstSubrule.Id] = firstAllo;

				foreach (var subrule in rule.Subrules.Skip(1))
				{
					var allo = affixAllomorphFactory.Create();
					entry.AlternateFormsOS.Add(allo);
					allo.Form.SetVernacularDefaultWritingSystem(subrule.InsertShape);
					AttachEnvironments(envCtx, allo.PhoneEnvRC, subrule.RequiredEnvironments);
					alloFormMap[subrule.Id] = allo;
					result.Note(subrule.Id, allo.Guid, "MoAffixAllomorph");
				}

				result.Note(rule.Id, entry.Guid, "LexEntry");
				result.Note(firstSubrule.Id, firstAllo.Guid, "MoAffixAllomorph");
				ruleMsaMap[rule.Id] = inflMsa;
				ruleEntryMap[rule.Id] = entry;
			}
		}

		private static void CreateStemEntries(LcmCache cache, GrammarModel grammar, Dictionary<string, IPartOfSpeech> posMap,
			Dictionary<string, ICmPossibility> mprMap, Dictionary<string, IMoStemMsa> stemEntryMsaMap, Dictionary<string, IMoForm> alloFormMap,
			AuthorResult result, EnvCtx envCtx)
		{
			var entryFactory = cache.ServiceLocator.GetInstance<ILexEntryFactory>();
			var stemAllomorphFactory = cache.ServiceLocator.GetInstance<IMoStemAllomorphFactory>();
			var stemType = cache.ServiceLocator.GetInstance<IMoMorphTypeRepository>().GetObject(MoMorphTypeTags.kguidMorphStem);

			foreach (var lexEntry in grammar.LexicalEntries)
			{
				var pos = lexEntry.PartOfSpeechId != null ? posMap[lexEntry.PartOfSpeechId] : null;
				var gloss = lexEntry.Gloss ?? lexEntry.MorphemeId ?? lexEntry.Id;
				var msa = new SandboxGenericMSA { MsaType = MsaType.kStem, MainPOS = pos };

				// HCLoader emits AlternateForms before LexemeForm, so the fixture's LAST allomorph
				// must be authored as LexemeForm and every earlier one as an AlternateForm, in
				// order, for the round trip to reproduce the fixture's own allomorph order.
				var lexemeAllomorph = lexEntry.Allomorphs[lexEntry.Allomorphs.Count - 1];
				var entry = entryFactory.Create(stemType, TsStringUtils.MakeString(lexemeAllomorph.Shape, cache.DefaultVernWs), gloss, msa);

				var stemMsa = (IMoStemMsa)entry.MorphoSyntaxAnalysesOC.First();
				foreach (var mprId in lexEntry.RuleFeatureIds)
					stemMsa.ProdRestrictRC.Add(mprMap[mprId]);

				var lexemeForm = (IMoStemAllomorph)entry.LexemeFormOA;
				AttachEnvironments(envCtx, lexemeForm.PhoneEnvRC, lexemeAllomorph.RequiredEnvironments);
				alloFormMap[lexemeAllomorph.Id] = lexemeForm;
				result.Note(lexemeAllomorph.Id, lexemeForm.Guid, "MoStemAllomorph");

				for (var i = 0; i < lexEntry.Allomorphs.Count - 1; i++)
				{
					var alternate = lexEntry.Allomorphs[i];
					var allo = stemAllomorphFactory.Create();
					entry.AlternateFormsOS.Add(allo);
					allo.Form.SetVernacularDefaultWritingSystem(alternate.Shape);
					AttachEnvironments(envCtx, allo.PhoneEnvRC, alternate.RequiredEnvironments);
					alloFormMap[alternate.Id] = allo;
					result.Note(alternate.Id, allo.Guid, "MoStemAllomorph");
				}

				result.Note(lexEntry.Id, entry.Guid, "LexEntry");
				stemEntryMsaMap[lexEntry.Id] = stemMsa;
			}
		}

		private static void AttachEnvironments(EnvCtx envCtx, ILcmReferenceCollection<IPhEnvironment> phoneEnvRC, IReadOnlyList<EnvironmentModel> environments)
		{
			foreach (var env in environments)
				phoneEnvRC.Add(BuildEnvironment(envCtx, env));
		}

		private static IPhEnvironment BuildEnvironment(EnvCtx envCtx, EnvironmentModel env)
		{
			var cache = envCtx.Cache;
			var lcmEnv = cache.ServiceLocator.GetInstance<IPhEnvironmentFactory>().Create();
			cache.LanguageProject.PhonologicalDataOA.EnvironmentsOS.Add(lcmEnv);
			var text = FieldWorksEnvironmentSyntax.Build(env, envCtx.NaturalClassAbbreviations, envCtx.SegmentRepresentations);
			lcmEnv.StringRepresentation = TsStringUtils.MakeString(text, cache.DefaultVernWs);
			return lcmEnv;
		}

		private static void CreateAffixTemplatesAndSlots(LcmCache cache, GrammarModel grammar, Dictionary<string, IPartOfSpeech> posMap,
			Dictionary<string, IMoInflAffMsa> ruleMsaMap, AuthorResult result)
		{
			var templateFactory = cache.ServiceLocator.GetInstance<IMoInflAffixTemplateFactory>();
			var slotFactory = cache.ServiceLocator.GetInstance<IMoInflAffixSlotFactory>();

			foreach (var template in grammar.AffixTemplates)
			{
				var pos = posMap[template.RequiredPartOfSpeechId];
				var lcmTemplate = templateFactory.Create();
				pos.AffixTemplatesOS.Add(lcmTemplate);
				lcmTemplate.Name.SetAnalysisDefaultWritingSystem(template.Name);
				result.Note(template.Name, lcmTemplate.Guid, "MoInflAffixTemplate");

				// Slots are added to PrefixSlotsRS/SuffixSlotsRS in the FIXTURE's own declaration
				// order (innermost-first, per the fixture's own convention). HCLoader.LoadAffixTemplate
				// reverses PrefixSlotsRS and lists suffix slots before (reversed) prefix slots --
				// confirmed against HCLoaderTests.cs's own AffixTemplate test (:729-768): PrefixSlotsRS
				// built as [prefixSlot1 (added 1st), prefixSlot2 (added 2nd)] but the resulting
				// Language.Strata[0].AffixTemplates[0].Slots comes back as
				// [suffixSlot, prefixSlot2, prefixSlot1] -- so adding in declaration order here
				// reproduces the fixture's own outward (highest-slot-first) rule-chain order without
				// any manual reversal on this side.
				foreach (var slot in template.Slots)
				{
					var ruleIds = slot.RuleIds;
					// GrammarParser.Parse already refused a slot whose rules mix prefix and suffix.
					var isPrefix = grammar.MorphologicalRules[ruleIds[0]].Subrules[0].IsPrefix;

					var lcmSlot = slotFactory.Create();
					pos.AffixSlotsOC.Add(lcmSlot);
					lcmSlot.Name.SetAnalysisDefaultWritingSystem(slot.Name);
					lcmSlot.Optional = slot.Optional;
					if (isPrefix)
						lcmTemplate.PrefixSlotsRS.Add(lcmSlot);
					else
						lcmTemplate.SuffixSlotsRS.Add(lcmSlot);
					result.Note(slot.Name, lcmSlot.Guid, "MoInflAffixSlot");

					foreach (var ruleId in ruleIds)
						ruleMsaMap[ruleId].SlotsRC.Add(lcmSlot);
				}
			}
		}

		private static void CreateCoOccurrenceRules(LcmCache cache, GrammarModel grammar, Dictionary<string, IMoInflAffMsa> ruleMsaMap,
			Dictionary<string, IMoStemMsa> stemEntryMsaMap, Dictionary<string, IMoForm> alloFormMap, AuthorResult result)
		{
			// GrammarParser.Parse already refused a reference to an unknown morpheme/allomorph id.
			IMoMorphSynAnalysis LookupMsa(string morphemeId) =>
				ruleMsaMap.TryGetValue(morphemeId, out var ruleMsa) ? ruleMsa : stemEntryMsaMap[morphemeId];

			IMoForm LookupAllo(string alloId) => alloFormMap[alloId];

			// The DTD gives MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule no id attribute,
			// so each is keyed by document order for guidMap/reconciliation purposes.
			var morphFactory = cache.ServiceLocator.GetInstance<IMoMorphAdhocProhibFactory>();
			for (var i = 0; i < grammar.MorphemeCoOccurrenceRules.Count; i++)
			{
				var rule = grammar.MorphemeCoOccurrenceRules[i];
				var prohib = morphFactory.Create();
				cache.LanguageProject.MorphologicalDataOA.AdhocCoProhibitionsOC.Add(prohib);
				prohib.FirstMorphemeRA = LookupMsa(rule.PrimaryId);
				foreach (var otherId in rule.OtherIds)
					prohib.RestOfMorphsRS.Add(LookupMsa(otherId));
				prohib.Adjacency = AdjacencyOf(rule.Adjacency);
				prohib.Disabled = !rule.IsActive;
				result.Note($"morphemeCoOccurrence[{i}]", prohib.Guid, "MoMorphAdhocProhib");
			}

			var alloFactory = cache.ServiceLocator.GetInstance<IMoAlloAdhocProhibFactory>();
			for (var i = 0; i < grammar.AllomorphCoOccurrenceRules.Count; i++)
			{
				var rule = grammar.AllomorphCoOccurrenceRules[i];
				var prohib = alloFactory.Create();
				cache.LanguageProject.MorphologicalDataOA.AdhocCoProhibitionsOC.Add(prohib);
				prohib.FirstAllomorphRA = LookupAllo(rule.PrimaryId);
				foreach (var otherId in rule.OtherIds)
					prohib.RestOfAllosRS.Add(LookupAllo(otherId));
				prohib.Adjacency = AdjacencyOf(rule.Adjacency);
				prohib.Disabled = !rule.IsActive;
				result.Note($"allomorphCoOccurrence[{i}]", prohib.Guid, "MoAlloAdhocProhib");
			}
		}

		/// <summary>
		/// Refuses (author.unconsumed-construct) naming the first parsed construct with no
		/// guidMap entry -- walks grammar's own collections, never the LCM side, so a future
		/// parser addition with no authoring code fails loudly instead of being silently dropped.
		/// </summary>
		private static void ReconcileAuthored(GrammarModel grammar, AuthorResult result)
		{
			void Require(string kind, string id)
			{
				if (!result.GuidMap.ContainsKey(id))
					throw new GrammarAuthorException($"author.unconsumed-construct: {kind} \"{id}\" was parsed but never authored");
			}

			foreach (var pos in grammar.PartsOfSpeech)
				Require("PartOfSpeech", pos.Id);
			foreach (var phoneme in grammar.Phonemes)
				Require("SegmentDefinition", phoneme.Id);
			foreach (var marker in grammar.BoundaryMarkers)
				Require("BoundaryDefinition", marker.Id);
			foreach (var nc in grammar.NaturalClasses)
				Require("SegmentNaturalClass", nc.Id);
			foreach (var feature in grammar.MprFeatures)
				Require("MorphologicalPhonologicalRuleFeature", feature.Id);
			foreach (var rule in grammar.MorphologicalRules.Values)
			{
				Require("MorphologicalRule", rule.Id);
				foreach (var subrule in rule.Subrules)
					Require("MorphologicalSubrule", subrule.Id);
			}
			foreach (var template in grammar.AffixTemplates)
			{
				Require("AffixTemplate", template.Name);
				foreach (var slot in template.Slots)
					Require("Slot", slot.Name);
			}
			foreach (var lexEntry in grammar.LexicalEntries)
			{
				Require("LexicalEntry", lexEntry.Id);
				foreach (var allo in lexEntry.Allomorphs)
					Require("Allomorph", allo.Id);
			}
			for (var i = 0; i < grammar.MorphemeCoOccurrenceRules.Count; i++)
				Require("MorphemeCoOccurrenceRule", $"morphemeCoOccurrence[{i}]");
			for (var i = 0; i < grammar.AllomorphCoOccurrenceRules.Count; i++)
				Require("AllomorphCoOccurrenceRule", $"allomorphCoOccurrence[{i}]");
		}

		// HCLoader.GetAdjacency (HCLoader.cs:2241-2255) maps these ints to HC's
		// MorphCoOccurrenceAdjacency. GrammarParser.Parse already refused any other value.
		private static int AdjacencyOf(string adjacency)
		{
			switch (adjacency)
			{
				case "anywhere": return 0;
				case "somewhereToLeft": return 1;
				case "somewhereToRight": return 2;
				case "adjacentToLeft": return 3;
				case "adjacentToRight": return 4;
				default: throw new InvalidOperationException($"unreachable: adjacency \"{adjacency}\" was not validated by GrammarParser");
			}
		}

		private static void SetParserParameters(LcmCache cache, GrammarModel grammar, int maxPrefixes, int maxSuffixes, int maxAnalyses)
		{
			cache.LanguageProject.MorphologicalDataOA.ParserParameters =
				"<ParserParameters><ActiveParser>XAmple</ActiveParser><XAmple>" +
				"<MaxNulls>0</MaxNulls>" +
				$"<MaxPrefixes>{maxPrefixes}</MaxPrefixes>" +
				"<MaxInfixes>0</MaxInfixes>" +
				"<MaxRoots>1</MaxRoots>" +
				$"<MaxSuffixes>{maxSuffixes}</MaxSuffixes>" +
				"<MaxInterfixes>0</MaxInterfixes>" +
				$"<MaxAnalysesToReturn>{maxAnalyses}</MaxAnalysesToReturn>" +
				"</XAmple><HC><NoDefaultCompounding>true</NoDefaultCompounding>" +
				"<AcceptUnspecifiedGraphemes>false</AcceptUnspecifiedGraphemes></HC></ParserParameters>";
		}
	}
}
