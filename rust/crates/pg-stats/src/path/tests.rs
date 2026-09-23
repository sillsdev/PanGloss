use super::*;

#[test]
fn digest_is_stable_and_path_dependent() {
    assert_eq!(hex_digest(b"a"), hex_digest(b"a"));
    assert_ne!(hex_digest(b"a"), hex_digest(b"b"));
}

#[test]
fn default_cache_dir_is_not_beside_the_fwdata_file() {
    let dir = crate::test_support::TempDir::new("pg-stats-path");
    let fwdata = dir.path().join("project.fwdata");
    std::fs::write(&fwdata, b"stub").unwrap();
    let root = dir.path().join("localappdata");

    let cache_dir = cache_dir_under(&fwdata, root.clone()).unwrap();
    assert!(cache_dir.starts_with(&root));
    assert_ne!(cache_dir.parent().unwrap(), fwdata.parent().unwrap());
    assert!(cache_dir.to_string_lossy().contains("PanGloss"));
    assert!(cache_dir.to_string_lossy().contains("stats"));
}

#[test]
fn same_path_produces_same_dir_twice() {
    let dir = crate::test_support::TempDir::new("pg-stats-path-2");
    let fwdata = dir.path().join("project.fwdata");
    std::fs::write(&fwdata, b"stub").unwrap();
    let root = dir.path().join("localappdata");

    let a = cache_dir_under(&fwdata, root.clone()).unwrap();
    let b = cache_dir_under(&fwdata, root).unwrap();
    assert_eq!(a, b);
}
