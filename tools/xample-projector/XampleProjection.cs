using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Xml.Linq;
using System.Xml.XPath;
using System.Xml.Xsl;
using SIL.LCModel;
using SIL.LCModel.DomainServices;
using SIL.WordWorks.GAFAWS.PositionAnalysis;

namespace XampleProjector
{
	/// <summary>
	/// Drives the same XSL transforms and GAFAWS step FieldWorks' internal (and inaccessible)
	/// M3ToXAmpleTransformer/XAmpleParser.Update use, adapted to write into a caller-chosen
	/// directory instead of the hardcoded %TEMP% the original always uses.
	/// </summary>
	internal static class XampleProjection
	{
		internal static List<GeneratedFile> Generate(LcmCache cache, string fieldWorksDir, string outDir, string database)
		{
			var lp = cache.LanguageProject;
			var model = M3ModelExportServices.ExportGrammarAndLexicon(lp);
			var template = M3ModelExportServices.ExportGafaws(lp.PartsOfSpeechOA.PossibilitiesOS);

			// One path can be written more than once (each POS-with-templates iteration below
			// reuses the same GAFAWS input/output names), so collect the set of distinct paths
			// touched and describe each from its FINAL on-disk state once, after every write is
			// done -- otherwise an earlier, now-stale write's digest would be reported instead
			// of what the file actually contains.
			var touchedPaths = new List<string>();
			touchedPaths.AddRange(PrepareTemplatesForXAmpleFiles(model, template, fieldWorksDir, outDir, database));

			foreach (var element in model.Elements())
				RemoveDottedCircles(element);

			touchedPaths.AddRange(MakeAmpleFiles(model, fieldWorksDir, outDir, database));
			return touchedPaths.Distinct().Select(GeneratedFile.Describe).ToList();
		}

		// --- ported from M3ToXAmpleTransformer.PrepareTemplatesForXAmpleFiles / DefineUndefinedSlots /
		// GetUndefinedSlots / InsertOrderclassInfo / TransformPosInfoToGafawsInputFormat / ApplyGafawsAlgorithm ---

		private static List<string> PrepareTemplatesForXAmpleFiles(XDocument domModel, XDocument domTemplate, string fieldWorksDir, string outDir, string database)
		{
			var touchedPaths = new List<string>();
			foreach (var templateElem in domTemplate.Root.Elements("PartsOfSpeech").Elements("PartOfSpeech")
				.Where(pe => pe.DescendantsAndSelf().Elements("AffixTemplates").Elements("MoInflAffixTemplate")
					.Any(te => te.Element("PrefixSlots") != null || te.Element("SuffixSlots") != null)))
			{
				DefineUndefinedSlots(templateElem);

				var gafawsInputPath = Path.Combine(outDir, database + "gafawsData.xml");
				var gafawsTransform = LoadTransform(fieldWorksDir, "FxtM3ParserToGAFAWS");
				var templateDom = new XDocument(new XElement(templateElem));
				using (var writer = new StreamWriter(gafawsInputPath))
					gafawsTransform.Transform(templateDom.CreateNavigator(), null, writer);
				touchedPaths.Add(gafawsInputPath);

				var pa = new PositionAnalyzer();
				var resultFile = pa.Process(gafawsInputPath);
				if (string.IsNullOrEmpty(resultFile))
					continue;
				touchedPaths.Add(resultFile);

				InsertOrderclassInfo(domModel, resultFile);
			}
			return touchedPaths;
		}

		private static void DefineUndefinedSlots(XElement templateElem)
		{
			var undefinedSlots = new HashSet<string>();
			GetUndefinedSlots(templateElem, undefinedSlots);
			if (undefinedSlots.Count == 0)
				return;
			foreach (var elem in templateElem.Elements())
			{
				if (elem.Name != "AffixSlots")
					continue;
				foreach (var slotId in undefinedSlots)
				{
					var slot = new XElement("MoInflAffixSlot");
					slot.SetAttributeValue("Id", slotId);
					elem.Add(slot);
				}
				break;
			}
		}

		private static void GetUndefinedSlots(XElement element, ISet<string> undefinedSlots)
		{
			foreach (var elem in element.Elements())
				GetUndefinedSlots(elem, undefinedSlots);

			if (element.Name == "PrefixSlots" || element.Name == "SuffixSlots")
				undefinedSlots.Add((string)element.Attribute("dst"));

			var affixSlotsElem = element.Element("AffixSlots");
			if (affixSlotsElem == null)
				return;
			foreach (var slot in affixSlotsElem.Elements())
				undefinedSlots.Remove((string)slot.Attribute("Id"));
		}

		private static void InsertOrderclassInfo(XDocument domModel, string resultFile)
		{
			var dom = XDocument.Load(resultFile);
			foreach (var gafawsElem in dom.Elements("GAFAWSData").Elements("Morphemes").Elements("Morpheme"))
			{
				var morphemeId = (string)gafawsElem.Attribute("MID");
				if (morphemeId == "R")
					continue;
				var modelElem = domModel.Descendants("MoInflAffixSlot").First(e => (string)e.Attribute("Id") == morphemeId);
				modelElem.Add(new XElement("orderclass",
					new XElement("minValue", (string)gafawsElem.Attribute("StartCLIDREF")),
					new XElement("maxValue", (string)gafawsElem.Attribute("EndCLIDREF"))));
			}
		}

		// --- ported from XAmpleParser.RemoveDottedCircles ---

		private static void RemoveDottedCircles(XElement element)
		{
			if (!element.Elements().Any())
			{
				element.Value = element.Value?.Replace("◌", string.Empty);
				return;
			}
			foreach (var subElement in element.Elements())
				RemoveDottedCircles(subElement);
		}

		// --- ported from M3ToXAmpleTransformer.MakeAmpleFiles, parameterized on outDir instead of %TEMP% ---

		private static List<string> MakeAmpleFiles(XDocument model, string fieldWorksDir, string outDir, string database)
		{
			return new List<string>
			{
				TransformToFile(model, fieldWorksDir, "FxtM3ParserToXAmpleADCtl", Path.Combine(outDir, database + "adctl.txt")),
				TransformToFile(model, fieldWorksDir, "FxtM3ParserToToXAmpleGrammar", Path.Combine(outDir, database + "gram.txt")),
				TransformToFile(model, fieldWorksDir, "FxtM3ParserToXAmpleWordGrammarDebuggingXSLT", Path.Combine(outDir, database + "XAmpleWordGrammarDebugger.xsl")),
				TransformToFile(model, fieldWorksDir, "FxtM3ParserToXAmpleLex", Path.Combine(outDir, database + "lex.txt")),
			};
		}

		private static string TransformToFile(XDocument model, string fieldWorksDir, string xslName, string outPath)
		{
			var xslt = LoadTransform(fieldWorksDir, xslName);
			using (var writer = new StreamWriter(outPath))
				xslt.Transform(model.CreateNavigator(), null, writer);
			return outPath;
		}

		private static XslCompiledTransform LoadTransform(string fieldWorksDir, string xslName)
		{
			var path = Path.Combine(fieldWorksDir, "Transforms", "Application", xslName + ".xsl");
			if (!File.Exists(path))
				throw new ProjectionException($"missing XAMPLE transform: {path}");
			try
			{
				var xslt = new XslCompiledTransform();
				xslt.Load(path);
				return xslt;
			}
			catch (Exception ex)
			{
				throw new ProjectionException($"failed to load XAMPLE transform {path}: {ex.Message}", ex);
			}
		}
	}
}
