# Recipe elimination report

The grammar admits the `complete-template` family, but the current executable baseline contains no `Union` node whose children can be permuted. The registered complete-template transform therefore content-addresses to the default Plan and is eliminated as `content-address-duplicate`.

This is a structural impossibility for the current semantics-preserving V1 transform set, not an untested alternative. If a future materializer introduces another semantics-preserving complete-template topology, this fixture test must change from the single-candidate branch to require and build that distinct Plan.
