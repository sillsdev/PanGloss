using SIL.FieldWorks.Common.FwUtils;
using SIL.LCModel.Utils;
using SIL.WritingSystems;

namespace XampleProjector
{
	/// <summary>
	/// FwRegistryHelper.Initialize/FwUtils.InitializeIcu/Sldr.Initialize are process-global,
	/// one-shot calls -- Sldr.Initialize itself throws InvalidOperationException on a second call
	/// in the same process. Every command that opens more than one LcmCache in a single run
	/// (mutate: the clone, then a second open to reopen-and-verify) must go through this instead
	/// of calling them directly, or the second open crashes. Discovered live: every prior command
	/// opened at most one cache per process, so this was never exercised before mutate.
	/// </summary>
	internal static class FieldWorksBootstrap
	{
		private static bool s_initialized;

		internal static void EnsureInitialized()
		{
			if (s_initialized)
				return;
			FwRegistryHelper.Initialize();
			FwUtils.InitializeIcu();
			Sldr.Initialize();
			s_initialized = true;
		}
	}
}
