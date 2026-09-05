// Adapted from FieldWorks Src\GenerateHCConfig\ConsoleLogger.cs (ILcmUI + IHCLoadErrorLogger stub).
// Extended beyond the original to also collect entries as structured (kind, message) pairs,
// since the caller needs them in the tool's JSON response, not just on stdout.
using System;
using System.Collections.Generic;
using System.ComponentModel;
using SIL.FieldWorks.WordWorks.Parser;
using SIL.LCModel;

namespace XampleProjector
{
	internal readonly struct Diagnostic
	{
		internal string Kind { get; }
		internal string Message { get; }

		internal Diagnostic(string kind, string message)
		{
			Kind = kind;
			Message = message;
		}
	}

	internal class DiagnosticLogger : ILcmUI, IHCLoadErrorLogger
	{
		private readonly List<Diagnostic> m_diagnostics = new List<Diagnostic>();

		internal DiagnosticLogger(ISynchronizeInvoke synchronizeInvoke)
		{
			SynchronizeInvoke = synchronizeInvoke;
		}

		internal IReadOnlyList<Diagnostic> Diagnostics => m_diagnostics;

		private void Record(string kind, string message)
		{
			m_diagnostics.Add(new Diagnostic(kind, message));
			Console.WriteLine("[{0}] {1}", kind, message);
		}

		public ISynchronizeInvoke SynchronizeInvoke { get; }

		public bool ConflictingSave() => throw new NotImplementedException();

		public DateTime LastActivityTime => DateTime.Now;

		public FileSelection ChooseFilesToUse() => throw new NotImplementedException();

		public bool RestoreLinkedFilesInProjectFolder() => throw new NotImplementedException();

		public YesNoCancel CannotRestoreLinkedFilesToOriginalLocation() => throw new NotImplementedException();

		public void DisplayMessage(MessageType type, string message, string caption, string helpTopic)
		{
			Record(type.ToString(), message);
		}

		public void ReportException(Exception error, bool isLethal)
		{
			Record("Exception", error.Message);
		}

		public void ReportDuplicateGuids(string errorText)
		{
			Record("DuplicateGuids", errorText);
		}

		public void DisplayCircularRefBreakerReport(string msg, string caption)
		{
			Record(caption, msg);
		}

		public bool Retry(string msg, string caption) => throw new NotImplementedException();

		public bool OfferToRestore(string projectPath, string backupPath) => throw new NotImplementedException();

		public void InvalidShape(string str, int errorPos, IMoMorphSynAnalysis msa)
		{
			Record("InvalidShape", string.Format("The form \"{0}\" contains an undefined phoneme at {1}.", str, errorPos));
		}

		public void InvalidAffixProcess(IMoAffixProcess affixProcess, bool isInvalidLhs, IMoMorphSynAnalysis msa)
		{
			Record("InvalidAffixProcess", string.Format("The affix process \"{0}\" is invalid.", affixProcess.Form.BestVernacularAlternative.Text));
		}

		public void InvalidPhoneme(IPhPhoneme phoneme)
		{
			Record("InvalidPhoneme", string.Format("The phoneme \"{0}\" does not contain any valid graphemes.", phoneme.Name.BestAnalysisVernacularAlternative.Text));
		}

		public void DuplicateGrapheme(IPhPhoneme phoneme)
		{
			Record("DuplicateGrapheme", string.Format("The phoneme \"{0}\" has the same grapheme as another phoneme.", phoneme.Name.BestAnalysisVernacularAlternative.Text));
		}

		public void InvalidEnvironment(IMoForm form, IPhEnvironment env, string reason, IMoMorphSynAnalysis msa)
		{
			Record("InvalidEnvironment", string.Format("The environment \"{0}\" is invalid. Reason: {1}", env.StringRepresentation.Text, reason));
		}

		public void InvalidReduplicationForm(IMoForm form, string reason, IMoMorphSynAnalysis msa)
		{
			Record("InvalidReduplicationForm", string.Format("The reduplication form \"{0}\" is invalid. Reason: {1}", form.Form.VernacularDefaultWritingSystem.Text, reason));
		}

		public void InvalidRewriteRule(IPhRegularRule rule, string reason)
		{
			Record("InvalidRewriteRule", string.Format("The rewrite rule \"{0}\" is invalid. Reason: {1}", rule.Name.BestAnalysisVernacularAlternative.Text, reason));
		}

		public void InvalidStrata(string strata, string reason)
		{
			Record("InvalidStrata", reason);
		}

		public void OutOfScopeSlot(IMoInflAffixSlot slot, IMoInflAffixTemplate template, string reason)
		{
			Record("OutOfScopeSlot", reason);
		}

		void IHCLoadErrorLogger.UnmatchedReduplicationIndexedClass(IMoForm form, string reason, string environment)
		{
			Record("UnmatchedReduplicationIndexedClass", reason);
		}
	}
}
