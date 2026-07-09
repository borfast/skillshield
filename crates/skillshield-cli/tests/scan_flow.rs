// End-to-end: build a baseline, mutate, confirm `status` logic via core.
use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::{Catalog, MatchSpec, Rule, Scope};
use skillshield_core::config::ScanConfig;
use skillshield_core::diff::{diff, ChangeKind};
use skillshield_core::discovery::discover;
use std::fs;

fn catalog_for(dir: &std::path::Path) -> Catalog {
    Catalog {
        rules: vec![Rule {
            id: "t".into(),
            description: "".into(),
            spec: MatchSpec::DirFileSet(format!("{}/", dir.display())),
            scope: Scope::Global,
        }],
    }
}

#[test]
fn detects_new_file_after_baseline() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("skills");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.md"), "one").unwrap();

    let cat = catalog_for(&dir);
    let cfg = ScanConfig::default();
    let baseline = Baseline::new(discover(&cat, &cfg).entries);

    // A new file lands after baselining.
    fs::write(dir.join("evil.md"), "payload").unwrap();
    let scan2 = discover(&cat, &cfg);
    let d = diff(&baseline, &scan2);

    assert_eq!(d.findings.len(), 1);
    assert_eq!(d.findings[0].change, ChangeKind::Added);
    assert!(d.findings[0].path.ends_with("evil.md"));
}
