// Known-count workload for pmc-setup.ps1 -Verify: the loop body is exactly `dec` and `jnz`, so N iterations retire 2N instructions plus a fixed process overhead.
// Two lengths cancel the overhead; their counted difference must match 2 * (N2 - N1).
#[cfg(target_arch = "x86_64")]
fn main() {
    use std::arch::asm;
    let n: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("usage: known_loop <iterations>");
    let mut i = n;
    unsafe {
        asm!("2:", "dec {i}", "jnz 2b", i = inout(reg) i, options(nomem, nostack));
    }
    std::process::exit(i as i32);
}

#[cfg(not(target_arch = "x86_64"))]
compile_error!("known_loop has only an x86_64 form; pmc-setup.ps1 -Verify refuses other architectures");
