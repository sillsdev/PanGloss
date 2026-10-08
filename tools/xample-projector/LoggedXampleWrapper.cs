using System;
using XAmpleManagedWrapper;

namespace XampleProjector
{
	/// <summary>Installs the native log after LoadFiles resets the setup, before it reads controls.</summary>
	internal sealed class LoggedXampleWrapper : XAmpleWrapper
	{
		internal void LoadWithLog(string fixedDir, string dynamicDir, string database, string log)
		{
			var original = m_xample.AmpleSetParameterDelegate;
			var installed = false;
			m_xample.AmpleSetParameterDelegate = (setup, name, value) =>
			{
				if (!installed)
				{
					installed = true;
					var result = original(setup, "LogFile", log);
					if (result.IndexOf("none", StringComparison.OrdinalIgnoreCase) < 0)
						throw new InvalidOperationException("XAMPLE could not install native log: " + result);
				}
				return original(setup, name, value);
			};
			try { LoadFiles(fixedDir, dynamicDir, database); }
			finally { m_xample.AmpleSetParameterDelegate = original; }
			if (!installed)
				throw new InvalidOperationException("XAMPLE LoadFiles never consulted the native parameter delegate; log not installed");
		}
	}
}
