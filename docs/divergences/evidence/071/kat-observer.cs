using System;
using System.IO;
using System.Linq;
using SIL.Machine.FeatureModel;
using SIL.Machine.Morphology.HermitCrab;

var language = XmlLanguageLoader.Load(args[0]);
var morpher = new Morpher(new TraceManager(), language);
var root = language.Strata[0].Entries.Single();
Console.WriteLine("GENERATE\t" + root.Id + "\t" + string.Join(";", morpher.GenerateWords(root, Array.Empty<Morpheme>(), FeatureStruct.New().Value).OrderBy(s => s, StringComparer.Ordinal)));
foreach (string word in File.ReadAllLines(args[1]))
{
    var parses = morpher.ParseWord(word).ToArray();
    Console.WriteLine(word + "\t" + parses.Length + "\t" + SignatureFormat.BuildSignature(parses));
}
