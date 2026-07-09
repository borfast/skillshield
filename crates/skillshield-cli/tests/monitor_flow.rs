use skillshield_core::baseline::Baseline;
use skillshield_core::catalog::Catalog;
use skillshield_core::config::ScanConfig;
use skillshield_core::discovery::discover;
use std::fs;

#[test]
fn monitor_root_picks_up_project_agents_md() {
    let tmp = tempfile::tempdir().unwrap();
    let proj = tmp.path().join("proj");
    fs::create_dir_all(&proj).unwrap();
    fs::write(proj.join("AGENTS.md"), "rules").unwrap();

    let catalog = Catalog::builtin();
    let cfg = ScanConfig {
        project_roots: vec![proj.to_string_lossy().to_string()],
        ..ScanConfig::default()
    };
    let scan = discover(&catalog, &cfg);
    let baseline = Baseline::new(scan.entries);
    assert!(baseline.contains_path(&proj.join("AGENTS.md")));
}
