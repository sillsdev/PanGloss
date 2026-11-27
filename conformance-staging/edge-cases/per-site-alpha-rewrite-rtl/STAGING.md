# Per-site rewrite subrule feeding, right to left

This mirrors the left-to-right staged fixture, reversing the rewrite direction,
environments and roots. The founding C# driver at Machine
`4d9b454d42a0da35e058641a65596339a79d115b` accepts `SUBRULE|iyu` and the
committed negative controls; its replay is recorded in
`fst-stage1-rtl-oracle.log` under the production run's logs. This is an HC-Rust
port defect. The fixture remains staged pending an upstream conformance
contribution.
