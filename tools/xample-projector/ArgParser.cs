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
	}
}
