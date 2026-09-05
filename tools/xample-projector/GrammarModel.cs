using System.Collections.Generic;

namespace XampleProjector
{
	/// <summary>A prefix/suffix affix subrule: "any stem" input, one literal insertion, optional gates.</summary>
	internal sealed class SubruleModel
	{
		internal string Id { get; set; }
		internal bool IsPrefix { get; set; }
		internal string InsertShape { get; set; }
		internal IReadOnlyList<string> RequiredMprFeatureIds { get; set; } = System.Array.Empty<string>();
		internal IReadOnlyList<EnvironmentModel> RequiredEnvironments { get; set; } = System.Array.Empty<EnvironmentModel>();
	}

	/// <summary>One side (left or right) of a FieldWorks-style environment: an ordered list of terms.</summary>
	internal sealed class EnvironmentModel
	{
		internal IReadOnlyList<EnvironmentTerm> LeftTerms { get; set; } = System.Array.Empty<EnvironmentTerm>();
		internal bool LeftAnchoredAtWordStart { get; set; }
		internal IReadOnlyList<EnvironmentTerm> RightTerms { get; set; } = System.Array.Empty<EnvironmentTerm>();
		internal bool RightAnchoredAtWordEnd { get; set; }
	}

	/// <summary>
	/// One position in an environment context: exactly one of <see cref="NaturalClassId"/> /
	/// <see cref="LiteralSegmentId"/> is set, optionally wrapped as optional.
	/// </summary>
	internal sealed class EnvironmentTerm
	{
		internal string NaturalClassId { get; set; }
		internal string LiteralSegmentId { get; set; }
		internal bool Optional { get; set; }
	}

	internal sealed class MorphologicalRuleModel
	{
		internal string Id { get; set; }
		internal string RequiredPartOfSpeechId { get; set; }
		internal string OutputPartOfSpeechId { get; set; }
		internal string MorphemeId { get; set; }
		internal string Gloss { get; set; }
		internal IReadOnlyList<SubruleModel> Subrules { get; set; } = System.Array.Empty<SubruleModel>();
	}

	internal sealed class SlotModel
	{
		internal string Name { get; set; }
		internal bool Optional { get; set; }
		internal IReadOnlyList<string> RuleIds { get; set; } = System.Array.Empty<string>();
	}

	internal sealed class AffixTemplateModel
	{
		internal string Name { get; set; }
		internal string RequiredPartOfSpeechId { get; set; }
		internal IReadOnlyList<SlotModel> Slots { get; set; } = System.Array.Empty<SlotModel>();
	}

	internal sealed class AllomorphModel
	{
		internal string Id { get; set; }
		internal string Shape { get; set; }
		internal IReadOnlyList<EnvironmentModel> RequiredEnvironments { get; set; } = System.Array.Empty<EnvironmentModel>();
	}

	internal sealed class LexicalEntryModel
	{
		internal string Id { get; set; }
		internal string PartOfSpeechId { get; set; }
		internal string MorphemeId { get; set; }
		internal string Gloss { get; set; }
		internal IReadOnlyList<AllomorphModel> Allomorphs { get; set; } = System.Array.Empty<AllomorphModel>();
		internal IReadOnlyList<string> RuleFeatureIds { get; set; } = System.Array.Empty<string>();
	}

	/// <summary>type="exclude" only -- type="require" is refused before a GrammarModel is ever built.</summary>
	internal sealed class CoOccurrenceRuleModel
	{
		internal string PrimaryId { get; set; }
		internal IReadOnlyList<string> OtherIds { get; set; } = System.Array.Empty<string>();
		internal string Adjacency { get; set; }
		internal bool IsActive { get; set; } = true;
		internal bool IsAllomorphRule { get; set; }
	}

	internal sealed class PartOfSpeechModel
	{
		internal string Id { get; set; }
		internal string Name { get; set; }
	}

	internal sealed class PhonemeModel
	{
		internal string Id { get; set; }
		internal IReadOnlyList<string> Representations { get; set; } = System.Array.Empty<string>();
	}

	internal sealed class BoundaryMarkerModel
	{
		internal string Id { get; set; }
		internal string Representation { get; set; }
	}

	internal sealed class SegmentNaturalClassModel
	{
		internal string Id { get; set; }
		internal string Name { get; set; }
		internal IReadOnlyList<string> SegmentIds { get; set; } = System.Array.Empty<string>();
	}

	internal sealed class MprFeatureModel
	{
		internal string Id { get; set; }
		internal string Text { get; set; }
	}

	/// <summary>
	/// The fully-validated supported subset of one HermitCrabInput grammar.xml, ready to author into
	/// LibLCM. Every field here has already passed <see cref="GrammarParser"/>'s refusal checks --
	/// nothing downstream needs to re-check representability, only construct LCM objects.
	/// </summary>
	internal sealed class GrammarModel
	{
		internal string LanguageName { get; set; }
		internal IReadOnlyList<PartOfSpeechModel> PartsOfSpeech { get; set; } = System.Array.Empty<PartOfSpeechModel>();
		internal IReadOnlyList<PhonemeModel> Phonemes { get; set; } = System.Array.Empty<PhonemeModel>();
		internal IReadOnlyList<BoundaryMarkerModel> BoundaryMarkers { get; set; } = System.Array.Empty<BoundaryMarkerModel>();
		internal IReadOnlyList<SegmentNaturalClassModel> NaturalClasses { get; set; } = System.Array.Empty<SegmentNaturalClassModel>();
		internal IReadOnlyList<MprFeatureModel> MprFeatures { get; set; } = System.Array.Empty<MprFeatureModel>();
		internal IReadOnlyDictionary<string, MorphologicalRuleModel> MorphologicalRules { get; set; }
		internal IReadOnlyList<AffixTemplateModel> AffixTemplates { get; set; } = System.Array.Empty<AffixTemplateModel>();
		internal IReadOnlyList<LexicalEntryModel> LexicalEntries { get; set; } = System.Array.Empty<LexicalEntryModel>();
		internal IReadOnlyList<CoOccurrenceRuleModel> MorphemeCoOccurrenceRules { get; set; } = System.Array.Empty<CoOccurrenceRuleModel>();
		internal IReadOnlyList<CoOccurrenceRuleModel> AllomorphCoOccurrenceRules { get; set; } = System.Array.Empty<CoOccurrenceRuleModel>();

		/// <summary>Present-but-not-authored attributes, recorded for the response's "unmapped" array.</summary>
		internal IReadOnlyList<string> Unmapped { get; set; } = System.Array.Empty<string>();
	}
}
