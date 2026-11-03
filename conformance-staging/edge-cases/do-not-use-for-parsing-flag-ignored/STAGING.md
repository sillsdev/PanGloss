# do-not-use-for-parsing-flag-ignored

Synthetic. Pins that an entry flagged `DoNotUseForParsing` parses like an unflagged one (sillsdev/pangloss#154, divergence entry 121). FieldWorks' parsers never read the flag: it is absent from ParserCore, HCLoader and XAmple, and Machine never references it.

## What it pins

`grammar.xml` and `words.yaml` are byte-identical to `adhoc-prohibition-allomorph-variant-control` (the unflagged grammar, recorded from hc.dll at Machine `aab4c59819adb7b366915e09df2fb28c62b8c03a`: la/li 2 each, tala 1, tali 0). `fieldworks/project.fwdata` is that fixture's LibLCM-authored project with `DoNotUseForParsing` flipped from `False` to `True` on two entries, the `ta-` prefix entry and the `la` main entry (the checkbox LibLCM's `LexEntry.DoNotUseForParsing` stores). Importing it must give the same analysis counts as the unflagged grammar. A PanGloss that drops flagged entries loses `la`, `tala` and the variant copies.

## Oracle and limits

The recorded signatures come from the unflagged `grammar.xml` on hc.dll; HC-XML carries no flag, so that replay cannot see the flag. The project was edited by flipping the stored value, not re-authored with `tools/xample-projector author` (the tool needs a Windows FieldWorks build and has no flag option); it was not re-projected through HCLoader here. The verdict that FieldWorks ignores the flag rests on real FieldWorks runs (HermitCrab and XAMPLE, flagged and unflagged, same result) in `docs/divergences/evidence/121/`, and on the source search in divergence entry 121. `fieldworks/phonology-mutations.yaml` is omitted because its `base_sha256` names the unflagged project.

Upstream PR: none yet (candidate for sillsdev/fieldworks-parser-conformance).
