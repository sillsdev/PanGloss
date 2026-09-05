using System;

namespace XampleProjector
{
	/// <summary>
	/// A fixture construct outside the subset `author` supports. Always names the offending
	/// element/attribute and its fixture id -- a control that cannot represent a construct must
	/// say so, never silently skip or approximate it.
	/// </summary>
	internal sealed class GrammarAuthorException : Exception
	{
		internal GrammarAuthorException(string message) : base(message)
		{
		}
	}
}
