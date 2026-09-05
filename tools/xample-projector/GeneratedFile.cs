using System.IO;
using Newtonsoft.Json.Linq;

namespace XampleProjector
{
	internal sealed class GeneratedFile
	{
		internal string Path { get; }
		internal long Bytes { get; }
		internal string Sha256Hex { get; }

		private GeneratedFile(string path, long bytes, string sha256Hex)
		{
			Path = path;
			Bytes = bytes;
			Sha256Hex = sha256Hex;
		}

		internal static GeneratedFile Describe(string path)
		{
			var bytes = File.ReadAllBytes(path);
			return new GeneratedFile(path, bytes.LongLength, Sha256.OfBytes(bytes));
		}

		internal JObject ToJson()
		{
			return new JObject
			{
				["path"] = Path,
				["sha256"] = Sha256Hex,
				["bytes"] = Bytes,
			};
		}
	}
}
