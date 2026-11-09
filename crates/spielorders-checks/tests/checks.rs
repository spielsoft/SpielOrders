use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Repository(PathBuf);

impl Repository {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "spielorders-checks-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn write(&self, path: &str, contents: &str) {
        let destination = self.0.join(path);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, contents).unwrap();
    }

    fn check(&self, kind: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_spielorders-checks"))
            .arg(kind)
            .current_dir(&self.0)
            .output()
            .unwrap()
    }

    fn with_contracts() -> Self {
        let repository = Self::new();
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        for directory in ["contracts", "issues/wayfinder"] {
            copy_tree(&source.join(directory), &repository.0.join(directory));
        }
        repository
    }
}

fn copy_tree(source: &std::path::Path, destination: &std::path::Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn documentation_checks_resolve_real_links_and_reject_missing_targets() {
    let repository = Repository::new();
    repository.write("README.md", "[Guide](docs/guide.md)\n");
    repository.write(
        "docs/guide.md",
        "[Root](../README.md) [Web](https://example.com/)\n",
    );
    repository.write(
        "issues/wayfinder/spielorders-product-architecture/tickets/decision.md",
        "---\ntitle: A current decision\nstatus: closed\n---\n",
    );
    let valid = repository.check("docs");
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    fs::remove_file(repository.0.join("docs/guide.md")).unwrap();
    let invalid = repository.check("docs");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("docs/guide.md"));

    repository.write("docs/guide.md", "Current guide\n");
    repository.write(
        "issues/wayfinder/spielorders-product-architecture/tickets/decision.md",
        "---\ntitle: An unresolved decision\nstatus: open\n---\n",
    );
    let unresolved = repository.check("docs");
    assert!(!unresolved.status.success());
    assert!(String::from_utf8_lossy(&unresolved.stderr).contains("not closed"));
}

#[test]
fn schemas_check_the_corpus_and_reject_invalid_accepted_examples() {
    let repository = Repository::with_contracts();
    let valid = repository.check("schemas");
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let example = "contracts/examples/v1/minimal/order.json";
    let mut order: serde_json::Value =
        serde_json::from_slice(&fs::read(repository.0.join(example)).unwrap()).unwrap();
    order["title"] = serde_json::json!(42);
    repository.write(example, &order.to_string());
    let invalid = repository.check("schemas");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("minimal/order"));
}

#[test]
fn schema_checks_require_recorded_rejections_and_preservation_evidence() {
    let repository = Repository::with_contracts();
    let fixture = "contracts/fixtures/schema-invalid/order-unknown-field.json";
    let original = fs::read_to_string(repository.0.join(fixture)).unwrap();
    let accepted = fs::read_to_string(
        repository
            .0
            .join("contracts/examples/v1/minimal/order.json"),
    )
    .unwrap();
    repository.write(fixture, &accepted);
    let no_rejection = repository.check("schemas");
    assert!(!no_rejection.status.success());
    assert!(String::from_utf8_lossy(&no_rejection.stderr).contains("order unknown field"));
    repository.write(fixture, &original);

    let manifest_path = "contracts/fixtures/schema-invalid/manifest.json";
    let manifest_bytes = fs::read_to_string(repository.0.join(manifest_path)).unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&manifest_bytes).unwrap();
    manifest[0]["keyword"] = serde_json::json!("not-a-validation-keyword");
    repository.write(manifest_path, &manifest.to_string());
    let wrong_rejection = repository.check("schemas");
    assert!(!wrong_rejection.status.success());
    assert!(String::from_utf8_lossy(&wrong_rejection.stderr).contains("not-a-validation-keyword"));
    repository.write(manifest_path, &manifest_bytes);

    let scenario_path = "contracts/fixtures/scenarios.json";
    let scenario_bytes = fs::read_to_string(repository.0.join(scenario_path)).unwrap();
    let mut scenarios: serde_json::Value = serde_json::from_str(&scenario_bytes).unwrap();
    scenarios["scenarios"][1]["tree"][1]["source"] = serde_json::json!("fixtures/missing.bin");
    repository.write(scenario_path, &scenarios.to_string());
    let missing_asset = repository.check("schemas");
    assert!(!missing_asset.status.success());
    assert!(String::from_utf8_lossy(&missing_asset.stderr).contains("fixtures/missing.bin"));
    repository.write(scenario_path, &scenario_bytes);

    let first = fs::read(
        repository
            .0
            .join("contracts/fixtures/assets/supporting-a.bin"),
    )
    .unwrap();
    fs::write(
        repository
            .0
            .join("contracts/fixtures/assets/supporting-b.bin"),
        first,
    )
    .unwrap();
    let identical_sentinels = repository.check("schemas");
    assert!(!identical_sentinels.status.success());
    assert!(String::from_utf8_lossy(&identical_sentinels.stderr).contains("sentinels must differ"));
}
