using System;

namespace XampleProjector
{
	/// <summary>
	/// Every reason `mutate` stops names a stable dotted code and carries the exit code the
	/// caller must report (9 = refused, this request/target shape cannot proceed; 10 = integrity
	/// failure, the source or the materialized copy did not verify) -- a control that cannot act
	/// must say so, never delete partially or guess.
	/// </summary>
	internal sealed class MutationException : Exception
	{
		internal int ExitCode { get; }
		internal string Code { get; }

		internal MutationException(int exitCode, string code, string detail) : base($"{code}: {detail}")
		{
			ExitCode = exitCode;
			Code = code;
		}
	}
}
