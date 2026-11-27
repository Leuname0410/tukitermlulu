use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn bundle_config() -> serde_json::Value {
    let path = manifest_dir().join("tauri.conf.json");
    let contents = std::fs::read_to_string(&path).expect("tauri.conf.json debe existir");
    let config: serde_json::Value =
        serde_json::from_str(&contents).expect("tauri.conf.json debe ser JSON válido");
    config
        .get("bundle")
        .expect("debe existir la sección bundle")
        .clone()
}

fn field<'a>(config: &'a serde_json::Value, path: &[&str]) -> &'a serde_json::Value {
    let mut current = config;
    for key in path {
        current = current
            .get(key)
            .unwrap_or_else(|| panic!("falta el campo {}", key));
    }
    current
}

#[test]
fn should_expose_bundle_metadata_when_reading_tauri_config() {
    let bundle = bundle_config();

    assert_eq!(field(&bundle, &["active"]), &serde_json::json!(true));
    assert_eq!(
        field(&bundle, &["targets"]),
        &serde_json::json!(["nsis", "msi", "appimage", "deb", "dmg", "app"])
    );
    assert_eq!(field(&bundle, &["license"]), "MPL-2.0");
    assert_eq!(field(&bundle, &["category"]), "DeveloperTool");
    assert_eq!(field(&bundle, &["publisher"]), "tukitermlulu");
    assert_eq!(
        field(&bundle, &["homepage"]),
        "https://github.com/Leuname0410/tukitermlulu"
    );

    let short_description = field(&bundle, &["shortDescription"])
        .as_str()
        .expect("shortDescription debe ser texto");
    assert!(!short_description.is_empty());
    assert!(short_description.len() <= 80);
    assert!(
        !field(&bundle, &["longDescription"])
            .as_str()
            .expect("longDescription debe ser texto")
            .is_empty()
    );
    assert!(
        field(&bundle, &["copyright"])
            .as_str()
            .expect("copyright debe ser texto")
            .contains("2026")
    );

    let license_file = field(&bundle, &["licenseFile"])
        .as_str()
        .expect("licenseFile debe ser texto");
    assert!(
        manifest_dir().join(license_file).is_file(),
        "licenseFile debe existir en disco"
    );

    assert_eq!(
        field(&bundle, &["windows", "nsis", "installMode"]),
        "currentUser"
    );
    assert_eq!(
        field(&bundle, &["windows", "webviewInstallMode", "type"]),
        "downloadBootstrapper"
    );
    assert_eq!(
        field(&bundle, &["linux", "deb", "depends"]),
        &serde_json::json!([])
    );
    assert_eq!(field(&bundle, &["macOS", "minimumSystemVersion"]), "10.15");
    assert!(bundle.get("updater").is_none());
    assert!(
        field(&bundle, &["icon"])
            .as_array()
            .expect("icon debe ser una lista")
            .len()
            >= 5
    );
}
