using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using Newtonsoft.Json.Linq;
using SIL.LCModel;

namespace XampleProjector
{
	/// <summary>
	/// Read-only survey of a FieldWorks project's phonology: phonemes, boundary markers,
	/// natural classes, and which parser is active -- everything needed to plan a later
	/// LibLCM-driven mutation without writing anything to the source project.
	///
	/// Every array (phonemes, boundaryMarkers, naturalClasses, memberGuids, and each unit's
	/// inboundReferences) is sorted by guid (ordinal string compare) before being written, so
	/// two runs over the same project produce byte-identical JSON regardless of whatever order
	/// LCM's own collections happen to enumerate in.
	/// </summary>
	internal static class InspectCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--project", out var projectPath) ||
				!ArgParser.TryGetOption(args, "--out", out var outPath))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}

			return FieldWorksSession.Run(fieldWorksDir, projectPath, (cache, logger) =>
			{
				var response = BuildResponse(cache, fieldWorksDir, outPath, projectPath);
				JsonWriter.WriteFile(outPath, response);
				Console.WriteLine("Wrote {0}", outPath);
				return ExitCodes.Ok;
			});
		}

		private static JObject BuildResponse(LcmCache cache, string fieldWorksDir, string outPath, string projectPath)
		{
			var diagnostics = new JArray();
			var phonData = cache.LanguageProject.PhonologicalDataOA;
			var phonemeSet = phonData?.PhonemeSetsOS.FirstOrDefault();
			if (phonemeSet == null)
				diagnostics.Add("no phoneme set found on this project's phonological data");

			var phonemes = new List<JObject>();
			if (phonemeSet != null)
			{
				foreach (var phoneme in phonemeSet.PhonemesOC)
					phonemes.Add(DescribeTerminalUnit(phoneme, diagnostics, "phoneme"));
			}

			var boundaryMarkers = new List<JObject>();
			if (phonemeSet != null)
			{
				foreach (var marker in phonemeSet.BoundaryMarkersOC)
					boundaryMarkers.Add(DescribeTerminalUnit(marker, diagnostics, "boundary marker"));
			}

			var naturalClasses = new List<JObject>();
			foreach (var natClass in phonData?.NaturalClassesOS ?? Enumerable.Empty<IPhNaturalClass>())
				naturalClasses.Add(DescribeNaturalClass(natClass));

			// outPath's directory is this mode's analog of --out-dir: the base generated[].path
			// (project mode) and sourcePath (both modes) are relative to.
			var outFileDir = Path.GetDirectoryName(Path.GetFullPath(outPath));

			var response = new JObject
			{
				[Fields.SchemaVersion] = SchemaVersion.Current,
				[Fields.Mode] = "inspect",
				[Fields.FieldWorksVersion] = FieldWorksVersion(fieldWorksDir),
				[Fields.SourcePath] = PathUtil.MakeRelative(outFileDir, projectPath),
				[Fields.SourceSha256] = Sha256.OfFile(projectPath),
				[Fields.ProjectName] = cache.ProjectId.Name,
				[Fields.ActiveParser] = cache.LanguageProject.MorphologicalDataOA?.ActiveParser,
				[Fields.Phonemes] = SortedByGuid(phonemes),
				[Fields.BoundaryMarkers] = SortedByGuid(boundaryMarkers),
				[Fields.NaturalClasses] = SortedByGuid(naturalClasses),
				[Fields.Diagnostics] = diagnostics,
			};
			return response;
		}

		private static JObject DescribeTerminalUnit(IPhTerminalUnit unit, JArray diagnostics, string kindForDiagnostics)
		{
			var representations = new JArray();
			foreach (var code in unit.CodesOS)
			{
				var text = code.Representation?.VernacularDefaultWritingSystem?.Text;
				if (!string.IsNullOrEmpty(text))
					representations.Add(text);
			}
			if (representations.Count == 0)
				diagnostics.Add($"{kindForDiagnostics} {unit.Guid} has no valid grapheme representations");

			var inboundReferences = new List<JObject>();
			foreach (var referrer in unit.ReferringObjects)
			{
				inboundReferences.Add(new JObject
				{
					["guid"] = referrer.Guid.ToString(),
					["class"] = referrer.ClassName,
				});
			}

			return new JObject
			{
				["guid"] = unit.Guid.ToString(),
				["representations"] = representations,
				["inboundReferences"] = SortedByGuid(inboundReferences),
			};
		}

		private static JObject DescribeNaturalClass(IPhNaturalClass natClass)
		{
			string kind;
			var memberGuids = new List<string>();
			switch (natClass)
			{
				case IPhNCSegments segments:
					kind = "segments";
					foreach (var member in segments.SegmentsRC)
						memberGuids.Add(member.Guid.ToString());
					break;
				case IPhNCFeatures _:
					kind = "features";
					break;
				default:
					kind = natClass.ClassName;
					break;
			}
			memberGuids.Sort(StringComparer.Ordinal);

			return new JObject
			{
				["guid"] = natClass.Guid.ToString(),
				["name"] = natClass.Name?.BestAnalysisVernacularAlternative?.Text,
				["kind"] = kind,
				["memberGuids"] = new JArray(memberGuids),
			};
		}

		private static JArray SortedByGuid(List<JObject> items)
		{
			return new JArray(items.OrderBy(o => (string)o["guid"], StringComparer.Ordinal));
		}

		internal static string FieldWorksVersion(string fieldWorksDir)
		{
			var path = Path.Combine(fieldWorksDir, "ParserCore.dll");
			return File.Exists(path) ? FileVersionInfo.GetVersionInfo(path).FileVersion : null;
		}
	}
}
