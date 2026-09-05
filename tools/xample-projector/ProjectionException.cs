using System;

namespace XampleProjector
{
	/// <summary>Anything that stops HC or XAMPLE projection from producing a complete result.</summary>
	internal sealed class ProjectionException : Exception
	{
		internal ProjectionException(string message) : base(message)
		{
		}

		internal ProjectionException(string message, Exception inner) : base(message, inner)
		{
		}
	}
}
