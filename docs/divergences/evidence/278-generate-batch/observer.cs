using System;
using System.Linq;
using System.Collections.Generic;
using SIL.Machine.FeatureModel;
using SIL.Machine.Morphology.HermitCrab;

var language = XmlLanguageLoader.Load(args[0]);
var morpher = new Morpher(new TraceManager(), language);
var stratum = language.Strata[0];
var root = stratum.Entries.Single(e => e.Id == "root-id");
var rules = stratum.MorphologicalRules.Concat(stratum.AffixTemplates.SelectMany(t => t.Slots.SelectMany(sl => sl.Rules))).Cast<MorphemicMorphologicalRule>().ToDictionary(r => r.Id);
string[][] combos = { new string[0], new[]{"infl-id"}, new[]{"deriv-id"}, new[]{"deriv-id","infl-id"}, new[]{"infl-id","deriv-id"} };
foreach (var c in combos) {
  var ms = c.Select(n => (Morpheme)rules[n]).ToArray();
  var s = morpher.GenerateWords(root, ms, FeatureStruct.New().Value).OrderBy(x => x, StringComparer.Ordinal);
  Console.WriteLine(string.Join("+", c.DefaultIfEmpty("(root)")) + "\t" + string.Join(";", s));
}
