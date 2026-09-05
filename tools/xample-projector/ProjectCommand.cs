using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Newtonsoft.Json.Linq;
using SIL.FieldWorks.WordWorks.Parser;
using SIL.LCModel;
using SIL.Machine.Morphology.HermitCrab;

namespace XampleProjector
{
	/// <summary>
	/// Produces both real FieldWorks projections of one opened project from a single source
	/// path: the HC XML via HCLoader+XmlLanguageWriter, and the XAMPLE control/dictionary/
	/// grammar files via the same XSL transforms and GAFAWS step FieldWorks itself drives
	/// (see XampleProjection). Both sides read the same opened LcmCache, so they can never
	/// diverge on which project state they saw.
	/// </summary>
	internal static class ProjectCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--project", out var projectPath) ||
				!ArgParser.TryGetOption(args, "--out-dir", out var outDir) ||
				!ArgParser.TryGetOption(args, "--database", out var database))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}

			Directory.CreateDirectory(outDir);

			return FieldWorksSession.Run(projectPath, (cache, logger) =>
			{
				List<GeneratedFile> generated;
				try
				{
					generated = Project(cache, logger, fieldWorksDir, outDir, database);
				}
				catch (ProjectionException ex)
				{
					Console.Error.WriteLine("Projection failure: {0}", ex.Message);
					return ExitCodes.ProjectionFailure;
				}

				var response = BuildResponse(cache, fieldWorksDir, projectPath, database, generated, logger);
				JsonWriter.WriteFile(Path.Combine(outDir, "response.json"), response);

				Console.WriteLine("Generated {0} file(s) in {1}:", generated.Count, outDir);
				foreach (var file in generated)
					Console.WriteLine("  {0} ({1} bytes, sha256 {2})", file.Path, file.Bytes, file.Sha256Hex);

				return ExitCodes.Ok;
			});
		}

		private static List<GeneratedFile> Project(LcmCache cache, DiagnosticLogger logger, string fieldWorksDir, string outDir, string database)
		{
			var generated = new List<GeneratedFile>();

			Language language;
			try
			{
				language = HCLoader.Load(cache, logger);
			}
			catch (Exception ex)
			{
				throw new ProjectionException($"HCLoader.Load failed: {ex.Message}", ex);
			}

			var hcPath = Path.Combine(outDir, database + ".hc.xml");
			try
			{
				XmlLanguageWriter.Save(language, hcPath);
			}
			catch (Exception ex)
			{
				throw new ProjectionException($"XmlLanguageWriter.Save failed: {ex.Message}", ex);
			}
			generated.Add(GeneratedFile.Describe(hcPath));

			generated.AddRange(XampleProjection.Generate(cache, fieldWorksDir, outDir, database));

			return generated;
		}

		private static JObject BuildResponse(LcmCache cache, string fieldWorksDir, string projectPath, string database,
			List<GeneratedFile> generated, DiagnosticLogger logger)
		{
			var hcLoadDiagnostics = new JArray();
			foreach (var diagnostic in logger.Diagnostics)
			{
				hcLoadDiagnostics.Add(new JObject
				{
					["kind"] = diagnostic.Kind,
					["message"] = diagnostic.Message,
				});
			}

			return new JObject
			{
				[Fields.SchemaVersion] = SchemaVersion.Current,
				[Fields.Mode] = "project",
				[Fields.FieldWorksVersion] = InspectCommand.FieldWorksVersion(fieldWorksDir),
				[Fields.AssemblyVersions] = AssemblyVersionsJson(fieldWorksDir),
				[Fields.SourcePath] = projectPath,
				[Fields.SourceSha256] = Sha256.OfFile(projectPath),
				[Fields.Database] = database,
				[Fields.Generated] = new JArray(generated.Select(f => f.ToJson())),
				[Fields.HcLoadDiagnostics] = hcLoadDiagnostics,
				[Fields.Diagnostics] = new JArray(),
			};
		}

		private static JObject AssemblyVersionsJson(string fieldWorksDir)
		{
			var obj = new JObject();
			foreach (var pin in FieldWorksPins.ExpectedFileVersions)
			{
				var path = Path.Combine(fieldWorksDir, pin.Key);
				obj[pin.Key] = File.Exists(path)
					? System.Diagnostics.FileVersionInfo.GetVersionInfo(path).FileVersion
					: null;
			}
			return obj;
		}
	}
}
