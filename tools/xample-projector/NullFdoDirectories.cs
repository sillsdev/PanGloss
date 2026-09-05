// Adapted from FieldWorks Src\GenerateHCConfig\NullFdoDirectories.cs.
using SIL.LCModel;

namespace XampleProjector
{
	internal class NullFdoDirectories : ILcmDirectories
	{
		public string ProjectsDirectory => null;

		public string TemplateDirectory => null;
	}
}
