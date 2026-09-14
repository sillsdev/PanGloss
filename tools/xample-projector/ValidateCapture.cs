using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using Newtonsoft.Json.Linq;

namespace XampleProjector
{
	/// <summary>
	/// Schema validation only, over a captured response.json -- no FieldWorks install needed.
	/// This is the portable-CI path: it runs the same shape checks a fresh capture must satisfy,
	/// without requiring the machine to have FieldWorks 9 installed at all.
	/// </summary>
	internal static class ValidateCapture
	{
		private static readonly Regex Sha256Pattern = new Regex("^[0-9a-f]{64}$", RegexOptions.Compiled);

		// "mode" is itself required in both shapes, precisely so a caller (this file included)
		// never has to guess which set applies -- see CheckRequiredFields.
		private static readonly string[] ProjectRequiredFields =
		{
			Fields.SchemaVersion, Fields.Mode, Fields.FieldWorksVersion, Fields.AssemblyVersions,
			Fields.SourcePath, Fields.SourceSha256, Fields.Database, Fields.Generated,
			Fields.HcLoadDiagnostics, Fields.Diagnostics,
		};

		private static readonly string[] InspectRequiredFields =
		{
			Fields.SchemaVersion, Fields.Mode, Fields.FieldWorksVersion, Fields.SourcePath, Fields.SourceSha256,
			Fields.ProjectName, Fields.ActiveParser, Fields.Phonemes, Fields.BoundaryMarkers,
			Fields.NaturalClasses, Fields.Diagnostics,
		};

		private static readonly string[] AuthorRequiredFields =
		{
			Fields.SchemaVersion, Fields.Mode, Fields.FieldWorksVersion, Fields.GrammarPath, Fields.GrammarSha256,
			Fields.ProjectPath, Fields.ProjectSha256, Fields.Authored, Fields.Unmapped, Fields.GuidMap, Fields.Diagnostics,
		};

		private static readonly string[] MutateRequiredFields =
		{
			Fields.SchemaVersion, Fields.Mode, Fields.CaseId, Fields.BaseSha256, Fields.MaterializedSha256,
			Fields.MaterializedProjectPath, Fields.Removed, Fields.InboundReferences, Fields.Reopened,
			Fields.DeletedCount, Fields.Diagnostics,
		};

		private static readonly string[] ParseRequiredFields =
		{
			Fields.SchemaVersion, Fields.Mode, Fields.Database, Fields.EngineVersion, Fields.Parameters, Fields.Words,
		};

		internal static int Run(string path)
		{
			if (!File.Exists(path))
			{
				Console.Error.WriteLine("Capture validation failure: file not found: {0}", path);
				return ExitCodes.CaptureValidationFailure;
			}

			JObject document;
			try
			{
				document = JObject.Parse(File.ReadAllText(path));
			}
			catch (Exception ex)
			{
				Console.Error.WriteLine("Capture validation failure: not valid JSON: {0}", ex.Message);
				return ExitCodes.CaptureValidationFailure;
			}

			var problems = new List<string>();
			CheckRequiredFieldsForMode(document, problems);
			CheckSchemaVersion(document, problems);
			CheckSha256Fields(document, problems);
			CheckAssemblyVersionKeys(document, problems);
			CheckNoAbsolutePaths(document, problems);
			CheckParseParametersShape(document, problems);

			if (problems.Count == 0)
			{
				Console.WriteLine("Capture valid: {0}", path);
				return ExitCodes.Ok;
			}

			Console.Error.WriteLine("Capture validation failure: {0}", path);
			foreach (var problem in problems)
				Console.Error.WriteLine("  - {0}", problem);
			return ExitCodes.CaptureValidationFailure;
		}

		/// <summary>
		/// Required fields are scoped by "mode" -- inspect and project responses are disjoint
		/// shapes, and validating one against the other's field set would either demand fields
		/// that mode never produces or silently accept a capture missing fields its own mode
		/// requires.
		/// </summary>
		private static void CheckRequiredFieldsForMode(JObject document, List<string> problems)
		{
			var mode = (string)document[Fields.Mode];
			if (mode == null)
			{
				problems.Add($"missing required field \"{Fields.Mode}\" (required fields cannot be scoped without it)");
				return;
			}

			string[] required;
			switch (mode)
			{
				case "project":
					required = ProjectRequiredFields;
					break;
				case "inspect":
					required = InspectRequiredFields;
					break;
				case "author":
					required = AuthorRequiredFields;
					break;
				case "mutate":
					required = MutateRequiredFields;
					break;
				case "parse":
					required = ParseRequiredFields;
					break;
				default:
					problems.Add($"[{mode}] unknown mode (expected \"inspect\", \"project\", \"author\", \"mutate\", or \"parse\")");
					return;
			}

			foreach (var field in required)
			{
				if (document[field] == null)
					problems.Add($"[{mode}] missing required field \"{field}\"");
			}
		}

		private static void CheckSchemaVersion(JObject document, List<string> problems)
		{
			var schemaVersion = document[Fields.SchemaVersion];
			if (schemaVersion != null && schemaVersion.Type == JTokenType.Integer && (int)schemaVersion == SchemaVersion.Current)
				return;
			problems.Add($"\"{Fields.SchemaVersion}\" must equal {SchemaVersion.Current}");
		}

