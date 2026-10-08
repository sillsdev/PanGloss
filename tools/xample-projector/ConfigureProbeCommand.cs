using System;
using System.IO;
using System.Linq;
using System.Xml.Linq;
using Newtonsoft.Json.Linq;
using SIL.LCModel;
using SIL.LCModel.Core.Text;
using SIL.LCModel.Core.WritingSystems;
using SIL.LCModel.Infrastructure;

namespace XampleProjector
{
	/// <summary>Configures a fresh synthetic project clone through LibLCM and verifies it after reopening.</summary>
	internal static class ConfigureProbeCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			try { return RunCore(args, fieldWorksDir); }
			catch (Exception ex)
			{
				Console.Error.WriteLine("Probe configuration failed: " + ex);
				return ExitCodes.MutationRefusal;
			}
		}

		private static int RunCore(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--project", out var source) ||
				!ArgParser.TryGetOption(args, "--request", out var requestPath) ||
				!ArgParser.TryGetOption(args, "--out-dir", out var output))
				return ExitCodes.Usage;
			var request = JObject.Parse(File.ReadAllText(requestPath));
			if ((int?)request["schemaVersion"] != 1 || !(request["operations"] is JArray operations) || operations.Count == 0)
			{
				Console.Error.WriteLine("Probe request requires schemaVersion 1 and nonempty operations.");
				return ExitCodes.MutationRefusal;
			}
			source = Path.GetFullPath(source);
			output = Path.GetFullPath(output);
			var sourceHash = Sha256.OfFile(source);
			if (sourceHash != (string)request["baseSha256"] || Directory.Exists(output) ||
				output.StartsWith(Path.GetDirectoryName(source) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase))
			{
				Console.Error.WriteLine("Probe integrity refusal: source hash mismatch, existing output, or output inside source project.");
				return ExitCodes.MutationIntegrityFailure;
			}
			foreach (JObject op in operations)
				if (!new[] { "accept-unspecified", "wordforming", "environment", "feature-class", "segment-class", "clear-features", "rewrite" }.Contains((string)op["op"]))
				{
					Console.Error.WriteLine("Unknown probe operation: " + (string)op["op"]);
					return ExitCodes.MutationRefusal;
				}
			var cloneDir = Path.Combine(output, Path.GetFileName(Path.GetDirectoryName(source)));
			CopyDirectory(Path.GetDirectoryName(source), cloneDir);
			var clone = Path.Combine(cloneDir, Path.GetFileName(source));
			var exit = FieldWorksSession.Run(fieldWorksDir, clone, (cache, logger) =>
			{
				NonUndoableUnitOfWorkHelper.Do(cache.ActionHandlerAccessor, () =>
				{
					foreach (JObject op in operations)
						Apply(cache, op);
				});
				cache.ServiceLocator.GetInstance<IUndoStackManager>().Save();
				return ExitCodes.Ok;
			});
			if (exit != 0) return exit;
			exit = FieldWorksSession.Run(fieldWorksDir, clone, (cache, logger) =>
			{
				foreach (JObject op in operations)
					Verify(cache, op);
				JsonWriter.WriteFile(Path.Combine(output, "probe-response.json"), new JObject
				{
					["schemaVersion"] = 1, ["mode"] = "configure-probe", ["baseSha256"] = sourceHash,
					["materializedSha256"] = Sha256.OfFile(clone), ["materializedProjectPath"] = PathUtil.MakeRelative(output, clone),
					["reopened"] = true, ["operations"] = operations,
					["parserParameters"] = cache.LanguageProject.MorphologicalDataOA.ParserParameters,
					["environments"] = new JArray(cache.LanguageProject.PhonologicalDataOA.EnvironmentsOS.Select(e => e.StringRepresentation.Text)),
					["naturalClasses"] = new JArray(cache.LanguageProject.PhonologicalDataOA.NaturalClassesOS.Select(n =>
						new JObject { ["kind"] = n.ClassName, ["name"] = n.Abbreviation.BestAnalysisAlternative.Text })),
					["wordforming"] = new JArray(ValidCharacters.Load(cache.ServiceLocator.WritingSystems.DefaultVernacularWritingSystem).WordFormingCharacters),
				});
				return ExitCodes.Ok;
			});
			if (sourceHash != Sha256.OfFile(source))
			{
				Console.Error.WriteLine("Probe integrity failure: source project changed.");
				return ExitCodes.MutationIntegrityFailure;
			}
			return exit;
		}

		private static void Verify(LcmCache cache, JObject op)
		{
			var pd = cache.LanguageProject.PhonologicalDataOA;
			var ok = false;
				switch ((string)op["op"])
				{
				case "clear-features":
					ok = pd.PhonemeSetsOS[0].PhonemesOC.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["representation"]).FeaturesOA == null;
					break;
				case "accept-unspecified":
					ok = bool.Parse(XElement.Parse(cache.LanguageProject.MorphologicalDataOA.ParserParameters).Element("HC").Element("AcceptUnspecifiedGraphemes").Value) == (bool)op["value"];
					break;
				case "wordforming":
					var chars = ValidCharacters.Load(cache.ServiceLocator.WritingSystems.DefaultVernacularWritingSystem).WordFormingCharacters.ToList();
					ok = ((JArray)op["representations"]).All(r => chars.Contains((string)r));
					break;
				case "environment":
					ok = cache.ServiceLocator.GetInstance<IMoAffixAllomorphRepository>().AllInstances().Single().PhoneEnvRC.Single().StringRepresentation.Text == (string)op["text"];
					break;
				case "feature-class":
					ok = pd.NaturalClassesOS.OfType<IPhNCFeatures>().Single(n => n.Abbreviation.BestAnalysisAlternative.Text == "V").FeaturesOA.FeatureSpecsOC.Count == 1;
					foreach (JProperty assignment in ((JObject)op["assignments"]).Properties())
					{
						var p = pd.PhonemeSetsOS[0].PhonemesOC.Single(pn => pn.CodesOS.Any(c => c.Representation.VernacularDefaultWritingSystem.Text == assignment.Name));
						ok &= p.FeaturesOA.FeatureSpecsOC.OfType<IFsClosedValue>().Single(v => v.FeatureRA.Abbreviation.BestAnalysisAlternative.Text == "voc").ValueRA.Abbreviation.BestAnalysisAlternative.Text == (string)assignment.Value;
						if ((bool?)op["distinct"] == true)
							ok &= p.FeaturesOA.FeatureSpecsOC.OfType<IFsClosedValue>().Single(v => v.FeatureRA.Abbreviation.BestAnalysisAlternative.Text == "identity").ValueRA.Abbreviation.BestAnalysisAlternative.Text == assignment.Name;
					}
					break;
				case "segment-class":
					var members = pd.NaturalClassesOS.OfType<IPhNCSegments>().Single(n => n.Abbreviation.BestAnalysisAlternative.Text == "V").SegmentsRC
						.Select(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text).OrderBy(r => r);
					ok = members.SequenceEqual(((JArray)op["members"]).Select(r => (string)r).OrderBy(r => r));
					break;
				case "rewrite":
					var rule = pd.PhonRulesOS.OfType<IPhRegularRule>().Single(r => r.Name.BestAnalysisAlternative.Text == "probe-rewrite");
					ok = op["leftClass"] != null
						? ((IPhSimpleContextNC)rule.RightHandSidesOS[0].LeftContextOA).FeatureStructureRA.Abbreviation.BestAnalysisAlternative.Text == (string)op["leftClass"]
						: ((IPhSimpleContextSeg)rule.RightHandSidesOS[0].LeftContextOA).FeatureStructureRA.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["left"];
					ok &= ((IPhSimpleContextSeg)rule.StrucDescOS.Single()).FeatureStructureRA.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["input"];
					ok &= ((IPhSimpleContextSeg)rule.RightHandSidesOS[0].StrucChangeOS.Single()).FeatureStructureRA.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["output"];
					break;
			}
			if (!ok) throw new InvalidOperationException("reopened probe did not verify operation " + (string)op["op"]);
		}

		private static void Apply(LcmCache cache, JObject op)
		{
			var pd = cache.LanguageProject.PhonologicalDataOA;
			var phonemes = pd.PhonemeSetsOS[0].PhonemesOC;
			switch ((string)op["op"])
			{
				case "clear-features":
					phonemes.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["representation"]).FeaturesOA = null;
					break;
				case "accept-unspecified":
					var parameters = XElement.Parse(cache.LanguageProject.MorphologicalDataOA.ParserParameters);
					parameters.Element("HC").Element("AcceptUnspecifiedGraphemes").Value = ((bool)op["value"]).ToString().ToLowerInvariant();
					cache.LanguageProject.MorphologicalDataOA.ParserParameters = parameters.ToString();
					break;
				case "wordforming":
					var ws = cache.ServiceLocator.WritingSystems.DefaultVernacularWritingSystem;
					var valid = ValidCharacters.Load(ws);
					foreach (var rep in (JArray)op["representations"])
						valid.AddCharacter((string)rep, ValidCharacterType.WordForming);
					valid.SaveTo(ws);
					break;
				case "environment":
					var env = cache.ServiceLocator.GetInstance<IPhEnvironmentFactory>().Create();
					pd.EnvironmentsOS.Add(env);
					env.StringRepresentation = TsStringUtils.MakeString((string)op["text"], cache.DefaultVernWs);
					var affix = cache.ServiceLocator.GetInstance<IMoAffixAllomorphRepository>().AllInstances().Single();
					affix.PhoneEnvRC.Clear();
					affix.PhoneEnvRC.Add(env);
					break;
				case "feature-class":
					var feature = cache.ServiceLocator.GetInstance<IFsClosedFeatureFactory>().Create();
					cache.LanguageProject.PhFeatureSystemOA.FeaturesOC.Add(feature);
					feature.Name.SetAnalysisDefaultWritingSystem("vocalic");
					feature.Abbreviation.SetAnalysisDefaultWritingSystem("voc");
					foreach (var value in new[] { "+", "-" })
					{
						var symbol = cache.ServiceLocator.GetInstance<IFsSymFeatValFactory>().Create();
						feature.ValuesOC.Add(symbol);
						symbol.Name.SetAnalysisDefaultWritingSystem(value);
						symbol.Abbreviation.SetAnalysisDefaultWritingSystem(value);
					}
					var nc = cache.ServiceLocator.GetInstance<IPhNCFeaturesFactory>().Create();
					pd.NaturalClassesOS.Add(nc);
					nc.Name.SetAnalysisDefaultWritingSystem("V");
					nc.Abbreviation.SetAnalysisDefaultWritingSystem("V");
					SetFeatureStruct(cache, feature, "+", fs => nc.FeaturesOA = fs);
					foreach (JProperty assignment in ((JObject)op["assignments"]).Properties())
					{
						var p = phonemes.Single(pn => pn.CodesOS.Any(c => c.Representation.VernacularDefaultWritingSystem.Text == assignment.Name));
						SetFeatureStruct(cache, feature, (string)assignment.Value, fs => p.FeaturesOA = fs);
					}
					if ((bool?)op["distinct"] == true)
					{
						var identity = cache.ServiceLocator.GetInstance<IFsClosedFeatureFactory>().Create();
						cache.LanguageProject.PhFeatureSystemOA.FeaturesOC.Add(identity);
						identity.Name.SetAnalysisDefaultWritingSystem("identity");
						identity.Abbreviation.SetAnalysisDefaultWritingSystem("identity");
						foreach (JProperty assignment in ((JObject)op["assignments"]).Properties())
						{
							var symbol = cache.ServiceLocator.GetInstance<IFsSymFeatValFactory>().Create();
							identity.ValuesOC.Add(symbol);
							symbol.Name.SetAnalysisDefaultWritingSystem(assignment.Name);
							symbol.Abbreviation.SetAnalysisDefaultWritingSystem(assignment.Name);
							var phoneme = phonemes.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == assignment.Name);
							var spec = cache.ServiceLocator.GetInstance<IFsClosedValueFactory>().Create();
							phoneme.FeaturesOA.FeatureSpecsOC.Add(spec);
							spec.FeatureRA = identity;
							spec.ValueRA = symbol;
						}
					}
					break;
				case "segment-class":
					var segmentClass = cache.ServiceLocator.GetInstance<IPhNCSegmentsFactory>().Create();
					pd.NaturalClassesOS.Add(segmentClass);
					segmentClass.Name.SetAnalysisDefaultWritingSystem("V");
					segmentClass.Abbreviation.SetAnalysisDefaultWritingSystem("V");
					foreach (var member in (JArray)op["members"])
						segmentClass.SegmentsRC.Add(phonemes.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)member));
					break;
				case "rewrite":
					var rule = cache.ServiceLocator.GetInstance<IPhRegularRuleFactory>().Create();
					pd.PhonRulesOS.Add(rule);
					rule.Name.SetAnalysisDefaultWritingSystem("probe-rewrite");
					rule.Direction = 0;
					var input = cache.ServiceLocator.GetInstance<IPhSimpleContextSegFactory>().Create();
					rule.StrucDescOS.Add(input);
					input.FeatureStructureRA = phonemes.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["input"]);
					var rhs = rule.RightHandSidesOS[0];
					var replacement = cache.ServiceLocator.GetInstance<IPhSimpleContextSegFactory>().Create();
					rhs.StrucChangeOS.Add(replacement);
					replacement.FeatureStructureRA = phonemes.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["output"]);
					if (op["leftClass"] != null)
					{
						var left = cache.ServiceLocator.GetInstance<IPhSimpleContextNCFactory>().Create();
						rhs.LeftContextOA = left;
						left.FeatureStructureRA = pd.NaturalClassesOS.Single(n => n.Abbreviation.BestAnalysisAlternative.Text == (string)op["leftClass"]);
					}
					else
					{
						var left = cache.ServiceLocator.GetInstance<IPhSimpleContextSegFactory>().Create();
						rhs.LeftContextOA = left;
						left.FeatureStructureRA = phonemes.Single(p => p.CodesOS[0].Representation.VernacularDefaultWritingSystem.Text == (string)op["left"]);
					}
					break;
			}
		}

		private static void SetFeatureStruct(LcmCache cache, IFsClosedFeature feature, string value, Action<IFsFeatStruc> attach)
		{
			var fs = cache.ServiceLocator.GetInstance<IFsFeatStrucFactory>().Create();
			attach(fs);
			var cv = cache.ServiceLocator.GetInstance<IFsClosedValueFactory>().Create();
			fs.FeatureSpecsOC.Add(cv);
			cv.FeatureRA = feature;
			cv.ValueRA = feature.ValuesOC.Single(v => v.Abbreviation.BestAnalysisAlternative.Text == value);
		}

		private static void CopyDirectory(string source, string destination)
		{
			Directory.CreateDirectory(destination);
			foreach (var file in Directory.GetFiles(source)) File.Copy(file, Path.Combine(destination, Path.GetFileName(file)));
			foreach (var dir in Directory.GetDirectories(source)) CopyDirectory(dir, Path.Combine(destination, Path.GetFileName(dir)));
		}
	}
}
