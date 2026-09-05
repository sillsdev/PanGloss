using System.Collections.Generic;

namespace XampleProjector
{
	internal static class ArgParser
	{
		internal static bool TryGetOption(string[] args, string name, out string value)
		{
			for (int i = 1; i < args.Length - 1; i++)
			{
				if (args[i] == name)
				{
					value = args[i + 1];
					return true;
				}
			}
			value = null;
			return false;
		}

		/// <summary>Every value passed for a repeatable flag (e.g. <c>--expect a --expect b</c>), in order.</summary>
		internal static List<string> GetOptions(string[] args, string name)
		{
			var result = new List<string>();
			for (int i = 1; i < args.Length - 1; i++)
			{
				if (args[i] == name)
					result.Add(args[i + 1]);
			}
			return result;
		}
	}
}
