using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Newtonsoft.Json.Linq;
using SIL.LCModel;
using SIL.LCModel.Infrastructure;

namespace XampleProjector
{
	/// <summary>
	/// Applies a scripted LibLCM phoneme-inventory counterfactual to a CLONE of a FieldWorks
	/// project, never the source: clone the whole project directory, resolve and check the
	/// targets, refuse (leaving the clone as-is, nothing deleted) if any target is still
	/// referenced, delete inside one UOW, flush, reopen the clone and verify the deletion by
	/// re-reading it -- never by trusting the in-memory state that just performed it.
	/// </summary>
	internal static class MutateCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--project", out var sourceProjectPath) ||
				!ArgParser.TryGetOption(args, "--request", out var requestPath) ||
				!ArgParser.TryGetOption(args, "--out-dir", out var outDir))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}

			if (!File.Exists(sourceProjectPath))
			{
				Console.Error.WriteLine("Mutation failure: source project not found: {0}", sourceProjectPath);
				return ExitCodes.ProjectOpenFailure;
			}
			if (!File.Exists(requestPath))
			{
				Console.Error.WriteLine("Mutation failure: request file not found: {0}", requestPath);
				return ExitCodes.Usage;
			}

			JObject request;
			try
			{
				request = JObject.Parse(File.ReadAllText(requestPath));
			}
			catch (Exception ex)
			{
				Console.Error.WriteLine("Mutation refusal: request is not valid JSON: {0}", ex.Message);
				return ExitCodes.MutationRefusal;
			}

			Directory.CreateDirectory(outDir);

			try
			{
				return RunMutation(fieldWorksDir, sourceProjectPath, request, outDir);
			}
			catch (MutationException ex)
			{
				var kind = ex.ExitCode == ExitCodes.MutationIntegrityFailure ? "integrity failure" : "refusal";
				Console.Error.WriteLine("Mutation {0}: {1}", kind, ex.Message);
				return ex.ExitCode;
			}
		}

		private static int RunMutation(string fieldWorksDir, string sourceProjectPath, JObject request, string outDir)
		{
			var schemaVersion = (int?)request["schemaVersion"];
			if (schemaVersion != SchemaVersion.Current)
			{
				throw new MutationException(ExitCodes.MutationRefusal, "mutation.unsupported-schema-version",
					$"request declares schemaVersion {(schemaVersion.HasValue ? schemaVersion.Value.ToString() : "<missing>")}, but only schemaVersion {SchemaVersion.Current} is supported");
			}
			var caseId = (string)request["caseId"];
			if (string.IsNullOrEmpty(caseId))
				throw new MutationException(ExitCodes.MutationRefusal, "mutation.missing-case-id", "request has no \"caseId\"");
			var baseSha256 = (string)request["baseSha256"];
			if (string.IsNullOrEmpty(baseSha256))
				throw new MutationException(ExitCodes.MutationRefusal, "mutation.missing-base-sha256", "request has no \"baseSha256\"");
			if (!(request["operations"] is JArray operations) || operations.Count == 0)
				throw new MutationException(ExitCodes.MutationRefusal, "mutation.no-operations", "request has no non-empty \"operations\" array");

			// Step 1: verify the source's digest against what the caller thought they were
			// operating on, BEFORE anything is cloned or touched.
			var initialSourceHash = Sha256.OfFile(sourceProjectPath);
			if (!string.Equals(initialSourceHash, baseSha256, StringComparison.OrdinalIgnoreCase))
				throw new MutationException(ExitCodes.MutationIntegrityFailure, "mutation.source-digest-mismatch",
					$"source \"{sourceProjectPath}\" sha256 {initialSourceHash} does not match request baseSha256 {baseSha256}");

			// Step 2: clone the WHOLE project directory (WritingSystemStore included) -- every
			// later step touches only the clone.
			var sourceProjectDir = Path.GetDirectoryName(Path.GetFullPath(sourceProjectPath));
			var cloneProjectDir = Path.Combine(outDir, Path.GetFileName(sourceProjectDir));
			if (Directory.Exists(cloneProjectDir))
				throw new MutationException(ExitCodes.MutationRefusal, "mutation.clone-target-exists", $"clone target already exists: {cloneProjectDir}");
			CopyDirectory(sourceProjectDir, cloneProjectDir);
			var cloneProjectPath = Path.Combine(cloneProjectDir, Path.GetFileName(sourceProjectPath));

			var removed = new List<JObject>();
			var allInboundReferences = new List<JObject>();
			var blockingReferences = new List<string>();
			var deletedCount = 0;
			var countBeforeDelete = -1;

			var openExit = FieldWorksSession.Run(fieldWorksDir, cloneProjectPath, (cache, logger) =>
			{
				var phonemeSet = cache.LanguageProject.PhonologicalDataOA?.PhonemeSetsOS.FirstOrDefault();
				if (phonemeSet == null)
					throw new MutationException(ExitCodes.MutationRefusal, "mutation.no-phoneme-set", "the project has no phoneme set");
				countBeforeDelete = phonemeSet.PhonemesOC.Count;

				var targets = new List<IPhPhoneme>();
				var targetGuids = new HashSet<Guid>();

				foreach (var opToken in operations)
				{
					var op = (JObject)opToken;
					var opName = (string)op["op"];
					var requireUnreferenced = (bool?)op["requireUnreferenced"] ?? false;

					switch (opName)
					{
						case "remove_phoneme":
						{
							var guidText = (string)op["guid"];
							if (!Guid.TryParse(guidText, out var guid))
								throw new MutationException(ExitCodes.MutationRefusal, "mutation.invalid-guid", $"operation guid is not a valid guid: \"{guidText}\"");
							var phoneme = phonemeSet.PhonemesOC.FirstOrDefault(p => p.Guid == guid);
							if (phoneme == null)
								throw new MutationException(ExitCodes.MutationRefusal, "mutation.unknown-target", $"no phoneme with guid {guid} in the project's phoneme set");
							AddTarget(targets, targetGuids, phoneme);

							var assertToken = op["assertRepresentations"] as JArray;
							if (assertToken != null)
							{
								var expected = assertToken.Select(t => (string)t).ToList();
								var actual = Representations(phoneme);
								if (!actual.SequenceEqual(expected))
									throw new MutationException(ExitCodes.MutationRefusal, "mutation.representation-mismatch",
										$"phoneme {guid}: expected representations [{string.Join(",", expected)}], found [{string.Join(",", actual)}]");
							}
							RecordReferences(phoneme, requireUnreferenced, allInboundReferences, blockingReferences);
							break;
						}
						case "remove_all_phonemes":
						{
							foreach (var phoneme in phonemeSet.PhonemesOC.OrderBy(p => p.Guid.ToString(), StringComparer.Ordinal))
							{
								AddTarget(targets, targetGuids, phoneme);
								RecordReferences(phoneme, requireUnreferenced, allInboundReferences, blockingReferences);
							}
							break;
						}
						default:
							throw new MutationException(ExitCodes.MutationRefusal, "mutation.unknown-operation", $"unknown operation \"{opName}\"");
					}
				}

				if (targets.Count == 0)
					throw new MutationException(ExitCodes.MutationRefusal, "mutation.no-targets", "no phoneme targets resolved from the request's operations");

				if (blockingReferences.Count > 0)
					throw new MutationException(ExitCodes.MutationRefusal, "mutation.referenced-phoneme",
						"target phoneme(s) are still referenced, deleting nothing: " + string.Join("; ", blockingReferences));

				foreach (var target in targets)
					removed.Add(new JObject { ["guid"] = target.Guid.ToString(), ["representations"] = new JArray(Representations(target)) });

				NonUndoableUnitOfWorkHelper.Do(cache.ActionHandlerAccessor, () =>
				{
					foreach (var target in targets)
						target.Delete();
				});
				cache.ServiceLocator.GetInstance<IUndoStackManager>().Save();
				deletedCount = targets.Count;
				return ExitCodes.Ok;
			});
			if (openExit != ExitCodes.Ok)
				return openExit;

			// Step 5: reopen the clone and verify by re-reading it -- never by trusting the
			// in-memory state that just performed the delete.
			var removedGuids = new HashSet<string>(removed.Select(r => (string)r["guid"]), StringComparer.OrdinalIgnoreCase);
			var reopenExit = FieldWorksSession.Run(fieldWorksDir, cloneProjectPath, (cache, logger) =>
			{
				var phonemeSet = cache.LanguageProject.PhonologicalDataOA?.PhonemeSetsOS.FirstOrDefault();
				var countAfter = phonemeSet?.PhonemesOC.Count ?? 0;
				if (countBeforeDelete - countAfter != deletedCount)
					throw new MutationException(ExitCodes.MutationIntegrityFailure, "mutation.deletion-unverified",
						$"phoneme count before delete was {countBeforeDelete}, after reopen is {countAfter}, expected a drop of exactly {deletedCount}");
				var stillPresent = phonemeSet?.PhonemesOC.Where(p => removedGuids.Contains(p.Guid.ToString())).Select(p => p.Guid.ToString()).ToList()
					?? new List<string>();
				if (stillPresent.Count > 0)
					throw new MutationException(ExitCodes.MutationIntegrityFailure, "mutation.deletion-unverified",
						$"phoneme(s) still present after reopen: {string.Join(", ", stillPresent)}");
				return ExitCodes.Ok;
			});
			if (reopenExit != ExitCodes.Ok)
				return reopenExit;

			// Step 2 (closing half): the source must be byte-for-byte unchanged by all of the above.
			var finalSourceHash = Sha256.OfFile(sourceProjectPath);
			if (finalSourceHash != initialSourceHash)
				throw new MutationException(ExitCodes.MutationIntegrityFailure, "mutation.source-modified",
					$"source \"{sourceProjectPath}\" sha256 changed from {initialSourceHash} to {finalSourceHash}");

			var response = new JObject
			{
				[Fields.SchemaVersion] = SchemaVersion.Current,
				[Fields.Mode] = "mutate",
				[Fields.CaseId] = caseId,
				[Fields.BaseSha256] = baseSha256,
				[Fields.MaterializedSha256] = Sha256.OfFile(cloneProjectPath),
				[Fields.MaterializedProjectPath] = PathUtil.MakeRelative(outDir, cloneProjectPath),
				[Fields.Removed] = new JArray(removed),
				[Fields.InboundReferences] = new JArray(allInboundReferences),
				[Fields.Reopened] = true,
				[Fields.DeletedCount] = deletedCount,
				[Fields.Diagnostics] = new JArray(),
			};
			var responsePath = Path.Combine(outDir, "mutation-response.json");
			JsonWriter.WriteFile(responsePath, response);
			Console.WriteLine("Wrote {0}", responsePath);
			Console.WriteLine("Deleted {0} phoneme(s); materialized project: {1}", deletedCount, cloneProjectPath);
			return ExitCodes.Ok;
		}

		private static void AddTarget(List<IPhPhoneme> targets, HashSet<Guid> targetGuids, IPhPhoneme phoneme)
		{
			if (!targetGuids.Add(phoneme.Guid))
				throw new MutationException(ExitCodes.MutationRefusal, "mutation.duplicate-target", $"phoneme {phoneme.Guid} targeted by more than one operation");
			targets.Add(phoneme);
		}

		private static List<string> Representations(IPhPhoneme phoneme)
		{
			var result = new List<string>();
			foreach (var code in phoneme.CodesOS)
			{
				var text = code.Representation?.VernacularDefaultWritingSystem?.Text;
				if (!string.IsNullOrEmpty(text))
					result.Add(text);
			}
			return result;
		}

		/// <summary>
		/// Always records every referrer (informational: the response's inboundReferences is
		/// non-empty only when a NON-blocking op still had referents), and separately names a
		/// blocking one -- requireUnreferenced turns "recorded" into "the request refuses".
		/// </summary>
		private static void RecordReferences(IPhPhoneme phoneme, bool requireUnreferenced, List<JObject> allInboundReferences, List<string> blockingReferences)
		{
			foreach (var referrer in phoneme.ReferringObjects)
			{
				allInboundReferences.Add(new JObject
				{
					["targetGuid"] = phoneme.Guid.ToString(),
					["referrerGuid"] = referrer.Guid.ToString(),
					["referrerClass"] = referrer.ClassName,
				});
				if (requireUnreferenced)
					blockingReferences.Add($"{referrer.ClassName} {referrer.Guid} (\"{referrer.ShortName}\") -> phoneme {phoneme.Guid}");
			}
		}

		private static void CopyDirectory(string sourceDir, string destDir)
		{
			Directory.CreateDirectory(destDir);
			foreach (var filePath in Directory.GetFiles(sourceDir))
				File.Copy(filePath, Path.Combine(destDir, Path.GetFileName(filePath)), overwrite: false);
			foreach (var subDir in Directory.GetDirectories(sourceDir))
				CopyDirectory(subDir, Path.Combine(destDir, Path.GetFileName(subDir)));
		}
	}
}
