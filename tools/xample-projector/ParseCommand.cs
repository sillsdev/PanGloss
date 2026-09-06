using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Xml.Linq;
using Newtonsoft.Json.Linq;
using SIL.LCModel;
using XAmpleManagedWrapper;

namespace XampleProjector
{
	/// <summary>
	/// Drives the real XAmple engine (XAmpleManagedWrapper -&gt; xample.dll) over a project's
	/// already-generated adctl.txt/gram.txt/lex.txt, resolving each returned Morph's MoForm/MSI
	/// DbRef hvo the way SIL.FieldWorks.WordWorks.Parser.XAmpleParser.TryCreateParseMorph does
	/// (Src\LexText\ParserCore\XAmpleParser.cs:234-331) -- but reporting a guid, not the bare hvo,
	/// since an hvo is only stable within one LcmCache session and this tool opens a fresh one.
	///
	/// <c>DescribeMorph</c>'s MSI resolution is a simplified subset of <c>TryCreateParseMorph</c>
	/// (that class is internal, so it cannot be called directly -- see this project's own README
	/// "Provenance"): it handles the common "bare MSA hvo" and "lexEntryHvo.refIndex.msaHvo" (a
	/// real stem MSA at the trailing hvo) shapes, but NOT case 3 (a bare LexEntry hvo, no dots --
	/// XAmpleParser.cs:264-265) or case 4's sense-derived fallback when the trailing hvo does not
	/// resolve to an <c>IMoMorphSynAnalysis</c> (XAmpleParser.cs:317-322, via
	/// <c>MorphServices.GetMainOrFirstSenseOfVariant</c>). Both unhandled shapes report
	/// <c>msaGuid: null</c> rather than a wrong-but-plausible guid -- a known, narrower residual
	/// gap than the one this fix closes, with no live fixture yet to validate a correct
	/// implementation against.
	/// </summary>
	internal static class ParseCommand
	{
		/// <summary>
		/// Every one of these is baked into adctl.txt's own "\maxX" control line at author/project
		/// time (FxtM3ParserToXAmpleADCtl.xsl:130-134 for \maxp/\maxi/\maxs/\maxr/\maxn; \maxnull is
		/// written separately, at line 117) -- confirmed by reading
		/// XAmpleManagedWrapper\XAmpleDLLWrapper.cs: its public SetParameter(name, value) special-
		/// cases ONLY "MaxAnalysesToReturn" and silently drops every other name, so the native
		/// engine has no runtime knob for these at all. A flag here therefore patches a COPY of
		/// adctl.txt before LoadFiles, rather than pretending a SetParameter call would work.
		/// </summary>
		private static readonly IReadOnlyDictionary<string, string> MaxLineFlags = new Dictionary<string, string>
		{
			["--max-prefixes"] = "maxp",
			["--max-suffixes"] = "maxs",
			["--max-infixes"] = "maxi",
			["--max-roots"] = "maxr",
			["--max-interfixes"] = "maxn",
			["--max-nulls"] = "maxnull",
		};

		private const int DefaultMaxAnalysesToReturn = 1000;

