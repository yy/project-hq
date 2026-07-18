use project_hq::commands::run_init;
use project_hq::config::Config;
use project_hq::load_all;

#[test]
fn init_seeds_a_valid_hq_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let hq_dir = tmp.path().join("HQ");

    let created = run_init(&hq_dir).expect("init should succeed on a fresh directory");
    assert!(!created.is_empty());
    assert!(hq_dir.join("projects/welcome-to-hq.md").is_file());
    assert!(hq_dir.join("README.md").is_file());

    let config = Config::load(&hq_dir);
    assert_eq!(config.tracks, vec!["classes", "life", "projects"]);

    let projects = load_all(&hq_dir, &config);
    assert_eq!(projects.len(), 3);
    assert!(projects.iter().any(|p| p.title.starts_with("Welcome")));
}

#[test]
fn init_refuses_an_existing_hq_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let hq_dir = tmp.path().to_path_buf();

    run_init(&hq_dir).expect("first init should succeed");
    let error = run_init(&hq_dir).expect_err("second init should refuse");
    assert!(error
        .to_string()
        .contains("already looks like an HQ directory"));
}

#[test]
fn init_tolerates_an_empty_existing_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let hq_dir = tmp.path().join("HQ");
    std::fs::create_dir_all(hq_dir.join("classes")).unwrap();

    run_init(&hq_dir).expect("init should succeed when track dirs exist but are empty");
    assert!(hq_dir.join("classes/example-class.md").is_file());
}
