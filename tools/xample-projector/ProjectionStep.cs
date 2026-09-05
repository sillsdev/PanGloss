using System;

namespace XampleProjector
{
	/// <summary>
	/// Every failure inside HC or XAMPLE projection must surface as a ProjectionException naming
	/// the step that failed, never a raw exception -- a caller needs "which step" to diagnose a
	/// bad output directory, a missing transform, or a GAFAWS mismatch, and ProjectCommand maps
	/// ProjectionException to the documented exit 5. Shared by ProjectCommand (HCLoader.Load,
	/// XmlLanguageWriter.Save) and XampleProjection (M3 export, each XSL transform, GAFAWS).
	/// </summary>
	internal static class ProjectionStep
	{
		internal static void Run(string stepName, Action action)
		{
			Run<object>(stepName, () => { action(); return null; });
		}

		internal static T Run<T>(string stepName, Func<T> func)
		{
			try
			{
				return func();
			}
			catch (ProjectionException)
			{
				throw;
			}
			catch (Exception ex)
			{
				throw new ProjectionException($"{stepName} failed: {ex.Message}", ex);
			}
		}
	}
}
