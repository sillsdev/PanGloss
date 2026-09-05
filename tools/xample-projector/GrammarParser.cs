using System;
using System.Collections.Generic;
using System.Linq;
using System.Xml.Linq;

namespace XampleProjector
{
	/// <summary>
	/// Parses and validates a HermitCrabInput grammar.xml against the supported subset `author`
	/// can represent in LibLCM. Every refusal is a <see cref="GrammarAuthorException"/> naming the
	/// offending element/attribute and its id, thrown as soon as the first unsupported construct is
	/// found (document order) -- no partial LCM project is ever created for a refused grammar,
	/// because this whole pass touches no FieldWorks type at all.
	/// </summary>
	internal static class GrammarParser
	{
		private sealed class ParseCtx
		{
			internal HashSet<string> PartOfSpeechIds;
			internal HashSet<string> MprFeatureIds;
			internal HashSet<string> FeaturelessFeatureClassIds;
			internal HashSet<string> FeaturedFeatureClassIds;
			internal HashSet<string> SegmentClassIds;
			internal HashSet<string> SegmentIds;
			internal Dictionary<string, string> SeenIds;
		}

		internal static GrammarModel Parse(XDocument doc)
		{
			var languages = doc.Root.Elements("Language").ToList();
			if (languages.Count != 1)
				throw new GrammarAuthorException($"HermitCrabInput: expected exactly 1 Language element, found {languages.Count}");
			var language = languages[0];

			// The DTD types "id" as document-global, but DtdProcessing.Ignore (LoadGrammarDocument)
			// means nothing enforces that -- alloFormMap/GuidMap are plain dictionaries keyed by
			// fixture id and would otherwise silently overwrite on a collision.
			var seenIds = new Dictionary<string, string>();

			RefuseIfPresent(language, "PhonologicalFeatureSystem");
			RefuseIfPresent(language, "HeadFeatures");
			RefuseIfPresent(language, "FootFeatures");
			RefuseIfPresent(language, "StemNames");
			RefuseIfPresent(language, "Families");
			RefuseIfPresent(language, "PhonologicalRuleDefinitions");
			RefuseIfPresent(language, "SyntacticRules");

			var languageName = (string)language.Element("Name");

			var partsOfSpeech = ParsePartsOfSpeech(language, seenIds);
			var mprFeatures = ParseMprFeatures(language, seenIds);

			var strata = language.Element("Strata")?.Elements("Stratum").ToList() ?? new List<XElement>();
			if (strata.Count != 1)
				throw new GrammarAuthorException($"Strata: expected exactly 1 Stratum, found {strata.Count} (unsupported -- only a single-stratum grammar is supported in this slice)");
			var stratum = strata[0];
			RefuseIfInactive(stratum, "Stratum");

			var unmapped = new List<string>();
			var morphRuleOrder = (string)stratum.Attribute("morphologicalRuleOrder");
			if (morphRuleOrder != null)
				unmapped.Add($"Stratum@morphologicalRuleOrder=\"{morphRuleOrder}\" (ignored; FieldWorks has no equivalent)");
			var stratumRuleList = (string)stratum.Attribute("morphologicalRules");
			if (stratumRuleList != null)
				unmapped.Add($"Stratum@morphologicalRules=\"{stratumRuleList}\" (ignored; FieldWorks orders rules via slots/templates, not a stratum-level list)");
			if ((string)stratum.Attribute("phonologicalRules") != null)
				throw new GrammarAuthorException("Stratum@phonologicalRules references PhonologicalRule(s) (unsupported -- PhonologicalRuleDefinitions is not supported)");
			if ((string)stratum.Attribute("cyclicity") == "cyclic")
				throw new GrammarAuthorException("Stratum@cyclicity=\"cyclic\" is unsupported");

			var charTableId = (string)stratum.Attribute("characterDefinitionTable");
			var charTable = language.Elements("CharacterDefinitionTable").FirstOrDefault(t => (string)t.Attribute("id") == charTableId);
			if (charTable == null)
				throw new GrammarAuthorException($"Stratum references characterDefinitionTable=\"{charTableId}\" which was not found");
			RegisterId(seenIds, charTableId, "CharacterDefinitionTable");
			RefuseIfInactive(charTable, $"CharacterDefinitionTable \"{charTableId}\"");

			var phonemes = ParsePhonemes(charTable, seenIds);
			var boundaryMarkers = ParseBoundaryMarkers(charTable, seenIds);

			var featurelessFeatureClassIds = new HashSet<string>();
			var featuredFeatureClassIds = new HashSet<string>();
			var segmentClasses = new List<SegmentNaturalClassModel>();
			var naturalClassesElem = language.Element("NaturalClasses");
			if (naturalClassesElem != null)
			{
				foreach (var el in naturalClassesElem.Elements())
				{
					if (el.Name == "FeatureNaturalClass")
					{
						var id = (string)el.Attribute("id");
						RegisterId(seenIds, id, "FeatureNaturalClass");
						RefuseIfInactive(el, $"FeatureNaturalClass \"{id}\"");
						if (el.Elements("FeatureValue").Any())
							featuredFeatureClassIds.Add(id);
						else
							featurelessFeatureClassIds.Add(id);
					}
					else if (el.Name == "SegmentNaturalClass")
					{
						var id = (string)el.Attribute("id");
						RegisterId(seenIds, id, "SegmentNaturalClass");
						RefuseIfInactive(el, $"SegmentNaturalClass \"{id}\"");
						segmentClasses.Add(new SegmentNaturalClassModel
						{
							Id = id,
							Name = (string)el.Element("Name"),
							SegmentIds = el.Elements("Segment").Select(s => (string)s.Attribute("segment")).ToList(),
						});
					}
					else
					{
						throw new GrammarAuthorException($"NaturalClasses contains unsupported element <{el.Name}>");
					}
				}
			}

			var ctx = new ParseCtx
			{
				PartOfSpeechIds = new HashSet<string>(partsOfSpeech.Select(p => p.Id)),
				MprFeatureIds = new HashSet<string>(mprFeatures.Select(f => f.Id)),
				FeaturelessFeatureClassIds = featurelessFeatureClassIds,
				FeaturedFeatureClassIds = featuredFeatureClassIds,
				SegmentClassIds = new HashSet<string>(segmentClasses.Select(c => c.Id)),
				SegmentIds = new HashSet<string>(phonemes.Select(p => p.Id)),
				SeenIds = seenIds,
			};

			var rules = new Dictionary<string, MorphologicalRuleModel>();
			var ruleDefsElem = stratum.Element("MorphologicalRuleDefinitions");
			if (ruleDefsElem != null)
			{
				foreach (var el in ruleDefsElem.Elements())
				{
					if (el.Name != "MorphologicalRule")
					{
						throw new GrammarAuthorException(
							$"MorphologicalRuleDefinitions contains unsupported element <{el.Name}> (id=\"{(string)el.Attribute("id")}\") -- only MorphologicalRule is supported in this slice");
					}
					var rule = ParseMorphologicalRule(el, ctx);
					rules[rule.Id] = rule;
				}
			}

			var templates = new List<AffixTemplateModel>();
			var referencedRuleIds = new HashSet<string>();
			var templatesElem = stratum.Element("AffixTemplates");
			if (templatesElem != null)
			{
				foreach (var te in templatesElem.Elements("AffixTemplate"))
				{
					var templateName = (string)te.Element("Name");
					RefuseIfInactive(te, $"AffixTemplate \"{templateName}\"");
					if ((string)te.Attribute("requiredSubcategorizedRules") != null)
						throw new GrammarAuthorException($"AffixTemplate \"{templateName}\": requiredSubcategorizedRules is unsupported (SyntacticRules unsupported)");

					var slots = new List<SlotModel>();
					foreach (var se in te.Elements("Slot"))
					{
						var slotName = (string)se.Element("Name");
						RefuseIfInactive(se, $"Slot \"{slotName}\"");
						var ruleIds = SplitIds((string)se.Attribute("morphologicalRules"));
						if (ruleIds.Count == 0)
							throw new GrammarAuthorException($"Slot \"{slotName}\": morphologicalRules is required but empty");
						foreach (var rid in ruleIds)
						{
							if (!rules.ContainsKey(rid))
								throw new GrammarAuthorException($"AffixTemplate \"{templateName}\" slot \"{slotName}\" references unknown MorphologicalRule \"{rid}\"");
							referencedRuleIds.Add(rid);
						}
						// A slot has exactly one of PrefixSlotsRS/SuffixSlotsRS to be added to.
						var directions = ruleIds.Select(rid => rules[rid].Subrules[0].IsPrefix).Distinct().ToList();
						if (directions.Count != 1)
							throw new GrammarAuthorException($"Slot \"{slotName}\" mixes prefix and suffix rules (unsupported)");
						slots.Add(new SlotModel
						{
							Name = slotName,
							Optional = (string)se.Attribute("optional") == "true",
							RuleIds = ruleIds,
						});
					}
					templates.Add(new AffixTemplateModel
					{
						Name = templateName,
						RequiredPartOfSpeechId = (string)te.Attribute("requiredPartsOfSpeech"),
						Slots = slots,
					});
				}
			}

			foreach (var ruleId in rules.Keys)
			{
				if (!referencedRuleIds.Contains(ruleId))
				{
					throw new GrammarAuthorException(
						$"MorphologicalRule \"{ruleId}\" is not referenced by any AffixTemplate slot (unsupported in this slice -- derivational back-out is a later expansion)");
				}
			}

			foreach (var rule in rules.Values)
			{
				if (rule.RequiredPartOfSpeechId != rule.OutputPartOfSpeechId)
				{
					throw new GrammarAuthorException(
						$"MorphologicalRule \"{rule.Id}\": requiredPartsOfSpeech=\"{rule.RequiredPartOfSpeechId}\" != outputPartOfSpeech=\"{rule.OutputPartOfSpeechId}\" (unsupported for a slot rule)");
				}
			}

			var lexicalEntries = ParseLexicalEntries(stratum, ctx);

			// A MorphemeCoOccurrenceRule references a MorphologicalRule or LexicalEntry id; an
			// AllomorphCoOccurrenceRule references a MorphologicalSubrule or Allomorph id -- the
			// same two id spaces GrammarAuthor.CreateCoOccurrenceRules' lookups union over.
			var morphemeIds = new HashSet<string>(rules.Keys);
			foreach (var entry in lexicalEntries)
				morphemeIds.Add(entry.Id);

			var allomorphIds = new HashSet<string>();
			foreach (var rule in rules.Values)
				foreach (var subrule in rule.Subrules)
					allomorphIds.Add(subrule.Id);
			foreach (var entry in lexicalEntries)
				foreach (var allo in entry.Allomorphs)
					allomorphIds.Add(allo.Id);

			var morphemeRules = ParseCoOccurrenceRules(language.Element("MorphemeCoOccurrenceRules"), isAllomorph: false, morphemeIds);
			var allomorphRules = ParseCoOccurrenceRules(language.Element("AllomorphCoOccurrenceRules"), isAllomorph: true, allomorphIds);

			return new GrammarModel
			{
				LanguageName = languageName,
				PartsOfSpeech = partsOfSpeech,
				Phonemes = phonemes,
				BoundaryMarkers = boundaryMarkers,
				NaturalClasses = segmentClasses,
				MprFeatures = mprFeatures,
				MorphologicalRules = rules,
				AffixTemplates = templates,
				LexicalEntries = lexicalEntries,
				MorphemeCoOccurrenceRules = morphemeRules,
				AllomorphCoOccurrenceRules = allomorphRules,
				Unmapped = unmapped,
			};
		}

