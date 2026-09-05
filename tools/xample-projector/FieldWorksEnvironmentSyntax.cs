using System.Collections.Generic;
using System.Linq;

namespace XampleProjector
{
	/// <summary>
	/// Builds a FieldWorks-style `IPhEnvironment.StringRepresentation` string ("/ [C] _ #") from an
	/// already-validated <see cref="EnvironmentModel"/>. Natural-class terms render as
	/// "[Abbreviation]" (HCLoader resolves an environment's bracketed text against a natural
	/// class's Abbreviation, not its Name -- see the "any stem pattern" research note), literal
	/// segments render as their first declared representation, an optional term is parenthesized,
	/// and a word-boundary anchor glues (no space) to the adjacent term nearest that edge, or
	/// stands alone if that side has no other term -- both shapes are attested in
	/// HCLoaderTests.cs's own environment strings ("/ [V] _ #", "/ #[C] _ [C]").
	/// </summary>
	internal static class FieldWorksEnvironmentSyntax
	{
		internal static string Build(EnvironmentModel env, IReadOnlyDictionary<string, string> naturalClassAbbreviations,
			IReadOnlyDictionary<string, string> segmentRepresentations)
		{
			var left = BuildSide(env.LeftTerms, env.LeftAnchoredAtWordStart, isLeftSide: true, naturalClassAbbreviations, segmentRepresentations);
			var right = BuildSide(env.RightTerms, env.RightAnchoredAtWordEnd, isLeftSide: false, naturalClassAbbreviations, segmentRepresentations);
			var result = "/ ";
			if (left.Length > 0)
				result += left + " ";
			result += "_";
			if (right.Length > 0)
				result += " " + right;
			return result;
		}

		private static string BuildSide(IReadOnlyList<EnvironmentTerm> terms, bool anchored, bool isLeftSide,
			IReadOnlyDictionary<string, string> naturalClassAbbreviations, IReadOnlyDictionary<string, string> segmentRepresentations)
		{
			var tokens = terms.Select(t => FormatTerm(t, naturalClassAbbreviations, segmentRepresentations)).ToList();
			if (!anchored)
				return string.Join(" ", tokens);
			if (tokens.Count == 0)
				return "#";
			if (isLeftSide)
				tokens[0] = "#" + tokens[0];
			else
				tokens[tokens.Count - 1] = tokens[tokens.Count - 1] + "#";
			return string.Join(" ", tokens);
		}

		private static string FormatTerm(EnvironmentTerm term, IReadOnlyDictionary<string, string> naturalClassAbbreviations,
			IReadOnlyDictionary<string, string> segmentRepresentations)
		{
			var text = term.NaturalClassId != null
				? "[" + naturalClassAbbreviations[term.NaturalClassId] + "]"
				: segmentRepresentations[term.LiteralSegmentId];
			return term.Optional ? "(" + text + ")" : text;
		}
	}
}
