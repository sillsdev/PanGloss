using Newtonsoft.Json;
using Newtonsoft.Json.Linq;

namespace XampleProjector
{
	/// <summary>The pinned schema version for every JSON response this tool writes.</summary>
	internal static class SchemaVersion
	{
		internal const int Current = 1;
	}

	/// <summary>
	/// Field names shared by every response mode. Kept as constants rather than typed DTOs
	/// because the two producing modes (inspect, project) emit disjoint shapes and the
	/// validator only needs to check field presence and value shape, not round-trip a type.
	/// </summary>
	internal static class Fields
	{
		internal const string SchemaVersion = "schemaVersion";
		internal const string Mode = "mode";
		internal const string FieldWorksVersion = "fieldWorksVersion";
		internal const string AssemblyVersions = "assemblyVersions";
		internal const string SourcePath = "sourcePath";
		internal const string SourceSha256 = "sourceSha256";
		internal const string ProjectName = "projectName";
		internal const string ActiveParser = "activeParser";
		internal const string Phonemes = "phonemes";
		internal const string BoundaryMarkers = "boundaryMarkers";
		internal const string NaturalClasses = "naturalClasses";
		internal const string Diagnostics = "diagnostics";
		internal const string Database = "database";
		internal const string Generated = "generated";
		internal const string HcLoadDiagnostics = "hcLoadDiagnostics";
	}

	internal static class JsonWriter
	{
		internal static void WriteFile(string path, JObject document)
		{
			System.IO.File.WriteAllText(path, document.ToString(Formatting.Indented));
		}
	}
}
