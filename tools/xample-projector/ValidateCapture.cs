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
			CheckRequiredFields(document, problems);
			CheckSchemaVersion(document, problems);
			CheckSha256Fields(document, problems);
			CheckAssemblyVersionKeys(document, problems);

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

		private static void CheckRequiredFields(JObject document, List<string> problems)
		{
			string[] required =
			{
				Fields.SchemaVersion, Fields.Mode, Fields.FieldWorksVersion, Fields.AssemblyVersions,
				Fields.SourcePath, Fields.SourceSha256, Fields.Database, Fields.Generated,
				Fields.HcLoadDiagnostics, Fields.Diagnostics,
			};
			foreach (var field in required)
			{
				if (document[field] == null)
					problems.Add($"missing required field \"{field}\"");
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
	}
}
