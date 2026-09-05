using System.IO;
using Newtonsoft.Json.Linq;

namespace XampleProjector
{
	internal sealed class GeneratedFile
	{
		internal string AbsolutePath { get; }
		internal string RelativePath { get; }
		internal long Bytes { get; }
		internal string Sha256Hex { get; }
		internal bool Deterministic { get; }

		private GeneratedFile(string absolutePath, string relativePath, long bytes, string sha256Hex, bool deterministic)
		{
			AbsolutePath = absolutePath;
			RelativePath = relativePath;
			Bytes = bytes;
			Sha256Hex = sha256Hex;
			Deterministic = deterministic;
		}

		/// <summary>
		/// Describes a file already written under <paramref name="outDir"/>. The JSON "path" is
		/// relative to <paramref name="outDir"/> -- a captured response.json must never leak the
		/// machine-specific absolute temp path it happened to be produced under.
		/// </summary>
		internal static GeneratedFile Describe(string outDir, string absolutePath, bool deterministic = true)
		{
			var bytes = File.ReadAllBytes(absolutePath);
			var relativePath = PathUtil.MakeRelative(outDir, absolutePath);
			return new GeneratedFile(absolutePath, relativePath, bytes.LongLength, Sha256.OfBytes(bytes), deterministic);
		}

		internal JObject ToJson()
		{
			return new JObject
			{
				["path"] = RelativePath,
				["sha256"] = Sha256Hex,
				["bytes"] = Bytes,
				["deterministic"] = Deterministic,
			};
		}
	}
}