		internal static int Run(string[] args, string fieldWorksDir)
		{
			if (!ArgParser.TryGetOption(args, "--project", out var projectPath) ||
				!ArgParser.TryGetOption(args, "--project-dir", out var projectDir) ||
				!ArgParser.TryGetOption(args, "--database", out var database) ||
				!ArgParser.TryGetOption(args, "--words", out var wordsPath) ||
				!ArgParser.TryGetOption(args, "--out", out var outPath))
			{
				Program.WriteUsage();
				return ExitCodes.Usage;
			}
			if (!File.Exists(wordsPath))
			{
				Console.Error.WriteLine("Parse failure: words file not found: {0}", wordsPath);
				return ExitCodes.Usage;
			}

			foreach (var name in new[] { "adctl.txt", "gram.txt", "lex.txt" })
			{
				var path = Path.Combine(projectDir, database + name);
				if (!File.Exists(path))
				{
					Console.Error.WriteLine("Parse engine load failure: missing XAMPLE file: {0}", path);
					return ExitCodes.ParseEngineFailure;
				}
			}
			var cdTablePath = Path.Combine(fieldWorksDir, "Language Explorer", "Configuration", "Grammar", "cd.tab");
			if (!File.Exists(cdTablePath))
			{
				Console.Error.WriteLine("Parse engine load failure: missing XAMPLE file: {0}", cdTablePath);
				return ExitCodes.ParseEngineFailure;
			}

			var overrides = new Dictionary<string, string>();
			foreach (var kv in MaxLineFlags)
			{
				if (ArgParser.TryGetOption(args, kv.Key, out var value))
					overrides[kv.Value] = value;
			}
			int? maxAnalysesOverride = null;
			if (ArgParser.TryGetOption(args, "--max-analyses", out var maxAnalysesText))
				maxAnalysesOverride = int.Parse(maxAnalysesText, CultureInfo.InvariantCulture);

			return FieldWorksSession.Run(fieldWorksDir, projectPath, (cache, logger) =>
			{
				string dynamicFilesDir;
				var patched = overrides.Count > 0;
				var candidateWorkDir = patched
					? Path.Combine(Path.GetTempPath(), "xample-projector-parse-" + Guid.NewGuid().ToString("N"))
					: null;
				try
				{
					dynamicFilesDir = !patched
						? projectDir
						: PatchedDynamicFilesDir(projectDir, database, candidateWorkDir, overrides);
				}
				catch (Exception ex)
				{
					// PatchedDynamicFilesDir can throw after its own Directory.CreateDirectory already
					// succeeded (a File.Copy/File.WriteAllLines failure partway through), which would
					// otherwise leak candidateWorkDir with no cleanup -- the finally block below only
					// runs once dynamicFilesDir is assigned, i.e. never on this path. Same best-effort
					// delete as that finally block, for the same reason.
					if (candidateWorkDir != null)
					{
						try { Directory.Delete(candidateWorkDir, recursive: true); }
						catch (Exception) { /* best-effort cleanup only */ }
					}
					Console.Error.WriteLine("Parse engine load failure: could not patch adctl.txt: {0}", ex.Message);
					return ExitCodes.ParseEngineFailure;
				}

				try
				{
					var loadedAdctlPath = Path.Combine(dynamicFilesDir, database + "adctl.txt");
					var effectiveCaps = ReadEffectiveCaps(loadedAdctlPath);
					var maxAnalysesToReturn = maxAnalysesOverride ?? DefaultMaxAnalysesToReturn;
					var fixedFilesDir = Path.Combine(fieldWorksDir, "Language Explorer", "Configuration", "Grammar");

					using (var xample = new XAmpleWrapper())
					{
						try
						{
							xample.Init();
							xample.SetParameter("MaxAnalysesToReturn", maxAnalysesToReturn.ToString(CultureInfo.InvariantCulture));
							xample.LoadFiles(fixedFilesDir, dynamicFilesDir, database);
						}
						catch (Exception ex)
						{
							Console.Error.WriteLine("Parse engine load failure: {0}", ex.Message);
							return ExitCodes.ParseEngineFailure;
						}

						var words = File.ReadAllLines(wordsPath).Select(w => w.Trim()).Where(w => w.Length > 0).ToList();
						var wordResults = new JArray();
						foreach (var word in words)
							wordResults.Add(ParseOneWord(cache, xample, word));

						var xample64Path = Path.Combine(fieldWorksDir, "xample64.dll");
						// runtime vs. adctl are different provenances (SetParameter vs. a baked-in file); nest, don't flatten.
						var parametersJson = new JObject
						{
							["runtime"] = new JObject { ["maxAnalysesToReturn"] = maxAnalysesToReturn },
							["adctl"] = effectiveCaps,
							["adctlPatched"] = patched,
							["adctlSource"] = PathUtil.MakeRelative(projectDir, loadedAdctlPath),
						};

						var response = new JObject
						{
							[Fields.SchemaVersion] = SchemaVersion.Current,
							[Fields.Mode] = "parse",
							[Fields.Database] = database,
							// AmpleReportVersion is never called by the public managed wrapper surface
							// (XAmpleWrapper/IXAmpleWrapper expose no such method), so the pinned
							// xample64.dll file version is the only honest answer here.
							[Fields.EngineVersion] = File.Exists(xample64Path) ? FileVersionInfo.GetVersionInfo(xample64Path).FileVersion : null,
							[Fields.Parameters] = parametersJson,
							[Fields.Words] = wordResults,
						};
						JsonWriter.WriteFile(outPath, response);
						Console.WriteLine("Wrote {0}", outPath);
						return ExitCodes.Ok;
					}
				}
				finally
				{
					// Best-effort: a leaked scratch copy of adctl/gram/lex.txt costs disk, never
					// correctness, so a failed delete (e.g. an AV scan still holding a handle) must
					// not turn an otherwise-successful parse into a failure.
					if (patched)
					{
						try { Directory.Delete(dynamicFilesDir, recursive: true); }
						catch (Exception) { /* best-effort cleanup only, see comment above */ }
					}
				}
			});
		}

