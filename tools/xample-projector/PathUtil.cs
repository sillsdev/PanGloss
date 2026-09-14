using System;
using System.IO;

namespace XampleProjector
{
	internal static class PathUtil
	{
		/// <summary>
		/// A relative path from <paramref name="basePath"/> to <paramref name="targetPath"/>
		/// (net48 has no Path.GetRelativePath). Falls back to the absolute target if the two
		/// roots cannot be related as a file:// URI (e.g. different drive on a system where
		/// that occurs) -- callers that reject an absolute result will see that fallback and
		/// fail loudly rather than silently accept an unrelativizable path.
		/// </summary>
		internal static string MakeRelative(string basePath, string targetPath)
		{
			var baseUri = new Uri(AppendDirectorySeparator(Path.GetFullPath(basePath)));
			var targetUri = new Uri(Path.GetFullPath(targetPath));
			var relativeUri = baseUri.MakeRelativeUri(targetUri);
			var relative = Uri.UnescapeDataString(relativeUri.ToString());
			return relative.Replace('/', Path.DirectorySeparatorChar);
		}

		private static string AppendDirectorySeparator(string path)
		{
			if (path.EndsWith(Path.DirectorySeparatorChar.ToString(), StringComparison.Ordinal) ||
				path.EndsWith(Path.AltDirectorySeparatorChar.ToString(), StringComparison.Ordinal))
				return path;
			return path + Path.DirectorySeparatorChar;
		}
	}
}