		private static void CheckSha256Fields(JObject document, List<string> problems)
		{
			var sourceSha256 = (string)document[Fields.SourceSha256];
			if (sourceSha256 != null && !Sha256Pattern.IsMatch(sourceSha256))
				problems.Add($"\"{Fields.SourceSha256}\" is not 64 lowercase hex characters: \"{sourceSha256}\"");

			foreach (var field in new[] { Fields.GrammarSha256, Fields.ProjectSha256, Fields.BaseSha256, Fields.MaterializedSha256 })
			{
				var value = (string)document[field];
				if (value != null && !Sha256Pattern.IsMatch(value))
					problems.Add($"\"{field}\" is not 64 lowercase hex characters: \"{value}\"");
			}

			if (!(document[Fields.Generated] is JArray generated))
				return;
			foreach (var entry in generated.OfType<JObject>())
			{
				var sha256 = (string)entry["sha256"];
				if (sha256 == null)
				{
					problems.Add("a generated[] entry is missing \"sha256\"");
					continue;
				}
				if (!Sha256Pattern.IsMatch(sha256))
					problems.Add($"generated[].sha256 is not 64 lowercase hex characters: \"{sha256}\" (path {(string)entry["path"]})");
			}
		}

		private static void CheckAssemblyVersionKeys(JObject document, List<string> problems)
		{
			if (!(document[Fields.AssemblyVersions] is JObject assemblyVersions))
				return;

			var expectedKeys = new HashSet<string>(FieldWorksPins.ExpectedFileVersions.Keys);
			var actualKeys = new HashSet<string>(assemblyVersions.Properties().Select(p => p.Name));

			foreach (var missing in expectedKeys.Except(actualKeys))
				problems.Add($"\"{Fields.AssemblyVersions}\" is missing pinned key \"{missing}\"");
			foreach (var extra in actualKeys.Except(expectedKeys))
				problems.Add($"\"{Fields.AssemblyVersions}\" has an unpinned key \"{extra}\"");
		}

		/// <summary>
		/// A captured response.json is meant to be checked in and read on any machine, so
		/// neither "sourcePath" nor any generated[].path may leak the machine-specific absolute
		/// path (drive letter or UNC root) it happened to be produced under. "sourcePath" must
		/// be relative (both modes now write it relative to their own output location) or the
		/// "&lt;fw-projects-dir&gt;/..." placeholder convention used by checked-in fixtures.
		/// </summary>
		private static void CheckNoAbsolutePaths(JObject document, List<string> problems)
		{
			var sourcePath = (string)document[Fields.SourcePath];
			if (sourcePath != null && LooksRooted(sourcePath))
				problems.Add($"\"{Fields.SourcePath}\" must not be an absolute path or drive letter: \"{sourcePath}\"");

			foreach (var field in new[] { Fields.GrammarPath, Fields.ProjectPath, Fields.MaterializedProjectPath })
			{
				var value = (string)document[field];
				if (value != null && LooksRooted(value))
					problems.Add($"\"{field}\" must not be an absolute path or drive letter: \"{value}\"");
			}

			if (!(document[Fields.Generated] is JArray generated))
				return;
			foreach (var entry in generated.OfType<JObject>())
			{
				var path = (string)entry["path"];
				if (path != null && LooksRooted(path))
					problems.Add($"generated[].path must not be an absolute path or drive letter: \"{path}\"");
			}
		}

		private static readonly string[] AdctlCapFields =
		{
			"maxPrefixes", "maxSuffixes", "maxInfixes", "maxRoots", "maxInterfixes", "maxNulls",
		};

		/// <summary>
		/// A "parse" response's "parameters" distinguishes the one runtime-overridable cap
		/// (SetParameter) from the caps baked into adctl.txt at author/project time -- flattening
		/// them into sibling keys (the pre-fix shape) made it impossible to tell a cap's source.
		/// </summary>
		private static void CheckParseParametersShape(JObject document, List<string> problems)
		{
			if ((string)document[Fields.Mode] != "parse")
				return;
			if (!(document[Fields.Parameters] is JObject parameters))
				return; // already reported by CheckRequiredFieldsForMode

			if (!(parameters["runtime"] is JObject runtime) || runtime["maxAnalysesToReturn"] == null)
				problems.Add("[parse] \"parameters.runtime.maxAnalysesToReturn\" is missing");

			if (!(parameters["adctl"] is JObject adctl))
			{
				problems.Add("[parse] \"parameters.adctl\" is missing or not an object");
			}
			else
			{
				foreach (var field in AdctlCapFields)
				{
					if (adctl[field] == null)
						problems.Add($"[parse] \"parameters.adctl.{field}\" is missing");
				}
			}

			if (parameters["adctlPatched"] == null || parameters["adctlPatched"].Type != JTokenType.Boolean)
				problems.Add("[parse] \"parameters.adctlPatched\" is missing or not a boolean");

			var adctlSource = (string)parameters["adctlSource"];
			if (string.IsNullOrEmpty(adctlSource))
				problems.Add("[parse] \"parameters.adctlSource\" is missing");
			else if (LooksRooted(adctlSource))
				problems.Add($"\"parameters.adctlSource\" must not be an absolute path or drive letter: \"{adctlSource}\"");
		}

		/// <summary>
		/// "starts with a drive letter, or with a directory separator" -- exactly what an
		/// absolute or UNC path looks like, and exactly what a relative path (or the
		/// "&lt;fw-projects-dir&gt;/..." placeholder, which starts with "&lt;") never does. Deliberately
		/// NOT Path.IsPathRooted: on net48 it validates the string contains no illegal path
		/// characters first and throws ArgumentException on '&lt;'/'&gt;' -- exactly the placeholder's
		/// own characters -- so it cannot be used on a value that is only sometimes a real path.
		/// </summary>
		private static bool LooksRooted(string candidate)
		{
			if (string.IsNullOrEmpty(candidate))
				return false;
			if (candidate[0] == '\\' || candidate[0] == '/')
				return true;
			return candidate.Length >= 2 && char.IsLetter(candidate[0]) && candidate[1] == ':';
		}
	}
}