		private static string PatchedDynamicFilesDir(string projectDir, string database, string workDir, IReadOnlyDictionary<string, string> overrides)
		{
			Directory.CreateDirectory(workDir);
			foreach (var name in new[] { "adctl.txt", "gram.txt", "lex.txt" })
				File.Copy(Path.Combine(projectDir, database + name), Path.Combine(workDir, database + name), overwrite: true);

			var adctlPath = Path.Combine(workDir, database + "adctl.txt");
			var lines = File.ReadAllLines(adctlPath);
			for (var i = 0; i < lines.Length; i++)
			{
				foreach (var kv in overrides)
				{
					var prefix = "\\" + kv.Key;
					if (lines[i] == prefix || lines[i].StartsWith(prefix + " ", StringComparison.Ordinal))
						lines[i] = prefix + " " + kv.Value;
				}
			}
			File.WriteAllLines(adctlPath, lines);
			return workDir;
		}

		private static JObject ReadEffectiveCaps(string adctlPath)
		{
			var friendlyNames = new Dictionary<string, string>
			{
				["maxp"] = "maxPrefixes",
				["maxs"] = "maxSuffixes",
				["maxi"] = "maxInfixes",
				["maxr"] = "maxRoots",
				["maxn"] = "maxInterfixes",
				["maxnull"] = "maxNulls",
			};
			var result = new JObject();
			foreach (var line in File.ReadAllLines(adctlPath))
			{
				var trimmed = line.Trim();
				if (trimmed.Length == 0 || trimmed[0] != '\\')
					continue;
				var spaceIndex = trimmed.IndexOf(' ');
				if (spaceIndex < 0)
					continue;
				var code = trimmed.Substring(1, spaceIndex - 1);
				var value = trimmed.Substring(spaceIndex + 1).Trim();
				if (friendlyNames.TryGetValue(code, out var friendly) && int.TryParse(value, out var intValue))
					result[friendly] = intValue;
			}
			return result;
		}

		private static JObject ParseOneWord(LcmCache cache, XAmpleWrapper xample, string word)
		{
			string rawXml;
			try
			{
				rawXml = xample.ParseWord(word);
			}
			catch (Exception ex)
			{
				return new JObject
				{
					["word"] = word,
					["analyses"] = new JArray(),
					["reachedMaxAnalyses"] = false,
					["engineError"] = ex.Message,
				};
			}

			// Same post-processing XAmpleParser.ProcessParseResults applies before XElement.Parse
			// (Src\LexText\ParserCore\XAmpleParser.cs:180-182): the raw string is not well-formed
			// XML until DB_REF_HERE/"<...>" placeholders are replaced.
			var text = rawXml.Replace("DB_REF_HERE", "'0'").Replace("<...>", "[...]").TrimEnd('\0');

			XElement wordformElem;
			try
			{
				wordformElem = XElement.Parse(text);
			}
			catch (Exception ex)
			{
				return new JObject
				{
					["word"] = word,
					["analyses"] = new JArray(),
					["reachedMaxAnalyses"] = false,
					["engineError"] = $"unparseable engine result: {ex.Message}",
				};
			}

			string engineError = null;
			var reachedMax = false;
			var exceptionElem = wordformElem.Element("Exception");
			if (exceptionElem != null)
			{
				var code = (string)exceptionElem.Attribute("code");
				reachedMax = code == "ReachedMaxAnalyses";
				engineError = $"{code} (totalAnalyses={(string)exceptionElem.Attribute("totalAnalyses")})";
			}
			else
			{
				engineError = (string)wordformElem.Element("Error");
			}

			var repo = cache.ServiceLocator.GetInstance<ICmObjectRepository>();
			var analyses = new JArray();
			// Descendants(), not Elements() -- and every WfiAnalysis kept as its own entry, even a
			// byte-identical duplicate of another one: this is a multiset (each is a distinct
			// slot-firing combination even when the surface text and morph list happen to match).
			foreach (var analysisElem in wordformElem.Descendants("WfiAnalysis"))
			{
				var morphemes = new JArray();
				foreach (var morphElem in analysisElem.Descendants("Morph"))
					morphemes.Add(DescribeMorph(repo, morphElem));

				var categoryAttr = analysisElem.Attribute("Category") ?? analysisElem.Attribute("category");
				analyses.Add(new JObject
				{
					["morphemes"] = morphemes,
					["categoryId"] = categoryAttr != null ? (string)categoryAttr : null,
					["surfaceNfd"] = word.Normalize(NormalizationForm.FormD),
				});
			}

			return new JObject
			{
				["word"] = word,
				["analyses"] = analyses,
				["reachedMaxAnalyses"] = reachedMax,
				["engineError"] = engineError,
			};
		}

