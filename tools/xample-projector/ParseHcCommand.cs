using System;
using System.IO;
using System.Linq;
using Newtonsoft.Json.Linq;
using SIL.FieldWorks.WordWorks.Parser;
using SIL.LCModel;
using SIL.Machine.Morphology.HermitCrab;

namespace XampleProjector
{
	/// <summary>Measures HCLoader's language without losing its source-object properties in XML.</summary>
	internal static class ParseHcCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--project", out var project) ||
				!ArgParser.TryGetOption(args, "--hc-xml", out var xml) ||
				!ArgParser.TryGetOption(args, "--words", out var wordsPath) ||
				!ArgParser.TryGetOption(args, "--out", out var output))
				return ExitCodes.Usage;
			return FieldWorksSession.Run(fieldWorksDir, project, (cache, logger) =>
			{
				var language = HCLoader.Load(cache, logger);
				var morpher = new Morpher(new TraceManager(), language);
				var projected = new Morpher(new TraceManager(), XmlLanguageLoader.Load(xml));
				var repo = cache.ServiceLocator.GetInstance<ICmObjectRepository>();
				var results = new JArray();
				foreach (var word in File.ReadAllLines(wordsPath).Where(w => w.Length > 0))
				{
					var row = new JObject { ["word"] = word, ["analyses"] = new JArray(), ["engineError"] = null };
					System.Collections.Generic.List<Word> analyses = null;
					try { analyses = morpher.ParseWord(word).ToList(); }
					catch (Exception ex) { row["engineError"] = ex.GetType().FullName + ": " + ex.Message; }
					row["projectedEngineError"] = null;
					row["projectedAnalysisCount"] = null;
					try { row["projectedAnalysisCount"] = projected.ParseWord(word).Count(); }
					catch (Exception ex) { row["projectedEngineError"] = ex.GetType().FullName + ": " + ex.Message; }
					row["projectionAgrees"] = analyses != null
						? (int?)row["projectedAnalysisCount"] == analyses.Count
						: (string)row["engineError"] == (string)row["projectedEngineError"];
					if (analyses != null)
					{
						foreach (var analysis in analyses)
						{
							var morphs = new JArray();
							foreach (var morph in analysis.Morphs)
							{
								var allo = analysis.GetAllomorph(morph);
								if (allo.Properties["ID2"] != null)
									throw new InvalidOperationException("stored-key capture does not support paired circumfix forms");
								morphs.Add(new JObject
								{
									["allomorphGuid"] = Resolve(repo, allo.Properties["ID"], false),
									["msaGuid"] = Resolve(repo, allo.Morpheme.Properties["ID"], false),
									["inflectionTypeGuid"] = Resolve(repo, allo.Morpheme.Properties["InflTypeID"], true),
								});
							}
							((JArray)row["analyses"]).Add(new JObject { ["morphemes"] = morphs });
						}
						row["segmentCount"] = language.Strata[0].CharacterDefinitionTable.Segment(word).Count;
					}
					results.Add(row);
				}
				JsonWriter.WriteFile(output, new JObject
				{
					["schemaVersion"] = 1, ["mode"] = "parse-hc",
					["engineVersion"] = FieldWorksPins.ExpectedFileVersions["SIL.Machine.Morphology.HermitCrab.dll"],
					["sourceSha256"] = Sha256.OfFile(project), ["hcXmlSha256"] = Sha256.OfFile(xml),
					["diagnostics"] = new JArray(logger.Diagnostics.Select(d => new JObject { ["kind"] = d.Kind, ["message"] = d.Message })),
					["characterDefinitions"] = new JArray(language.Strata[0].CharacterDefinitionTable.Select(c =>
						new JObject { ["representations"] = new JArray(c.Representations), ["features"] = c.FeatureStruct.ToString() })),
					["naturalClassCompatibility"] = new JArray(language.NaturalClasses.Where(n => n.Name != "Any").Select(n =>
						new JObject
						{
							["name"] = n.Name, ["features"] = n.FeatureStruct.ToString(),
							["characters"] = new JArray(language.Strata[0].CharacterDefinitionTable.Select(c => new JObject
							{
								["representations"] = new JArray(c.Representations),
								["unifiable"] = n.FeatureStruct.IsUnifiable(c.FeatureStruct),
								["subsumes"] = n.FeatureStruct.Subsumes(c.FeatureStruct),
								["subsumesWithDefaults"] = n.FeatureStruct.Subsumes(c.FeatureStruct, true),
							})),
						})),
					["words"] = results,
				});
				return ExitCodes.Ok;
			});
		}

		private static string Resolve(ICmObjectRepository repo, object value, bool optional)
		{
			if (value == null && optional)
				return null;
			if (!(value is int id) || !repo.TryGetObject(id, out ICmObject obj))
				throw new InvalidOperationException("cannot resolve HC stored-analysis source identity: " + value);
			return obj.Guid.ToString();
		}
	}
}
