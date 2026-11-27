This adapter exports full ordered XML morpheme and allomorph keys, root position,
category and duplicate analyses from the pinned C# HermitCrab. It does not change
or rebuild the oracle. The private loader maps are inspected deliberately: the
public `Morpheme.Id` is a display ID and can be null. A missing loader map or key
fails the adapter rather than guessing an identity.

Build with .NET 10, supplying the directory of assemblies built from the exact
`machine` gitlink, and direct build output/intermediate files to scratch:

```
dotnet build rust/tools/fst-strategy-oracle -c Release -m:1 \
  -p:MachineAssemblyDirectory=<pin-matched-hc-directory> \
  -p:BaseIntermediateOutputPath=<scratch>/obj/ -o <scratch>/bin
```

Run `dotnet <scratch>/bin/fst-strategy-oracle.dll <grammar.xml> <words.txt>
<out.jsonl> <machine-pin>`. The caller verifies the assembly provenance; the output
records the pin and hashes of both inputs. STARTED rows distinguish a hung word
from an empty parse. Run heavy measurements sequentially with `ulimit -v 8388608`
and `timeout 3600`. On Linux set `DOTNET_GCHeapHardLimit=0x40000000` so CoreCLR can
start under the virtual-address limit. This is an operational envelope, not a
linguistic bound.

Compare using `python3 rust/tools/fst_differential.py <grammar-pass.jsonl>
--csharp <out.jsonl> --out <comparison.json>`. The comparison rejects mismatched
pins, hashes, occurrences and duplicate IDs, and reports missing/incomplete rows.
FWData input has no XML export in this runner; its differential names HC-Rust as
the reference and never claims a C# replay.
