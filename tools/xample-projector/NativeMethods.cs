using System.Runtime.InteropServices;

namespace XampleProjector
{
	/// <summary>
	/// Widens the process DLL search path to the FieldWorks install directory before any
	/// P/Invoke happens, so native dependencies (ICU, and in a later slice xample.dll)
	/// resolve without copying them next to this tool's own exe.
	/// </summary>
	internal static class NativeMethods
	{
		[DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
		internal static extern bool SetDllDirectory(string lpPathName);
	}
}
