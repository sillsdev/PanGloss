using System;
using System.Collections.Generic;
using System.IO;
using Newtonsoft.Json.Linq;

namespace XampleProjector
{
	/// <summary>
	/// Portable (FieldWorks-free, like --validate-capture) check that a computed guid-to-content-
	/// derived-label map is injective. build.ps1's witness-drift probe derives a label for every
	/// guid it can (a record's own Name/Gloss/Form text, or -- for a record with none of its own --
	/// its resolved owner's) so that a wiring difference (e.g. which slot an affix rule targets)
	/// survives normalization instead of every guid collapsing to the same blind placeholder; this
	/// command proves that map assigns no two guids the same label, using the SAME guarded-insert
	/// <see cref="IdRegistry.Register"/> GrammarParser uses to prove a grammar.xml's ids/Names are
	/// document-global-unique, rather than a second, independently-written uniqueness check.
	/// </summary>
	internal static class CheckLabelUniquenessCommand
	{
		internal static int Run(string[] args)
		{
			if (!ArgParser.TryGetOption(args, "--labels", out var labelsPath))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}
			if (!File.Exists(labelsPath))
			{
				Console.Error.WriteLine("Label uniqueness check failure: labels file not found: {0}", labelsPath);
				return ExitCodes.Usage;
			}

			JObject labels;
			try
			{
				labels = JObject.Parse(File.ReadAllText(labelsPath));
			}
			catch (Exception ex)
			{
				Console.Error.WriteLine("Label uniqueness check failure: labels file is not valid JSON: {0}", ex.Message);
				return ExitCodes.Usage;
			}

			var seenLabels = new Dictionary<string, string>();
			try
			{
				foreach (var prop in labels.Properties())
				{
					var guid = prop.Name;
					var label = (string)prop.Value;
					IdRegistry.Register(seenLabels, label, guid, existingGuid =>
						$"content-derived label \"{label}\" is used by both guid {existingGuid} and guid {guid} (two records must never render identically after content-derived labeling)");
				}
			}
			catch (GrammarAuthorException ex)
			{
				Console.Error.WriteLine("Label uniqueness refusal: {0}", ex.Message);
				return ExitCodes.AuthorRefusal;
			}

			Console.WriteLine("OK: {0} content-derived label(s), all unique.", seenLabels.Count);
			return ExitCodes.Ok;
		}
	}
}
