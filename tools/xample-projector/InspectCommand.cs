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

			return FieldWorksSession.Run(projectPath, (cache, logger) =>
			{
				var response = BuildResponse(cache, fieldWorksDir, projectPath);
				JsonWriter.WriteFile(outPath, response);
				Console.WriteLine("Wrote {0}", outPath);
				return ExitCodes.Ok;
			});
		}

		private static JObject BuildResponse(LcmCache cache, string fieldWorksDir, string projectPath)
		{
			var diagnostics = new JArray();
			var phonData = cache.LanguageProject.PhonologicalDataOA;
			var phonemeSet = phonData?.PhonemeSetsOS.FirstOrDefault();
			if (phonemeSet == null)
				diagnostics.Add("no phoneme set found on this project's phonological data");

			var phonemes = new JArray();
			if (phonemeSet != null)
			{
				foreach (var phoneme in phonemeSet.PhonemesOC)
					phonemes.Add(DescribeTerminalUnit(phoneme, diagnostics, "phoneme"));
			}

			var boundaryMarkers = new JArray();
			if (phonemeSet != null)
			{
				foreach (var marker in phonemeSet.BoundaryMarkersOC)
					boundaryMarkers.Add(DescribeTerminalUnit(marker, diagnostics, "boundary marker"));
			}

			var naturalClasses = new JArray();
			foreach (var natClass in phonData?.NaturalClassesOS ?? Enumerable.Empty<IPhNaturalClass>())
				naturalClasses.Add(DescribeNaturalClass(natClass));

			var response = new JObject
			{
				[Fields.SchemaVersion] = SchemaVersion.Current,
				[Fields.FieldWorksVersion] = FieldWorksVersion(fieldWorksDir),
				[Fields.SourcePath] = projectPath,
				[Fields.SourceSha256] = Sha256.OfFile(projectPath),
				[Fields.ProjectName] = cache.ProjectId.Name,
				[Fields.ActiveParser] = cache.LanguageProject.MorphologicalDataOA?.ActiveParser,
				[Fields.Phonemes] = phonemes,
				[Fields.BoundaryMarkers] = boundaryMarkers,
				[Fields.NaturalClasses] = naturalClasses,
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

			var inboundReferences = new JArray();
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
				["inboundReferences"] = inboundReferences,
			};
		}

		private static JObject DescribeNaturalClass(IPhNaturalClass natClass)
		{
			string kind;
			var memberGuids = new JArray();
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

			return new JObject
			{
				["guid"] = natClass.Guid.ToString(),
				["name"] = natClass.Name?.BestAnalysisVernacularAlternative?.Text,
				["kind"] = kind,
				["memberGuids"] = memberGuids,
			};
		}

		internal static string FieldWorksVersion(string fieldWorksDir)
		{
			var path = Path.Combine(fieldWorksDir, "ParserCore.dll");
			return File.Exists(path) ? FileVersionInfo.GetVersionInfo(path).FileVersion : null;
		}
	}
}
