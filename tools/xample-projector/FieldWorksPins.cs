using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Reflection;

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

		/// <summary>
		/// Verifies the assemblies actually loaded into THIS process -- not merely what sits on
		/// disk under <paramref name="fieldWorksDir"/> -- both loaded from the pinned directory
		/// and at the pinned version. A disk-only check (<see cref="Verify"/>) cannot catch the
		/// .NET assembly-resolve order picking up a same-named DLL from somewhere else on the
		/// search path; this makes the pin check about what RAN. Call only after code that
		/// forces these assemblies to load (e.g. right after LcmCache opens) -- calling it any
		/// earlier would report every assembly as "not loaded".
		/// </summary>
		internal static IReadOnlyList<PinMismatch> VerifyLoaded(string fieldWorksDir)
		{
			var mismatches = new List<PinMismatch>();
			CheckLoadedAssembly(mismatches, fieldWorksDir, "ParserCore.dll",
				typeof(SIL.FieldWorks.WordWorks.Parser.HCLoader).Assembly);
			CheckLoadedAssembly(mismatches, fieldWorksDir, "SIL.LCModel.dll",
				typeof(SIL.LCModel.LcmCache).Assembly);
			CheckLoadedAssembly(mismatches, fieldWorksDir, "SIL.Machine.Morphology.HermitCrab.dll",
				typeof(SIL.Machine.Morphology.HermitCrab.Language).Assembly);
			CheckLoadedAssembly(mismatches, fieldWorksDir, "SIL.Machine.dll",
				typeof(SIL.Machine.Annotations.Annotation<>).Assembly);
			return mismatches;
		}

		private static void CheckLoadedAssembly(List<PinMismatch> mismatches, string fieldWorksDir, string pinnedFileName, Assembly loaded)
		{
			var expectedPath = Path.GetFullPath(Path.Combine(fieldWorksDir, pinnedFileName));
			var loadedPath = Path.GetFullPath(loaded.Location);
			if (!string.Equals(expectedPath, loadedPath, StringComparison.OrdinalIgnoreCase))
			{
				mismatches.Add(new PinMismatch(pinnedFileName, expectedPath, loadedPath));
				return;
			}

			var expectedVersion = ExpectedFileVersions[pinnedFileName];
			var loadedVersion = FileVersionInfo.GetVersionInfo(loadedPath).FileVersion;
			if (loadedVersion != expectedVersion)
				mismatches.Add(new PinMismatch(pinnedFileName, expectedVersion, loadedVersion));
		}
	}
}
