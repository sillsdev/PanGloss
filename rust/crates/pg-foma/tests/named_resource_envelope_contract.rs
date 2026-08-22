use std::str::FromStr;

use pg_foma::resource_envelope::{CompileEnvelopeRequest, ResourceEnvelope, ResourceEnvelopeId};
use sha2::{Digest, Sha256};

#[test]
fn shipped_resource_envelopes_are_closed_complete_and_canonical() {
    assert_eq!(
        ResourceEnvelopeId::all(),
        &[
            ResourceEnvelopeId::ManagedV1,
            ResourceEnvelopeId::TunedSurfaceWork10kV1,
        ]
    );
    assert_eq!(ResourceEnvelopeId::ManagedV1.as_str(), "managed-v1");
    assert_eq!(
        ResourceEnvelopeId::TunedSurfaceWork10kV1.as_str(),
        "tuned-surface-work-10k-v1"
    );
    assert_eq!(
        ResourceEnvelopeId::from_str("managed-v1").unwrap(),
        ResourceEnvelopeId::ManagedV1
    );
    assert!(ResourceEnvelopeId::from_str("3000").is_err());
    assert!(ResourceEnvelopeId::from_str("10000").is_err());
    assert!(ResourceEnvelopeId::from_str("HC_TUNED_CLOSURE_WORK_LIMIT").is_err());

    let managed = ResourceEnvelope::for_id(ResourceEnvelopeId::ManagedV1);
    assert_eq!(managed.schema_version(), 1);
    assert_eq!(managed.worker_protocol_version(), 1);
    assert_eq!(managed.watchdog().wall_timeout_ms, 120_000);
    assert_eq!(managed.watchdog().rss_limit_mb, 4_096);
    assert_eq!(managed.watchdog().rss_sample_interval_ms, 200);
    assert_eq!(managed.communication().max_request_bytes, 4 * 1024 * 1024);
    assert_eq!(managed.communication().max_result_bytes, 16 * 1024 * 1024);
    assert_eq!(
        managed.communication().max_captured_stderr_bytes,
        4 * 1024 * 1024
    );
    assert_eq!(managed.compose().state_cap, 2_000_000);
    assert_eq!(managed.compose().arc_cap, 20_000_000);
    assert_eq!(managed.compose().tuple_cap, 5_000);
    assert_eq!(managed.compose().group_cap, 64);
    assert_eq!(managed.compose().line_cap, 1_000_000);
    assert_eq!(managed.compose().compound_pair_cap, 4_000_000);
    assert_eq!(managed.compose().chain_depth_cap, None);
    assert_eq!(managed.compose().ordering_multiplicity_cap, Some(100));
    assert_eq!(managed.enumeration().composite_entry_cap, 200_000);
    assert_eq!(managed.enumeration().pair_probe_cap, 3_000_000);
    assert_eq!(managed.backend().tuned_surface_closure_work_cap, 3_000);
    assert_eq!(managed.backend().tuned_surface_closure_depth_cap, 64);
    assert_eq!(
        managed.backend().tuned_surface_compound_chain_depth_cap,
        200
    );

    let retry = ResourceEnvelope::for_id(ResourceEnvelopeId::TunedSurfaceWork10kV1);
    assert_eq!(retry.backend().tuned_surface_closure_work_cap, 10_000);
    assert_eq!(retry.backend().tuned_surface_closure_depth_cap, 64);
    assert_eq!(retry.backend().tuned_surface_compound_chain_depth_cap, 200);
    assert_eq!(retry.watchdog(), managed.watchdog());
    assert_eq!(retry.communication(), managed.communication());
    assert_eq!(retry.compose(), managed.compose());
    assert_eq!(retry.enumeration(), managed.enumeration());

    let canonical = managed.canonical_json();
    assert_eq!(
        canonical,
        ResourceEnvelope::for_id(managed.id()).canonical_json()
    );
    assert_eq!(
        managed.digest(),
        format!("{:x}", Sha256::digest(canonical.as_bytes()))
    );
    assert_ne!(managed.digest(), retry.digest());

    let decoded: ResourceEnvelope =
        serde_json::from_str(&canonical).expect("canonical closed envelope must deserialize");
    assert_eq!(decoded, managed);
    let forged = canonical.replacen(
        "\"tuned_surface_closure_work_cap\":3000",
        "\"tuned_surface_closure_work_cap\":2999",
        1,
    );
    assert_ne!(forged, canonical);
    assert!(serde_json::from_str::<ResourceEnvelope>(&forged).is_err());
}

#[test]
fn named_requests_have_private_distinct_attempt_identity() {
    let first = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("a named envelope starts one attempt");
    let independent = CompileEnvelopeRequest::try_new(ResourceEnvelopeId::ManagedV1)
        .expect("a second request starts an independent attempt");
    assert_eq!(first.envelope_id(), ResourceEnvelopeId::ManagedV1);
    assert_eq!(first.retry_of(), None);
    assert_eq!(first.prior_closure(), None);
    assert_ne!(
        first.attempt_id(), independent.attempt_id(),
        "independent attempts need distinct immutable identities"
    );
}
