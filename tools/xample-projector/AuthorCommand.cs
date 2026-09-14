using System;
using System.IO;
using System.Linq;
using System.Xml;
using System.Xml.Linq;
using Newtonsoft.Json.Linq;
using SIL.LCModel;

namespace XampleProjector
{
	/// <summary>
	/// Backs a HermitCrabInput grammar.xml out into a real FieldWorks project via LibLCM,
	/// refusing every construct outside the supported subset (exit 7, naming the construct) before
	/// ever creating the target project -- see GrammarParser (validation) and GrammarAuthor (LCM
	/// object construction).
	/// </summary>
	internal static class AuthorCommand
	{
		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--grammar", out var grammarPath) ||
				!ArgParser.TryGetOption(args, "--out-dir", out var outDir) ||
				!ArgParser.TryGetOption(args, "--name", out var projectName))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}
			if (!ArgParser.TryGetOption(args, "--vernacular-ws", out var vernacularWs))
				vernacularWs = "en";
			var analysisWs = "en";

			int? xampleMaxPrefixes = null;
			if (ArgParser.TryGetOption(args, "--xample-max-prefixes", out var maxPrefixesText))
				xampleMaxPrefixes = int.Parse(maxPrefixesText);
			var xampleMaxAnalyses = 1000;
			if (ArgParser.TryGetOption(args, "--xample-max-analyses", out var maxAnalysesText))
				xampleMaxAnalyses = int.Parse(maxAnalysesText);

			if (!File.Exists(grammarPath))
			{
				Console.Error.WriteLine("Author failure: grammar file not found: {0}", grammarPath);
				return ExitCodes.Usage;
			}

			GrammarModel grammar;
			try
			{
				grammar = GrammarParser.Parse(LoadGrammarDocument(grammarPath));
			}
			catch (GrammarAuthorException ex)
			{
				Console.Error.WriteLine("Author refusal: {0}", ex.Message);
				return ExitCodes.AuthorRefusal;
			}

			Directory.CreateDirectory(outDir);
			var projectDir = Path.Combine(outDir, projectName);
			Directory.CreateDirectory(projectDir);
			var projectPath = Path.Combine(projectDir, projectName + ".fwdata");

			var xampleMaxN = xampleMaxPrefixes ?? Math.Max(5, grammar.AffixTemplates.Sum(t => t.Slots.Count));

			AuthorResult authored = null;
			var sessionExit = AuthorSession.Run(fieldWorksDir, projectPath, analysisWs, vernacularWs, (cache, logger) =>
			{
				try
				{
					authored = GrammarAuthor.Author(cache, grammar, xampleMaxN, xampleMaxN, xampleMaxAnalyses);
					// Commits are asynchronous; force the flush deterministically here (the same
					// public call ProjectLockingService/ProjectBackupService use) rather than
					// relying solely on Dispose, so the project's on-disk state is complete before
					// this method (and the response's ProjectSha256, computed after Dispose) proceed.
					cache.ServiceLocator.GetInstance<IUndoStackManager>().Save();
					return ExitCodes.Ok;
				}
				catch (GrammarAuthorException ex)
				{
					Console.Error.WriteLine("Author refusal: {0}", ex.Message);
					return ExitCodes.AuthorRefusal;
				}
			});
			if (sessionExit != ExitCodes.Ok)
				return sessionExit;

			// The project's own sha256 is computed only now, AFTER AuthorSession.Run's `using`
			// block has disposed the cache -- commits are asynchronous and Dispose is what
			// flushes them (see docs research on LibLCM authoring); hashing any earlier would risk
			// hashing a stale or incomplete .fwdata.
			var response = BuildResponse(fieldWorksDir, outDir, grammarPath, projectPath, grammar, authored);
			var responsePath = Path.Combine(outDir, "author-response.json");
			JsonWriter.WriteFile(responsePath, response);
			Console.WriteLine("Authored project: {0}", projectPath);
			Console.WriteLine("Authored counts:");
			foreach (var kv in authored.Authored.OrderBy(k => k.Key, StringComparer.Ordinal))
				Console.WriteLine("  {0}: {1}", kv.Key, kv.Value);
			return ExitCodes.Ok;
		}

		private static XDocument LoadGrammarDocument(string path)
		{
			// The DTD's SYSTEM identifier is relative to grammar.xml's own directory, which does
			// not always contain a copy of HermitCrabInput.dtd (e.g. machine/conformance/*/*/grammar.xml
			// references machine/conformance/HermitCrabInput.dtd, one directory up) -- DTD
			// processing is ignored entirely since this parser never needs entity/attlist defaults,
			// only the element tree.
			var settings = new XmlReaderSettings { DtdProcessing = DtdProcessing.Ignore };
			using (var reader = XmlReader.Create(path, settings))
				return XDocument.Load(reader);
		}

		private static JObject BuildResponse(string fieldWorksDir, string outDir, string grammarPath, string projectPath,
			GrammarModel grammar, AuthorResult authored)
		{
			var authoredJson = new JObject();
			foreach (var kv in authored.Authored)
				authoredJson[kv.Key] = kv.Value;

			var guidMapJson = new JObject();
			foreach (var kv in authored.GuidMap.OrderBy(k => k.Key, StringComparer.Ordinal))
				guidMapJson[kv.Key] = kv.Value.ToString();

			return new JObject
			{
				[Fields.SchemaVersion] = SchemaVersion.Current,
				[Fields.Mode] = "author",
				[Fields.FieldWorksVersion] = InspectCommand.FieldWorksVersion(fieldWorksDir),
				[Fields.GrammarPath] = PathUtil.MakeRelative(outDir, grammarPath),
				[Fields.GrammarSha256] = Sha256.OfFile(grammarPath),
				[Fields.ProjectPath] = PathUtil.MakeRelative(outDir, projectPath),
				[Fields.ProjectSha256] = Sha256.OfFile(projectPath),
				[Fields.Authored] = authoredJson,
				[Fields.Unmapped] = new JArray(grammar.Unmapped),
				[Fields.GuidMap] = guidMapJson,
				[Fields.Diagnostics] = new JArray(),
			};
		}
	}
}
