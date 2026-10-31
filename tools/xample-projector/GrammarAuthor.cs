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
			// GrammarParser.RegisterId proves every key here is unique before Author() ever runs
			// (build.ps1's "Duplicate id refusal probe" and "Duplicate slot-name refusal probe");
			// reaching this is a defect in that proof, not a refusable grammar.xml.
			if (GuidMap.ContainsKey(fixtureId))
				throw new InvalidOperationException($"unreachable: duplicate GuidMap key \"{fixtureId}\" (already mapped to {GuidMap[fixtureId]}, now {guid})");
			GuidMap[fixtureId] = guid;
			NoteCountOnly(className);
		}

		/// <summary>Maps an extra fixture id (an MSA copy) onto an already-authored object's guid
		/// without counting a new object.</summary>
		internal void NoteAlias(string fixtureId, Guid guid)
		{
			if (GuidMap.ContainsKey(fixtureId))
				throw new InvalidOperationException($"unreachable: duplicate GuidMap key \"{fixtureId}\"");
			GuidMap[fixtureId] = guid;
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

		internal static AuthorResult Author(LcmCache cache, GrammarModel grammar, int xampleMaxPrefixes, int xampleMaxSuffixes, int xampleMaxAnalyses,
			IReadOnlyList<IReadOnlyList<string>> msaCopyGroups = null, IReadOnlyDictionary<string, string> variantOf = null,
			IReadOnlyList<IReadOnlyList<string>> envCopyGroups = null, IReadOnlyList<string> halfProhibs = null)
		{
			envCopyGroups = envCopyGroups ?? Array.Empty<IReadOnlyList<string>>();
			halfProhibs = halfProhibs ?? Array.Empty<string>();
			msaCopyGroups = msaCopyGroups ?? Array.Empty<IReadOnlyList<string>>();
			variantOf = variantOf ?? new Dictionary<string, string>();
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
				CreateAffixRules(cache, grammar, posMap, mprMap, ruleMsaMap, ruleEntryMap, alloFormMap, result, envCtx, msaCopyGroups, envCopyGroups);

				var stemEntryMsaMap = new Dictionary<string, IMoStemMsa>();
				CreateStemEntries(cache, grammar, posMap, mprMap, stemEntryMsaMap, alloFormMap, result, envCtx, msaCopyGroups, variantOf);

				CreateAffixTemplatesAndSlots(cache, grammar, posMap, ruleMsaMap, result);
				CreateCoOccurrenceRules(cache, grammar, ruleMsaMap, stemEntryMsaMap, alloFormMap, result, halfProhibs);

				SetParserParameters(cache, grammar, xampleMaxPrefixes, xampleMaxSuffixes, xampleMaxAnalyses, msaCopyGroups.Count > 0 || variantOf.Count > 0);

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
			Dictionary<string, IMoForm> alloFormMap, AuthorResult result, EnvCtx envCtx,
			IReadOnlyList<IReadOnlyList<string>> msaCopyGroups, IReadOnlyList<IReadOnlyList<string>> envCopyGroups)
		{
			var entryFactory = cache.ServiceLocator.GetInstance<ILexEntryFactory>();
			var copyMembers = CopyMemberToCanonical(msaCopyGroups);
			var envMembers = CopyMemberToCanonical(envCopyGroups);
			var affixAllomorphFactory = cache.ServiceLocator.GetInstance<IMoAffixAllomorphFactory>();
			var morphTypeRepo = cache.ServiceLocator.GetInstance<IMoMorphTypeRepository>();
			var prefixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphPrefix);
			var suffixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphSuffix);

			foreach (var rule in grammar.MorphologicalRules.Values)
			{
				var pos = posMap[rule.RequiredPartOfSpeechId];
				var isPrefix = rule.Subrules[0].IsPrefix;
				var gloss = rule.Gloss ?? rule.MorphemeId ?? rule.Id;

				if (copyMembers.TryGetValue(rule.Id, out var canonicalRuleId))
				{
					// An MSA copy: one more MoInflAffMsa (+ sense) on the canonical rule's entry, sharing
					// its allomorph. MoInflAffMsa is created directly rather than through a sandbox MSA,
					// which would fold it into the canonical entry's equal MSA.
					var canonicalEntry = ruleEntryMap[canonicalRuleId];
					var copyMsa = cache.ServiceLocator.GetInstance<IMoInflAffMsaFactory>().Create();
					canonicalEntry.MorphoSyntaxAnalysesOC.Add(copyMsa);
					copyMsa.PartOfSpeechRA = pos;
					foreach (var mprId in rule.Subrules[0].RequiredMprFeatureIds)
						copyMsa.FromProdRestrictRC.Add(mprMap[mprId]);
					AddSenseWithMsa(cache, canonicalEntry, copyMsa, gloss);
					ruleMsaMap[rule.Id] = copyMsa;
					ruleEntryMap[rule.Id] = canonicalEntry;
					result.NoteAlias(rule.Id, canonicalEntry.Guid);
					// Each subrule of a copy shares the allomorph of the canonical rule's subrule at the same position.
					var canonicalSubrules = grammar.MorphologicalRules[canonicalRuleId].Subrules;
					for (var i = 0; i < rule.Subrules.Count; i++)
					{
						alloFormMap[rule.Subrules[i].Id] = alloFormMap[canonicalSubrules[i].Id];
						result.NoteAlias(rule.Subrules[i].Id, alloFormMap[rule.Subrules[i].Id].Guid);
					}
					continue;
				}

				if (rule.Subrules[0].IsCircumfix)
				{
					CreateCircumfixRule(cache, rule, pos, gloss, mprMap, ruleMsaMap, ruleEntryMap, alloFormMap, result);
					continue;
				}

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
					if (envMembers.TryGetValue(subrule.Id, out var primarySubruleId))
					{
						// One more environment of an allomorph already authored.
						var primaryAllo = (IMoAffixAllomorph)alloFormMap[primarySubruleId];
						AttachEnvironments(envCtx, primaryAllo.PhoneEnvRC, subrule.RequiredEnvironments);
						alloFormMap[subrule.Id] = primaryAllo;
						result.NoteAlias(subrule.Id, primaryAllo.Guid);
						continue;
					}
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

		/// <summary>
		/// A circumfix rule (subrules = every prefix half x suffix half pairing, as HCLoader emits them,
		/// HCLoader.cs:1097-1122) is ONE circumfix entry: a circumfix-type lexeme form plus one prefix and
		/// one suffix alternate form per distinct half. Every pairing subrule maps to the lexeme form,
		/// which is what an ad hoc prohibition names (m_allomorphs is keyed by entry.LexemeFormOA).
		/// </summary>
		private static void CreateCircumfixRule(LcmCache cache, MorphologicalRuleModel rule, IPartOfSpeech pos, string gloss,
			Dictionary<string, ICmPossibility> mprMap, Dictionary<string, IMoInflAffMsa> ruleMsaMap, Dictionary<string, ILexEntry> ruleEntryMap,
			Dictionary<string, IMoForm> alloFormMap, AuthorResult result)
		{
			var morphTypeRepo = cache.ServiceLocator.GetInstance<IMoMorphTypeRepository>();
			var prefixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphPrefix);
			var suffixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphSuffix);
			var circumfixType = morphTypeRepo.GetObject(MoMorphTypeTags.kguidMorphCircumfix);
			var alloFactory = cache.ServiceLocator.GetInstance<IMoAffixAllomorphFactory>();
			var prefixes = rule.Subrules.Select(s => s.InsertShape).Distinct().ToList();
			var suffixes = rule.Subrules.Select(s => s.SuffixShape).Distinct().ToList();

			var msa = new SandboxGenericMSA { MsaType = MsaType.kInfl, MainPOS = pos };
			var entry = cache.ServiceLocator.GetInstance<ILexEntryFactory>().Create(circumfixType,
				TsStringUtils.MakeString(prefixes[0] + "- -" + suffixes[0], cache.DefaultVernWs), gloss, msa);
			var inflMsa = (IMoInflAffMsa)entry.MorphoSyntaxAnalysesOC.First();
			foreach (var mprId in rule.Subrules[0].RequiredMprFeatureIds)
				inflMsa.FromProdRestrictRC.Add(mprMap[mprId]);

			// The factory splits the circumfix lexeme form into its left and right members and creates those two
			// alternate forms itself (LexEntryFactory.Create -> MoAffixAllomorphFactory.CreateCircumfix): the first
			// prefix half and the first suffix half. Every other half is added here.
			alloFormMap["half:" + rule.Id + ":" + prefixes[0]] = entry.AlternateFormsOS[0];
			alloFormMap["half:" + rule.Id + ":" + suffixes[0]] = entry.AlternateFormsOS[1];
			result.NoteCountOnly("MoAffixAllomorph");
			result.NoteCountOnly("MoAffixAllomorph");
			foreach (var shape in prefixes.Skip(1))
				alloFormMap["half:" + rule.Id + ":" + shape] = AddHalf(alloFactory, entry, shape, prefixType, result);
			foreach (var shape in suffixes.Skip(1))
				alloFormMap["half:" + rule.Id + ":" + shape] = AddHalf(alloFactory, entry, shape, suffixType, result);

			var lexemeForm = (IMoForm)entry.LexemeFormOA;
			result.Note(rule.Subrules[0].Id, lexemeForm.Guid, "MoAffixAllomorph");
			alloFormMap[rule.Subrules[0].Id] = lexemeForm;
			foreach (var subrule in rule.Subrules.Skip(1))
			{
				alloFormMap[subrule.Id] = lexemeForm;
				result.NoteAlias(subrule.Id, lexemeForm.Guid);
			}
			result.Note(rule.Id, entry.Guid, "LexEntry");
			ruleMsaMap[rule.Id] = inflMsa;
			ruleEntryMap[rule.Id] = entry;
		}

		private static IMoForm AddHalf(IMoAffixAllomorphFactory factory, ILexEntry entry, string shape, IMoMorphType morphType, AuthorResult result)
		{
			var half = factory.Create();
			entry.AlternateFormsOS.Add(half);
			half.Form.SetVernacularDefaultWritingSystem(shape);
			half.MorphTypeRA = morphType;
			result.NoteCountOnly("MoAffixAllomorph");
			return half;
		}

		private static void CreateStemEntries(LcmCache cache, GrammarModel grammar, Dictionary<string, IPartOfSpeech> posMap,
			Dictionary<string, ICmPossibility> mprMap, Dictionary<string, IMoStemMsa> stemEntryMsaMap, Dictionary<string, IMoForm> alloFormMap,
			AuthorResult result, EnvCtx envCtx,
			IReadOnlyList<IReadOnlyList<string>> msaCopyGroups, IReadOnlyDictionary<string, string> variantOf)
		{
			var entryFactory = cache.ServiceLocator.GetInstance<ILexEntryFactory>();
			var copyMembers = CopyMemberToCanonical(msaCopyGroups);
			var variantEntryByMain = new Dictionary<Guid, ILexEntry>();
			var entryByLexId = new Dictionary<string, ILexEntry>();
			var stemAllomorphFactory = cache.ServiceLocator.GetInstance<IMoStemAllomorphFactory>();
			var stemType = cache.ServiceLocator.GetInstance<IMoMorphTypeRepository>().GetObject(MoMorphTypeTags.kguidMorphStem);

			foreach (var lexEntry in grammar.LexicalEntries)
			{
				var pos = lexEntry.PartOfSpeechId != null ? posMap[lexEntry.PartOfSpeechId] : null;
				var gloss = lexEntry.Gloss ?? lexEntry.MorphemeId ?? lexEntry.Id;

				if (variantOf.TryGetValue(lexEntry.Id, out var mainLexId))
				{
					var mainEntry = entryByLexId[mainLexId];
					var variantAllo = lexEntry.Allomorphs[0];
					if (variantEntryByMain.TryGetValue(mainEntry.Guid, out var variantEntry))
					{
						alloFormMap[variantAllo.Id] = variantEntry.LexemeFormOA;
						result.NoteAlias(variantAllo.Id, variantEntry.LexemeFormOA.Guid);
					}
					else
					{
						// A variant entry has no senses and no MSAs: just a stem allomorph and one
						// variant LexEntryRef whose component and primary lexeme is the main entry.
						variantEntry = entryFactory.Create();
						var vAllo = stemAllomorphFactory.Create();
						variantEntry.LexemeFormOA = vAllo;
						vAllo.Form.SetVernacularDefaultWritingSystem(variantAllo.Shape);
						vAllo.MorphTypeRA = stemType;
						var entryRef = cache.ServiceLocator.GetInstance<ILexEntryRefFactory>().Create();
						variantEntry.EntryRefsOS.Add(entryRef);
						entryRef.RefType = LexEntryRefTags.krtVariant;
						entryRef.ComponentLexemesRS.Add(mainEntry);
						entryRef.PrimaryLexemesRS.Add(mainEntry);
						variantEntryByMain[mainEntry.Guid] = variantEntry;
						alloFormMap[variantAllo.Id] = vAllo;
						result.Note(variantAllo.Id, vAllo.Guid, "MoStemAllomorph");
						result.NoteCountOnly("LexEntryRef");
						result.Note(lexEntry.Id, variantEntry.Guid, "LexEntry");
						entryByLexId[lexEntry.Id] = variantEntry;
						stemEntryMsaMap[lexEntry.Id] = stemEntryMsaMap[mainLexId];
						continue;
					}
					result.NoteAlias(lexEntry.Id, variantEntry.Guid);
					entryByLexId[lexEntry.Id] = variantEntry;
					stemEntryMsaMap[lexEntry.Id] = stemEntryMsaMap[mainLexId];
					continue;
				}

				if (copyMembers.TryGetValue(lexEntry.Id, out var canonicalLexId))
				{
					// An MSA copy: one more MoStemMsa (+ sense) on the canonical entry, sharing its allomorph.
					var canonicalEntry = entryByLexId[canonicalLexId];
					var copyMsa = cache.ServiceLocator.GetInstance<IMoStemMsaFactory>().Create();
					canonicalEntry.MorphoSyntaxAnalysesOC.Add(copyMsa);
					copyMsa.PartOfSpeechRA = pos;
					foreach (var mprId in lexEntry.RuleFeatureIds)
						copyMsa.ProdRestrictRC.Add(mprMap[mprId]);
					AddSenseWithMsa(cache, canonicalEntry, copyMsa, gloss);
					entryByLexId[lexEntry.Id] = canonicalEntry;
					stemEntryMsaMap[lexEntry.Id] = copyMsa;
					result.NoteAlias(lexEntry.Id, canonicalEntry.Guid);
					var canonicalAllo = grammar.LexicalEntries.First(e => e.Id == canonicalLexId).Allomorphs[0];
					alloFormMap[lexEntry.Allomorphs[0].Id] = alloFormMap[canonicalAllo.Id];
					result.NoteAlias(lexEntry.Allomorphs[0].Id, alloFormMap[canonicalAllo.Id].Guid);
					continue;
				}

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
				entryByLexId[lexEntry.Id] = entry;
				stemEntryMsaMap[lexEntry.Id] = stemMsa;
			}
		}

		private static Dictionary<string, string> CopyMemberToCanonical(IReadOnlyList<IReadOnlyList<string>> groups)
		{
			var map = new Dictionary<string, string>();
			foreach (var group in groups)
				foreach (var member in group.Skip(1))
					map[member] = group[0];
			return map;
		}

		private static void AddSenseWithMsa(LcmCache cache, ILexEntry entry, IMoMorphSynAnalysis msa, string gloss)
		{
			var sense = cache.ServiceLocator.GetInstance<ILexSenseFactory>().Create();
			entry.SensesOS.Add(sense);
			sense.MorphoSyntaxAnalysisRA = msa;
			sense.Gloss.SetAnalysisDefaultWritingSystem(gloss);
		}

		/// <summary>
		/// Refuses (GrammarAuthorException) an --msa-copies group that is not a set of one-allomorph
		/// stem entries or one-subrule affix rules with identical allomorph shape: only then are they
		/// what HCLoader would emit for ONE FieldWorks entry with several MSAs.
		/// </summary>
		internal static void ValidateMsaCopyGroups(GrammarModel grammar, IReadOnlyList<IReadOnlyList<string>> groups)
		{
			var seen = new HashSet<string>();
			foreach (var group in groups)
			{
				if (group.Count < 2)
					throw new GrammarAuthorException("author.msa-copies: a group needs at least two ids");
				foreach (var id in group)
					if (!seen.Add(id))
						throw new GrammarAuthorException($"author.msa-copies: id \"{id}\" is in more than one group");
				var entries = group.Select(id => grammar.LexicalEntries.FirstOrDefault(e => e.Id == id)).ToList();
				if (entries.All(e => e != null))
				{
					if (entries.Any(e => e.Allomorphs.Count != 1 || e.Allomorphs[0].RequiredEnvironments.Count != 0
						|| e.Allomorphs[0].Shape != entries[0].Allomorphs[0].Shape))
						throw new GrammarAuthorException($"author.msa-copies: stem entries {string.Join(",", group)} must each have exactly one allomorph with the same shape and no environments");
					continue;
				}
				var rules = group.Select(id => grammar.MorphologicalRules.TryGetValue(id, out var r) ? r : null).ToList();
				if (rules.All(r => r != null))
				{
					// Same subrules position by position (shape, direction, environment count), so the copies share allomorphs by index.
					if (rules.Any(r => r.Subrules.Count != rules[0].Subrules.Count
						|| r.Subrules.Where((s, i) => s.InsertShape != rules[0].Subrules[i].InsertShape || s.SuffixShape != rules[0].Subrules[i].SuffixShape
							|| s.IsPrefix != rules[0].Subrules[i].IsPrefix || s.RequiredEnvironments.Count != rules[0].Subrules[i].RequiredEnvironments.Count).Any()
						|| r.RequiredPartOfSpeechId != rules[0].RequiredPartOfSpeechId
						|| r.OutputPartOfSpeechId != rules[0].OutputPartOfSpeechId))
						throw new GrammarAuthorException($"author.msa-copies: rules {string.Join(",", group)} must have the same subrules (shape, direction, environments) and POS");
					continue;
				}
				throw new GrammarAuthorException($"author.msa-copies: group {string.Join(",", group)} is neither all LexicalEntry ids nor all MorphologicalRule ids");
			}
		}

		/// <summary>
		/// Refuses an --env-copies group that is not subrules of ONE rule with the same shape and direction
		/// (the per-environment copies of one allomorph), listed in document order.
		/// </summary>
		internal static void ValidateEnvCopyGroups(GrammarModel grammar, IReadOnlyList<IReadOnlyList<string>> groups)
		{
			foreach (var group in groups)
			{
				if (group.Count < 2)
					throw new GrammarAuthorException("author.env-copies: a group needs at least two subrule ids");
				var rule = grammar.MorphologicalRules.Values.FirstOrDefault(r => r.Subrules.Any(s => s.Id == group[0]));
				if (rule == null || rule.Subrules[0].IsCircumfix)
					throw new GrammarAuthorException($"author.env-copies: \"{group[0]}\" is not a subrule of an affix rule");
				var ids = rule.Subrules.Select(s => s.Id).ToList();
				var indexes = group.Select(id => ids.IndexOf(id)).ToList();
				if (indexes.Any(i => i < 0) || !indexes.SequenceEqual(indexes.OrderBy(i => i)))
					throw new GrammarAuthorException($"author.env-copies: {string.Join(",", group)} must be subrules of one rule, in document order");
				var subs = group.Select(id => rule.Subrules.First(s => s.Id == id)).ToList();
				if (subs.Any(s => s.InsertShape != subs[0].InsertShape || s.IsPrefix != subs[0].IsPrefix))
					throw new GrammarAuthorException($"author.env-copies: {string.Join(",", group)} must share one shape and direction");
			}
		}

		/// <summary>Refuses a circumfix rule whose subrules are not the full prefix-half x suffix-half cross product.</summary>
		internal static void ValidateCircumfixes(GrammarModel grammar)
		{
			foreach (var rule in grammar.MorphologicalRules.Values.Where(r => r.Subrules.Any(s => s.IsCircumfix)))
			{
				var prefixes = rule.Subrules.Select(s => s.InsertShape).Distinct().ToList();
				var suffixes = rule.Subrules.Select(s => s.SuffixShape).Distinct().ToList();
				var pairs = rule.Subrules.Select(s => s.InsertShape + "|" + s.SuffixShape).Distinct().Count();
				if (rule.Subrules.Any(s => !s.IsCircumfix || s.RequiredEnvironments.Count != 0) || pairs != rule.Subrules.Count
					|| rule.Subrules.Count != prefixes.Count * suffixes.Count || prefixes.Intersect(suffixes).Any())
					throw new GrammarAuthorException($"author.circumfix: rule \"{rule.Id}\" must have exactly one environment-free subrule per prefix half x suffix half pairing (the cross product HCLoader emits), with no half shape shared by both sides");
			}
		}

		internal static void ValidateHalfProhibs(GrammarModel grammar, IReadOnlyList<string> halfProhibs)
		{
			foreach (var h in halfProhibs)
			{
				var eq = h.Split('=');
				var rs = eq.Length == 2 ? eq[1].Split(':') : null;
				MorphologicalRuleModel rule = null;
				if (rs == null || rs.Length != 2 || !grammar.LexicalEntries.Any(e => e.Allomorphs.Any(a => a.Id == eq[0]))
					|| !grammar.MorphologicalRules.TryGetValue(rs[0], out rule) || !rule.Subrules[0].IsCircumfix
					|| !rule.Subrules.Any(s => s.InsertShape == rs[1] || s.SuffixShape == rs[1]))
					throw new GrammarAuthorException($"author.half-prohib: \"{h}\" must be stemAllomorphId=circumfixRuleId:halfShape");
			}
		}

		internal static void ValidateVariants(GrammarModel grammar, IReadOnlyDictionary<string, string> variantOf)
		{
			foreach (var kv in variantOf)
			{
				var v = grammar.LexicalEntries.FirstOrDefault(e => e.Id == kv.Key);
				var m = grammar.LexicalEntries.FirstOrDefault(e => e.Id == kv.Value);
				if (v == null || m == null || v.Allomorphs.Count != 1 || v.Allomorphs[0].RequiredEnvironments.Count != 0)
					throw new GrammarAuthorException($"author.variant-of: \"{kv.Key}={kv.Value}\" needs two LexicalEntry ids and a one-allomorph variant");
				if (grammar.LexicalEntries.ToList().IndexOf(m) > grammar.LexicalEntries.ToList().IndexOf(v))
					throw new GrammarAuthorException($"author.variant-of: main entry \"{kv.Value}\" must precede its variant \"{kv.Key}\" in document order");
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
			Dictionary<string, IMoStemMsa> stemEntryMsaMap, Dictionary<string, IMoForm> alloFormMap, AuthorResult result,
			IReadOnlyList<string> halfProhibs)
		{
			// build.ps1's "Unknown co-occurrence id refusal probe" pins that an unresolvable id
			// here already refused in GrammarParser.Parse.
			IMoMorphSynAnalysis LookupMsa(string morphemeId) =>
				ruleMsaMap.TryGetValue(morphemeId, out var ruleMsa) ? ruleMsa : stemEntryMsaMap[morphemeId];

			IMoForm LookupAllo(string alloId) => alloFormMap[alloId];

			// The DTD gives MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule no id attribute,
			// so each is keyed by document order for guidMap/reconciliation purposes.
			var morphFactory = cache.ServiceLocator.GetInstance<IMoMorphAdhocProhibFactory>();
			var morphProhibBySignature = new Dictionary<string, IMoMorphAdhocProhib>();
			for (var i = 0; i < grammar.MorphemeCoOccurrenceRules.Count; i++)
			{
				var rule = grammar.MorphemeCoOccurrenceRules[i];
				var msig = LookupMsa(rule.PrimaryId).Guid + ">" + string.Join(",", rule.OtherIds.Select(o => LookupMsa(o).Guid.ToString())) + ">" + rule.Adjacency + ">" + rule.IsActive;
				if (morphProhibBySignature.TryGetValue(msig, out var earlierMorph))
				{
					result.NoteAlias($"morphemeCoOccurrence[{i}]", earlierMorph.Guid);
					continue;
				}
				var prohib = morphFactory.Create();
				morphProhibBySignature[msig] = prohib;
				cache.LanguageProject.MorphologicalDataOA.AdhocCoProhibitionsOC.Add(prohib);
				prohib.FirstMorphemeRA = LookupMsa(rule.PrimaryId);
				foreach (var otherId in rule.OtherIds)
					prohib.RestOfMorphsRS.Add(LookupMsa(otherId));
				prohib.Adjacency = AdjacencyOf(rule.Adjacency);
				prohib.Disabled = !rule.IsActive;
				result.Note($"morphemeCoOccurrence[{i}]", prohib.Guid, "MoMorphAdhocProhib");
			}

			var alloFactory = cache.ServiceLocator.GetInstance<IMoAlloAdhocProhibFactory>();
			// Rules that HCLoader emits once per MSA copy collapse to ONE FieldWorks prohibition: a
			// rule whose allomorph objects equal an earlier rule's is an alias of that rule.
			var prohibBySignature = new Dictionary<string, IMoAlloAdhocProhib>();
			for (var i = 0; i < grammar.AllomorphCoOccurrenceRules.Count; i++)
			{
				var rule = grammar.AllomorphCoOccurrenceRules[i];
				var signature = LookupAllo(rule.PrimaryId).Guid + ">" + string.Join(",", rule.OtherIds.Select(o => LookupAllo(o).Guid.ToString())) + ">" + rule.Adjacency + ">" + rule.IsActive;
				if (prohibBySignature.TryGetValue(signature, out var earlier))
				{
					result.NoteAlias($"allomorphCoOccurrence[{i}]", earlier.Guid);
					continue;
				}
				var prohib = alloFactory.Create();
				prohibBySignature[signature] = prohib;
				cache.LanguageProject.MorphologicalDataOA.AdhocCoProhibitionsOC.Add(prohib);
				prohib.FirstAllomorphRA = LookupAllo(rule.PrimaryId);
				foreach (var otherId in rule.OtherIds)
					prohib.RestOfAllosRS.Add(LookupAllo(otherId));
				prohib.Adjacency = AdjacencyOf(rule.Adjacency);
				prohib.Disabled = !rule.IsActive;
				result.Note($"allomorphCoOccurrence[{i}]", prohib.Guid, "MoAlloAdhocProhib");
			}

			// Prohibitions the grammar cannot carry: they name ONE half of a circumfix (HCLoader keys the
			// circumfix's allomorphs by its LexemeFormOA, so these resolve to no HC rule).
			foreach (var h in halfProhibs)
			{
				var eq = h.Split('=');
				var rs = eq[1].Split(':');
				var stemAllo = alloFormMap[eq[0]];
				var half = alloFormMap["half:" + rs[0] + ":" + rs[1]];
				foreach (var (first, other) in new[] { (half, stemAllo), (stemAllo, half) })
				{
					var prohib = alloFactory.Create();
					cache.LanguageProject.MorphologicalDataOA.AdhocCoProhibitionsOC.Add(prohib);
					prohib.FirstAllomorphRA = first;
					prohib.RestOfAllosRS.Add(other);
					prohib.Adjacency = AdjacencyOf("anywhere");
					result.NoteCountOnly("MoAlloAdhocProhib");
				}
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

		// HCLoader.GetAdjacency (HCLoader.cs:2241-2255) maps these ints to HC's MorphCoOccurrenceAdjacency.
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

		private static void SetParserParameters(LcmCache cache, GrammarModel grammar, int maxPrefixes, int maxSuffixes, int maxAnalyses, bool mergeMsasOff)
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
				(mergeMsasOff ? "<MergeMSAs>false</MergeMSAs>" : "") +
				"<AcceptUnspecifiedGraphemes>false</AcceptUnspecifiedGraphemes></HC></ParserParameters>";
		}
	}
}
