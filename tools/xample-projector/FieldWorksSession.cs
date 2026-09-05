using System;
using System.IO;
using SIL.FieldWorks.Common.FwUtils;
using SIL.LCModel;
using SIL.LCModel.Utils;
using SIL.WritingSystems;

namespace XampleProjector
{
	/// <summary>
	/// The headless open/close bootstrap, mirroring FieldWorks Src\GenerateHCConfig\Program.cs:
	/// FwRegistryHelper.Initialize -> FwUtils.InitializeIcu -> Sldr.Initialize -> open the
	/// LcmCache with DisableDataMigration so an out-of-date project fails loudly instead of
	/// silently migrating under an ad-hoc tool.
	/// </summary>
	internal static class FieldWorksSession
	{
		internal static int Run(string projectPath, Func<LcmCache, DiagnosticLogger, int> body)
		{
			if (!File.Exists(projectPath))
			{
				Console.Error.WriteLine("Project open failure: file not found: {0}", projectPath);
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
			var progress = new NullThreadedProgress(synchronizeInvoke);

			try
			{
				using (var cache = LcmCache.CreateCacheFromExistingData(projectId, "en", logger, dirs, settings, progress))
				{
					return body(cache, logger);
				}
			}
			catch (LcmFileLockedException ex)
			{
				Console.Error.WriteLine("Project open failure: the project is open in another application ({0}).", ex.Message);
				return ExitCodes.ProjectOpenFailure;
			}
			catch (LcmDataMigrationForbiddenException ex)
			{
				Console.Error.WriteLine("Project open failure: the project needs FLEx migration before this tool can open it ({0}).", ex.Message);
				return ExitCodes.ProjectOpenFailure;
			}
		}
	}
}
