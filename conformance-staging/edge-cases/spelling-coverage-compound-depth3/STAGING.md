# Recursive compound spelling at 3 stems

The two rules grow the head at opposite ends. Each inserts the FieldWorks
compound boundary, with three spellings in the character table.
One named case uses three distinct roots with a prefix on the middle root.
The two-letter neighbors through MaxRoots + 1 test multiplicity and the stem cap.

The pinned C# driver supplied the signatures and rule traces. The backend's
[identity receipt](../../../rust/crates/pg-foma-backend/tests/fixtures/recursive-compound-depth3.json)
comes from the same Machine assemblies through Morpher.AnalyzeWord. It records
the ordered morphemes, head index, category, and occurrence count for each word.
The MaxRoots setting and recursive rule application are FieldWorks settings.

Upstream PR pending.
