using System.Collections.Generic;
using System.Diagnostics;
using System.IO;

namespace XampleProjector
{
	/// <summary>
	/// The FieldWorks 9.3.10 (net48, x64) file versions this tool was built and verified
	/// against. A mismatch means the install underneath has drifted from what was verified,
	/// so startup refuses rather than silently projecting against an unverified FieldWorks build.
	/// </summary>
	internal static class FieldWorksPins
	{
		internal static readonly IReadOnlyDictionary<string, string> ExpectedFileVersions = new Dictionary<string, string>
		{
			["ParserCore.dll"] = "9.3.10.26161",
			["SIL.LCModel.dll"] = "11.0.0.55167",
			["SIL.Machine.dll"] = "3.8.2.0",
			["SIL.Machine.Morphology.HermitCrab.dll"] = "3.7.4.0",
			["XAmpleManagedWrapper.dll"] = "9.3.10.26161",
			["xample64.dll"] = "3.12.23.21",
		};

		internal readonly struct PinMismatch
		{
			internal string FileName { get; }
			internal string Expected { get; }
			internal string Actual { get; }

			internal PinMismatch(string fileName, string expected, string actual)
			{
				FileName = fileName;
				Expected = expected;
				Actual = actual;
			}
		}

		/// <summary>
		/// Checks file versions directly from disk (never loads assemblies), so this works
		/// uniformly for the managed pins and the native xample64.dll pin.
		/// </summary>
		internal static IReadOnlyList<PinMismatch> Verify(string fieldWorksDir)
		{
			var mismatches = new List<PinMismatch>();
			foreach (var pin in ExpectedFileVersions)
			{
				var path = Path.Combine(fieldWorksDir, pin.Key);
				if (!File.Exists(path))
				{
					mismatches.Add(new PinMismatch(pin.Key, pin.Value, "(missing)"));
					continue;
				}

				var actual = FileVersionInfo.GetVersionInfo(path).FileVersion;
				if (actual != pin.Value)
					mismatches.Add(new PinMismatch(pin.Key, pin.Value, actual));
			}

			return mismatches;
		}
	}
}
