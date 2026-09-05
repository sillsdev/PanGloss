using System;
using System.Collections.Generic;

namespace XampleProjector
{
	/// <summary>
	/// The one guarded-insert this repo's id/label uniqueness checks share: register `key` in `seen`,
	/// refusing loudly (never silently overwriting or ignoring) if it is already taken by a different
	/// value. <see cref="GrammarParser"/> uses this to prove every grammar.xml id/Name is
	/// document-global-unique before authoring; <see cref="CheckLabelUniquenessCommand"/> uses the
	/// SAME method to prove a content-derived guid label map is injective, rather than re-deriving an
	/// equivalent-looking second uniqueness check.
	/// </summary>
	internal static class IdRegistry
	{
		internal static void Register(Dictionary<string, string> seen, string key, string value, Func<string, string> messageForCollision)
		{
			if (key == null)
				return;
			if (seen.TryGetValue(key, out var existing))
				throw new GrammarAuthorException(messageForCollision(existing));
			seen[key] = value;
		}
	}
}
