//! Build provenance embedded in the executable.

pub(crate) fn embedded_build_info() -> &'static str {
    env!("PANGLOSS_BUILD_INFO")
}
