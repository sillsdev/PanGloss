# Simultaneous overlap expansion

Regression for sillsdev/PanGloss#199: a simultaneous `aa → iii` rule on `aaaa` must not accept the overlapping-site result `iiiaiii`. The three rejection expectations were verified against the founding C# oracle at Machine 4d9b454d42a0da35e058641a65596339a79d115b; the supplied oracle receipt is `code-review/probe-d741df3f/oracle-final.log` in the lane workspace. C# synthesis produces `iiiaiii` from `aaaa`, but its analysis rejects that same surface; the expected rejections record the observed analysis behavior. The `aa` short-root control generates and parses `iii` as `SHORT|iii` in the same pinned C# driver. This is an HC-Rust analysis port defect.

Upstream conformance PR: pending.
