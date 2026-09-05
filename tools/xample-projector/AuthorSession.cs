using System;
using System.IO;
using SIL.FieldWorks.Common.FwUtils;
using SIL.LCModel;
using SIL.LCModel.Utils;
using SIL.WritingSystems;

namespace XampleProjector
{
	/// <summary>
	/// The headless "create a brand-new blank project" bootstrap -- the authoring counterpart to
	/// <see cref="FieldWorksSession"/>'s "open an existing project". See
	/// docs research (liblcm-authoring.md): CreateCacheWithNewBlankLangProj writes the empty
	/// .fwdata skeleton and locks it immediately, so a caller must not create the target path twice.
	/// </summary>
	internal static class AuthorSession
	{
		internal static int Run(string fieldWorksDir, string projectPath, string analysisWs, string vernacularWs, Func<LcmCache, DiagnosticLogger, int> body)
		{
			if (File.Exists(projectPath))
			{
				Console.Error.WriteLine("Project open failure: target already exists: {0}", projectPath);
				return ExitCodes.ProjectOpenFailure;
			}

			FwRegistryHelper.Initialize();
			FwUtils.InitializeIcu();
			Sldr.Initialize();

			var synchronizeInvoke = new SingleThreadedSynchronizeInvoke();
			var projectId = new ProjectIdentifier(projectPath);
			var logger = new DiagnosticLogger(synchronizeInvoke);
			var dirs = new NullFdoDirectories();
			var settings = new LcmSettings { DisableDataMigration = true };

			using (var cache = LcmCache.CreateCacheWithNewBlankLangProj(projectId, analysisWs, vernacularWs, analysisWs, logger, dirs, settings))
			{
				var loadedMismatches = FieldWorksPins.VerifyLoaded(fieldWorksDir);
				if (loadedMismatches.Count > 0)
				{
					Console.Error.WriteLine("A FieldWorks assembly actually loaded by this process does not match the pinned build:");
					foreach (var mismatch in loadedMismatches)
						Console.Error.WriteLine("  {0}: expected {1}, actually loaded {2}", mismatch.FileName, mismatch.Expected, mismatch.Actual);
					return ExitCodes.PinMismatch;
				}

				return body(cache, logger);
			}
		}
	}
}