		private static List<PartOfSpeechModel> ParsePartsOfSpeech(XElement language, Dictionary<string, string> seenIds)
		{
			var container = language.Element("PartsOfSpeech");
			if (container == null)
				throw new GrammarAuthorException("Language has no PartsOfSpeech element");
			return container.Elements("PartOfSpeech")
				.Select(el =>
				{
					var id = (string)el.Attribute("id");
					RegisterId(seenIds, id, "PartOfSpeech");
					return new PartOfSpeechModel { Id = id, Name = (string)el.Element("Name") };
				})
				.ToList();
		}

		private static List<MprFeatureModel> ParseMprFeatures(XElement language, Dictionary<string, string> seenIds)
		{
			var result = new List<MprFeatureModel>();
			var container = language.Element("MorphologicalPhonologicalRuleFeatures");
			if (container == null)
				return result;
			RefuseIfPresent(container, "MorphologicalPhonologicalRuleFeatureGroup", "MorphologicalPhonologicalRuleFeatures");
			foreach (var el in container.Elements("MorphologicalPhonologicalRuleFeature"))
			{
				var id = (string)el.Attribute("id");
				RegisterId(seenIds, id, "MorphologicalPhonologicalRuleFeature");
				RefuseIfInactive(el, $"MorphologicalPhonologicalRuleFeature \"{id}\"");
				result.Add(new MprFeatureModel { Id = id, Text = (string)el });
			}
			return result;
		}