		private static JObject DescribeMorph(ICmObjectRepository repo, XElement morphElem)
		{
			var formHvoText = (string)morphElem.Element("MoForm")?.Attribute("DbRef");
			var msiHvoText = (string)morphElem.Element("MSI")?.Attribute("DbRef");

			string formText = null, formType = null, morphnameOrGloss = null;
			if (int.TryParse(formHvoText, out var formHvo) && repo.TryGetObject(formHvo, out ICmObject formObj) && formObj is IMoForm form)
			{
				formText = form.Form?.BestVernacularAlternative?.Text;
				formType = form.ClassName;
				morphnameOrGloss = GlossOf(form.Owner as ILexEntry);
			}

			// An irregularly inflected variant's MSI DbRef is "lexEntryHvo.refIndex.msaHvo"
			// (XAmpleParser.cs:266-270): the leading hvo is the variant LexEntry sharing the
			// trailing MSA with other variants of the same headword. Composed below as
			// "{variantEntryGuid}#{msaGuid}" -- the SAME composite key
			// pg_grammar::compile::lexicon::build_variant_stem_entry gives that variant on the HC
			// side (xml_key: format!("{}#{}", variant_entry.guid, guid)) -- so a variant HC
			// distinguishes by entry is no longer silently collapsed to one shared key here.
			var dbRefParts = msiHvoText?.Split('.');
			string variantEntryGuid = null;
			var msaHvoText = msiHvoText;
			if (dbRefParts != null && dbRefParts.Length == 3)
			{
				msaHvoText = dbRefParts[2];
				if (int.TryParse(dbRefParts[0], out var variantEntryHvo) && repo.TryGetObject(variantEntryHvo, out ICmObject variantEntryObj))
					variantEntryGuid = variantEntryObj.Guid.ToString();
			}

			// An unresolved DbRef, or one that resolves to something other than a real MSA (e.g.
			// case 3's bare LexEntry hvo, XAmpleParser.cs:264-265, which this tool's simplified
			// resolution does not fully replicate -- see ParseCommand's own module doc), reports
			// as an explicit absence (null), never the raw hvo/dotted DbRef text: that text is
			// engine-internal, not a guid, and a caller must never mistake it for one.
			string msaGuid = null;
			if (int.TryParse(msaHvoText, out var msaHvo) && repo.TryGetObject(msaHvo, out ICmObject msaObj) && msaObj is IMoMorphSynAnalysis msa)
			{
				msaGuid = variantEntryGuid != null ? $"{variantEntryGuid}#{msa.Guid}" : msa.Guid.ToString();
				if (morphnameOrGloss == null)
					morphnameOrGloss = GlossOf(msa.Owner as ILexEntry);
			}

			return new JObject
			{
				["form"] = formText,
				["msaGuid"] = msaGuid,
				["morphnameOrGloss"] = morphnameOrGloss,
				["type"] = formType,
			};
		}

		private static string GlossOf(ILexEntry entry)
		{
			return entry?.SensesOS.FirstOrDefault()?.Gloss?.BestAnalysisAlternative?.Text;
		}
	}
}
