using System.IO;
using System.Security.Cryptography;
using System.Text;

namespace XampleProjector
{
	internal static class Sha256
	{
		internal static string OfFile(string path)
		{
			using (var sha = SHA256.Create())
			using (var stream = File.OpenRead(path))
			{
				var hash = sha.ComputeHash(stream);
				return ToHex(hash);
			}
		}

		internal static string OfBytes(byte[] bytes)
		{
			using (var sha = SHA256.Create())
				return ToHex(sha.ComputeHash(bytes));
		}

		private static string ToHex(byte[] hash)
		{
			var sb = new StringBuilder(hash.Length * 2);
			foreach (var b in hash)
				sb.Append(b.ToString("x2"));
			return sb.ToString();
		}
	}
}