		private static List<PhonemeModel> ParsePhonemes(XElement charTable, Dictionary<string, string> seenIds)
		{
			var result = new List<PhonemeModel>();
			var segDefs = charTable.Element("SegmentDefinitions");
			if (segDefs == null)
				return result;
			foreach (var el in segDefs.Elements("SegmentDefinition"))
			{
				var id = (string)el.Attribute("id");
				RegisterId(seenIds, id, "SegmentDefinition");
				RefuseIfInactive(el, $"SegmentDefinition \"{id}\"");
				RefuseIfPresent(el, "FeatureValue", $"SegmentDefinition \"{id}\"");
				var reps = el.Element("Representations").Elements("Representation").Select(r => (string)r).ToList();
				if (reps.Count == 0)
					throw new GrammarAuthorException($"SegmentDefinition \"{id}\": no Representation elements");
				result.Add(new PhonemeModel { Id = id, Representations = reps });
			}
			return result;
		}

		private static List<BoundaryMarkerModel> ParseBoundaryMarkers(XElement charTable, Dictionary<string, string> seenIds)
		{
			var result = new List<BoundaryMarkerModel>();
			var container = charTable.Element("BoundaryDefinitions");
			if (container == null)
				return result;
			foreach (var el in container.Elements("BoundaryDefinition"))
			{
				var id = (string)el.Attribute("id");
				RegisterId(seenIds, id, "BoundaryDefinition");
				RefuseIfInactive(el, $"BoundaryDefinition \"{id}\"");
				var reps = el.Element("Representations").Elements("Representation").Select(r => (string)r).ToList();
				if (reps.Count != 1 || (reps[0] != "+" && reps[0] != "#"))
				{
					throw new GrammarAuthorException(
						$"BoundaryDefinition \"{id}\": representation \"{string.Join(",", reps)}\" is not \"+\" or \"#\" (unsupported -- only the well-known morph/word boundary markers are supported)");
				}
				result.Add(new BoundaryMarkerModel { Id = id, Representation = reps[0] });
			}
			return result;
		}

