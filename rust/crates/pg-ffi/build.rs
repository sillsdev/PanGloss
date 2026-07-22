fn main() {
    println!("cargo:rerun-if-changed=include/pangloss.h");
    println!("cargo:rerun-if-changed=tests/header_smoke.c");
    println!("cargo:rerun-if-changed=tests/header_smoke.cpp");
    cc::Build::new()
        .file("tests/header_smoke.c")
        .include("include")
        .warnings_into_errors(true)
        .compile("pangloss_header_c_smoke");
    cc::Build::new()
        .cpp(true)
        .file("tests/header_smoke.cpp")
        .include("include")
        .warnings_into_errors(true)
        .compile("pangloss_header_cpp_smoke");
}
