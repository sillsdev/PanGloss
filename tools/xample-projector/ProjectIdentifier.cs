// Adapted from FieldWorks Src\GenerateHCConfig\ProjectIdentifier.cs.
using System;
using SIL.LCModel;

namespace XampleProjector
{
	internal class ProjectIdentifier : IProjectIdentifier
	{
		private readonly BackendProviderType m_backendProviderType;

		public ProjectIdentifier(string projectPath)
		{
			Path = System.IO.Path.GetFullPath(projectPath);
			string ext = System.IO.Path.GetExtension(Path);
			switch (ext.ToLowerInvariant())
			{
				case LcmFileHelper.ksFwDataXmlFileExtension:
					m_backendProviderType = BackendProviderType.kXML;
					break;
			}
		}

		public bool IsLocal => true;

		public string Path { get; set; }

		public string ProjectFolder => System.IO.Path.GetDirectoryName(Path);

		public string SharedProjectFolder => ProjectFolder;

		public string ServerName => null;

		public string Handle => Name;

		public string PipeHandle => throw new NotImplementedException();

		public string Name => System.IO.Path.GetFileNameWithoutExtension(Path);

		public BackendProviderType Type => m_backendProviderType;

		public string UiName => Name;
	}
}