		private static MorphologicalRuleModel ParseMorphologicalRule(XElement el, ParseCtx ctx)
		{
			var id = (string)el.Attribute("id");
			RegisterId(ctx.SeenIds, id, "MorphologicalRule");
			RefuseIfInactive(el, $"MorphologicalRule \"{id}\"");
			if ((string)el.Attribute("partial") == "true")
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": partial=\"true\" is unsupported");
			if ((string)el.Attribute("requiredStemName") != null)
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": requiredStemName is unsupported (StemNames unsupported)");
			if ((string)el.Attribute("requiredSubcategorizedRules") != null || (string)el.Attribute("outputSubcategorization") != null)
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": subcategorization attributes are unsupported (SyntacticRules unsupported)");
			if ((string)el.Attribute("outputObligatoryFeatures") != null)
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": outputObligatoryFeatures is unsupported (feature systems unsupported)");

			RefuseIfPresent(el, "OutputSubcategorizationOverrides", $"MorphologicalRule \"{id}\"");
			RefuseIfPresent(el, "OutputHeadFeatures", $"MorphologicalRule \"{id}\"");
			RefuseIfPresent(el, "OutputFootFeatures", $"MorphologicalRule \"{id}\"");
			RefuseIfPresent(el, "RequiredHeadFeatures", $"MorphologicalRule \"{id}\"");
			RefuseIfPresent(el, "RequiredFootFeatures", $"MorphologicalRule \"{id}\"");
			RefuseIfPresent(el, "Properties", $"MorphologicalRule \"{id}\"");

			var requiredPos = (string)el.Attribute("requiredPartsOfSpeech");
			var outputPos = (string)el.Attribute("outputPartOfSpeech");
			if (requiredPos != null && !ctx.PartOfSpeechIds.Contains(requiredPos))
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": requiredPartsOfSpeech=\"{requiredPos}\" is not a declared PartOfSpeech");
			if (outputPos != null && !ctx.PartOfSpeechIds.Contains(outputPos))
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": outputPartOfSpeech=\"{outputPos}\" is not a declared PartOfSpeech");

			var subrulesEl = el.Element("MorphologicalSubrules");
			var subrules = subrulesEl.Elements("MorphologicalSubrule").Select(s => ParseSubrule(s, id, ctx)).ToList();
			if (subrules.Count == 0)
				throw new GrammarAuthorException($"MorphologicalRule \"{id}\": no MorphologicalSubrule elements");

			var mprSets = subrules
				.Select(s => string.Join(" ", s.RequiredMprFeatureIds.OrderBy(x => x, StringComparer.Ordinal)))
				.Distinct()
				.ToList();
			if (mprSets.Count > 1)
			{
				throw new GrammarAuthorException(
					$"MorphologicalRule \"{id}\": requiredMPRFeatures differ across subrules (unsupported -- FieldWorks' FromProdRestrictRC applies to the whole affix, not per subrule)");
			}

			return new MorphologicalRuleModel
			{
				Id = id,
				RequiredPartOfSpeechId = requiredPos,
				OutputPartOfSpeechId = outputPos,
				MorphemeId = (string)el.Element("MorphemeId"),
				Gloss = (string)el.Element("Gloss"),
				Subrules = subrules,
			};
		}

		private static SubruleModel ParseSubrule(XElement subEl, string ruleId, ParseCtx ctx)
		{
			var id = (string)subEl.Attribute("id");
			var label = $"MorphologicalRule \"{ruleId}\" subrule \"{id}\"";
			RegisterId(ctx.SeenIds, id, "MorphologicalSubrule");
			RefuseIfInactive(subEl, label);
			RefuseIfPresent(subEl, "VariableFeatures", label);
			RefuseIfPresent(subEl, "RequiredHeadFeatures", label);
			RefuseIfPresent(subEl, "RequiredFootFeatures", label);
			RefuseIfPresent(subEl, "ExcludedEnvironments", label);
			RefuseIfPresent(subEl, "Properties", label);

			var inputEl = subEl.Element("MorphologicalInput");
			if ((string)inputEl.Attribute("excludedMPRFeatures") != null)
				throw new GrammarAuthorException($"{label}: excludedMPRFeatures is unsupported");
			var requiredMpr = SplitIds((string)inputEl.Attribute("requiredMPRFeatures"));
			foreach (var f in requiredMpr)
			{
				if (!ctx.MprFeatureIds.Contains(f))
					throw new GrammarAuthorException($"{label}: requiredMPRFeatures references unknown feature \"{f}\"");
			}

			var sequences = inputEl.Elements("PhoneticSequence").ToList();
			if (sequences.Count != 1)
			{
				throw new GrammarAuthorException(
					$"{label}: MorphologicalInput must have exactly 1 PhoneticSequence, found {sequences.Count} (unsupported)");
			}
			ValidateAnyStemPattern(sequences[0], label, ctx);

			var outputEl = subEl.Element("MorphologicalOutput");
			if ((string)outputEl.Attribute("MPRFeatures") != null)
			{
				throw new GrammarAuthorException(
					$"{label}: MorphologicalOutput declares MPRFeatures (unsupported -- an inflectional/slot affix cannot set MPR features in FieldWorks)");
			}
			var redup = (string)outputEl.Attribute("redupMorphType");
			if (redup != null && redup != "implicit")
				throw new GrammarAuthorException($"{label}: redupMorphType=\"{redup}\" is unsupported");

			var outChildren = outputEl.Elements().ToList();
			if (outChildren.Count != 2)
			{
				throw new GrammarAuthorException(
					$"{label}: MorphologicalOutput must have exactly 2 children, found {outChildren.Count} (only InsertSegments+CopyFromInput or CopyFromInput+InsertSegments are supported)");
			}
			bool isPrefix;
			XElement insertEl;
			if (outChildren[0].Name == "InsertSegments" && outChildren[1].Name == "CopyFromInput")
			{
				isPrefix = true;
				insertEl = outChildren[0];
			}
			else if (outChildren[0].Name == "CopyFromInput" && outChildren[1].Name == "InsertSegments")
			{
				isPrefix = false;
				insertEl = outChildren[1];
			}
			else
			{
				throw new GrammarAuthorException(
					$"{label}: unsupported MorphologicalOutput shape <{outChildren[0].Name}>+<{outChildren[1].Name}> (only InsertSegments+CopyFromInput or CopyFromInput+InsertSegments are supported)");
			}
			var insertShape = (string)insertEl.Element("PhoneticShape");

			var environments = new List<EnvironmentModel>();
			var reqEnvEl = subEl.Element("RequiredEnvironments");
			if (reqEnvEl != null)
			{
				foreach (var envEl in reqEnvEl.Elements("Environment"))
					environments.Add(ParseEnvironment(envEl, ctx, label));
			}

			return new SubruleModel
			{
				Id = id,
				IsPrefix = isPrefix,
				InsertShape = insertShape,
				RequiredMprFeatureIds = requiredMpr,
				RequiredEnvironments = environments,
			};
		}

		private static void ValidateAnyStemPattern(XElement sequence, string label, ParseCtx ctx)
		{
			var children = sequence.Elements().ToList();
			if (children.Count != 1 || children[0].Name != "OptionalSegmentSequence")
			{
				throw new GrammarAuthorException(
					$"{label}: unsupported MorphologicalInput stem pattern (only a single OptionalSegmentSequence[min=1,max=-1] over a featureless natural class is supported)");
			}
			var oss = children[0];
			if ((string)oss.Attribute("min") != "1" || (string)oss.Attribute("max") != "-1")
				throw new GrammarAuthorException($"{label}: OptionalSegmentSequence must be min=\"1\" max=\"-1\" (unsupported bound)");
			var inner = oss.Elements().ToList();
			if (inner.Count != 1 || inner[0].Name != "SimpleContext")
				throw new GrammarAuthorException($"{label}: OptionalSegmentSequence must wrap exactly one SimpleContext (unsupported)");
			var ncId = (string)inner[0].Attribute("naturalClass");
			if (ctx.FeaturedFeatureClassIds.Contains(ncId))
			{
				throw new GrammarAuthorException(
					$"{label}: stem pattern references FeatureNaturalClass \"{ncId}\" which declares features (unsupported -- only a featureless FeatureNaturalClass used as the stem pattern is supported)");
			}
			if (!ctx.FeaturelessFeatureClassIds.Contains(ncId))
				throw new GrammarAuthorException($"{label}: stem pattern references \"{ncId}\" which is not a featureless FeatureNaturalClass (unsupported)");
		}

		private static EnvironmentModel ParseEnvironment(XElement envElem, ParseCtx ctx, string context)
		{
			var model = new EnvironmentModel();
			var left = envElem.Element("LeftEnvironment");
			if (left != null)
			{
				var (terms, anchored) = ParseEnvironmentSide(left, ctx, context, isLeftSide: true);
				model.LeftTerms = terms;
				model.LeftAnchoredAtWordStart = anchored;
			}
			var right = envElem.Element("RightEnvironment");
			if (right != null)
			{
				var (terms, anchored) = ParseEnvironmentSide(right, ctx, context, isLeftSide: false);
				model.RightTerms = terms;
				model.RightAnchoredAtWordEnd = anchored;
			}
			return model;
		}

		private static (List<EnvironmentTerm> terms, bool anchored) ParseEnvironmentSide(XElement sideElem, ParseCtx ctx, string context, bool isLeftSide)
		{
			var template = sideElem.Element("PhoneticTemplate");
			var seq = template.Element("PhoneticSequence");
			var terms = new List<EnvironmentTerm>();
			foreach (var child in seq.Elements())
			{
				if (child.Name == "SimpleContext")
				{
					terms.Add(new EnvironmentTerm { NaturalClassId = RequireSegmentClass(child, ctx, context) });
				}
				else if (child.Name == "Segment")
				{
					terms.Add(new EnvironmentTerm { LiteralSegmentId = RequireSegment(child, ctx, context) });
				}
				else if (child.Name == "OptionalSegmentSequence")
				{
					var inner = child.Elements().ToList();
					if (inner.Count != 1)
						throw new GrammarAuthorException($"{context}: environment OptionalSegmentSequence must wrap exactly one term (unsupported)");
					if (inner[0].Name == "SimpleContext")
						terms.Add(new EnvironmentTerm { NaturalClassId = RequireSegmentClass(inner[0], ctx, context), Optional = true });
					else if (inner[0].Name == "Segment")
						terms.Add(new EnvironmentTerm { LiteralSegmentId = RequireSegment(inner[0], ctx, context), Optional = true });
					else
						throw new GrammarAuthorException($"{context}: environment OptionalSegmentSequence wraps unsupported element <{inner[0].Name}>");
				}
				else
				{
					throw new GrammarAuthorException(
						$"{context}: environment contains unsupported element <{child.Name}> (only natural-class contexts, literal segments, and optionality are supported)");
				}
			}
			var anchored = isLeftSide
				? (string)template.Attribute("initialBoundaryCondition") == "true"
				: (string)template.Attribute("finalBoundaryCondition") == "true";
			return (terms, anchored);
		}

		private static string RequireSegmentClass(XElement simpleContextEl, ParseCtx ctx, string context)
		{
			var ncId = (string)simpleContextEl.Attribute("naturalClass");
			if (!ctx.SegmentClassIds.Contains(ncId))
			{
				throw new GrammarAuthorException(
					$"{context}: environment references natural class \"{ncId}\" which is not a supported SegmentNaturalClass (a FeatureNaturalClass is supported only as the any-stem pattern)");
			}
			return ncId;
		}

		private static string RequireSegment(XElement segmentEl, ParseCtx ctx, string context)
		{
			var segId = (string)segmentEl.Attribute("segment");
			if (!ctx.SegmentIds.Contains(segId))
				throw new GrammarAuthorException($"{context}: environment references segment \"{segId}\" which was not declared");
			return segId;
		}

		private static List<LexicalEntryModel> ParseLexicalEntries(XElement stratum, ParseCtx ctx)
		{
			var result = new List<LexicalEntryModel>();
			var container = stratum.Element("LexicalEntries");
			if (container == null)
				return result;
			foreach (var el in container.Elements("LexicalEntry"))
			{
				var id = (string)el.Attribute("id");
				RegisterId(ctx.SeenIds, id, "LexicalEntry");
				RefuseIfInactive(el, $"LexicalEntry \"{id}\"");
				if ((string)el.Attribute("partial") == "true")
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": partial=\"true\" is unsupported");
				if ((string)el.Attribute("family") != null)
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": family attribute is unsupported (Families unsupported)");
				if ((string)el.Attribute("subcategorizations") != null)
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": subcategorizations attribute is unsupported (SyntacticRules unsupported)");
				if ((string)el.Attribute("obligatoryHeadFeatures") != null || (string)el.Attribute("obligatoryFootFeatures") != null)
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": obligatoryHeadFeatures/obligatoryFootFeatures are unsupported (feature systems unsupported)");
				if ((string)el.Attribute("morphologicalRules") != null)
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": morphologicalRules attribute is unsupported in this slice");

				var posId = (string)el.Attribute("partOfSpeech");
				if (posId != null && !ctx.PartOfSpeechIds.Contains(posId))
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": partOfSpeech=\"{posId}\" is not a declared PartOfSpeech");

				var ruleFeatureIds = SplitIds((string)el.Attribute("ruleFeatures"));
				foreach (var f in ruleFeatureIds)
				{
					if (!ctx.MprFeatureIds.Contains(f))
						throw new GrammarAuthorException($"LexicalEntry \"{id}\": ruleFeatures references unknown feature \"{f}\"");
				}

				RefuseIfPresent(el, "AssignedHeadFeatures", $"LexicalEntry \"{id}\"");
				RefuseIfPresent(el, "AssignedFootFeatures", $"LexicalEntry \"{id}\"");
				RefuseIfPresent(el, "Properties", $"LexicalEntry \"{id}\"");

				var allomorphsEl = el.Element("Allomorphs");
				var allomorphs = new List<AllomorphModel>();
				foreach (var alloEl in allomorphsEl.Elements("Allomorph"))
				{
					var alloId = (string)alloEl.Attribute("id");
					RegisterId(ctx.SeenIds, alloId, "Allomorph");
					RefuseIfInactive(alloEl, $"Allomorph \"{alloId}\"");
					if ((string)alloEl.Attribute("stemName") != null)
						throw new GrammarAuthorException($"Allomorph \"{alloId}\": stemName is unsupported (StemNames unsupported)");
					RefuseIfPresent(alloEl, "ExcludedEnvironments", $"Allomorph \"{alloId}\"");
					RefuseIfPresent(alloEl, "Properties", $"Allomorph \"{alloId}\"");

					var shape = (string)alloEl.Element("PhoneticShape");
					var environments = new List<EnvironmentModel>();
					var reqEnvEl = alloEl.Element("RequiredEnvironments");
					if (reqEnvEl != null)
					{
						foreach (var envEl in reqEnvEl.Elements("Environment"))
							environments.Add(ParseEnvironment(envEl, ctx, $"Allomorph \"{alloId}\""));
					}
					allomorphs.Add(new AllomorphModel { Id = alloId, Shape = shape, RequiredEnvironments = environments });
				}
				if (allomorphs.Count == 0)
					throw new GrammarAuthorException($"LexicalEntry \"{id}\": no Allomorph elements (unsupported)");

				result.Add(new LexicalEntryModel
				{
					Id = id,
					PartOfSpeechId = posId,
					MorphemeId = (string)el.Element("MorphemeId"),
					Gloss = (string)el.Element("Gloss"),
					Allomorphs = allomorphs,
					RuleFeatureIds = ruleFeatureIds,
				});
			}
			return result;
		}

		// The only five values HCLoader.GetAdjacency (HCLoader.cs:2241-2255) recognizes.
		private static readonly HashSet<string> ValidAdjacencyValues = new HashSet<string>
		{
			"anywhere", "somewhereToLeft", "somewhereToRight", "adjacentToLeft", "adjacentToRight",
		};

		private static List<CoOccurrenceRuleModel> ParseCoOccurrenceRules(XElement containerEl, bool isAllomorph, HashSet<string> validIds)
		{
			var result = new List<CoOccurrenceRuleModel>();
			if (containerEl == null)
				return result;
			var childName = isAllomorph ? "AllomorphCoOccurrenceRule" : "MorphemeCoOccurrenceRule";
			var primaryAttr = isAllomorph ? "primaryAllomorph" : "primaryMorpheme";
			var othersAttr = isAllomorph ? "otherAllomorphs" : "otherMorphemes";
			var idKind = isAllomorph ? "Allomorph or MorphologicalSubrule" : "MorphologicalRule or LexicalEntry";
			foreach (var el in containerEl.Elements())
			{
				if (el.Name != childName)
					throw new GrammarAuthorException($"{containerEl.Name} contains unsupported element <{el.Name}>");
				var primary = (string)el.Attribute(primaryAttr);
				var type = (string)el.Attribute("type") ?? "exclude";
				if (type == "require")
				{
					throw new GrammarAuthorException(
						$"{childName} {primaryAttr}=\"{primary}\" has type=\"require\" (unsupported -- only type=\"exclude\" is supported in this slice)");
				}
				if (type != "exclude")
					throw new GrammarAuthorException($"{childName} {primaryAttr}=\"{primary}\": unsupported type=\"{type}\"");

				if (!validIds.Contains(primary))
				{
					throw new GrammarAuthorException(
						$"{childName} {primaryAttr}=\"{primary}\" is not a known {idKind}");
				}

				var isActive = (string)el.Attribute("isActive") != "no";
				var adjacency = (string)el.Attribute("adjacency") ?? "anywhere";
				if (!ValidAdjacencyValues.Contains(adjacency))
				{
					throw new GrammarAuthorException(
						$"{childName} {primaryAttr}=\"{primary}\": unsupported adjacency=\"{adjacency}\"");
				}
				var others = SplitIds((string)el.Attribute(othersAttr));
				foreach (var otherId in others)
				{
					if (!validIds.Contains(otherId))
					{
						throw new GrammarAuthorException(
							$"{childName} {primaryAttr}=\"{primary}\": {othersAttr} references \"{otherId}\" which is not a known {idKind}");
					}
				}
				result.Add(new CoOccurrenceRuleModel
				{
					PrimaryId = primary,
					OtherIds = others,
					Adjacency = adjacency,
					IsActive = isActive,
					IsAllomorphRule = isAllomorph,
				});
			}
			return result;
		}

		private static void RefuseIfPresent(XElement parent, string childName, string context = null)
		{
			if (parent.Element(childName) != null)
				throw new GrammarAuthorException($"{(context != null ? context + ": " : "")}<{childName}> is present (unsupported in this slice)");
		}

		private static void RefuseIfInactive(XElement el, string label)
		{
			if ((string)el.Attribute("isActive") == "no")
				throw new GrammarAuthorException($"{label}: isActive=\"no\" is unsupported outside MorphemeCoOccurrenceRule/AllomorphCoOccurrenceRule");
		}

		// Refuses the first id reused across element kinds -- the DTD types "id" as document-global.
		private static void RegisterId(Dictionary<string, string> seenIds, string id, string elementKind)
		{
			if (id == null)
				return;
			if (seenIds.TryGetValue(id, out var firstKind))
			{
				throw new GrammarAuthorException(
					$"id \"{id}\" is used by both a {firstKind} and a {elementKind} (unsupported -- the DTD declares id as document-global)");
			}
			seenIds[id] = elementKind;
		}

		private static List<string> SplitIds(string value)
		{
			return string.IsNullOrEmpty(value)
				? new List<string>()
				: value.Split((char[])null, StringSplitOptions.RemoveEmptyEntries).ToList();
		}
	}
}
