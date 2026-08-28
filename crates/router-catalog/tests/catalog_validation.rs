use router_catalog::Catalog;
use router_types::{ModelId, SkillId};

#[test]
fn rejects_bare_model_alias_in_catalog() {
    let bytes = include_bytes!("../../../tests/fixtures/catalogs/invalid-alias.json");
    let error = Catalog::from_json(bytes).unwrap_err();
    assert_eq!(error.code(), "catalog.model.non_canonical_id");
}

#[test]
fn rejects_duplicate_model_ids() {
    let bytes = include_bytes!("../../../tests/fixtures/catalogs/duplicate-model.json");
    let error = Catalog::from_json(bytes).unwrap_err();
    assert_eq!(error.code(), "catalog.model.duplicate_id");
}

#[test]
fn loads_exact_codex_models() {
    let bytes = include_bytes!("../../../assets/catalogs/codex-core-v1.json");
    let catalog = Catalog::from_json(bytes).unwrap();
    assert!(
        catalog
            .model(&ModelId::parse("gpt-5.6-sol").unwrap())
            .is_some()
    );
    assert!(
        catalog
            .model(&ModelId::parse("gpt-5.6-terra").unwrap())
            .is_some()
    );
    assert!(
        catalog
            .model(&ModelId::parse("gpt-5.6-luna").unwrap())
            .is_some()
    );
}

#[test]
fn exposes_catalog_version_and_builtin_skill_metadata() {
    let bytes = include_bytes!("../../../assets/catalogs/codex-core-v1.json");
    let catalog = Catalog::from_json(bytes).unwrap();

    assert_eq!(catalog.version(), "codex-core-v1");
    assert!(catalog.skill(&SkillId::parse("tdd").unwrap()).is_some());
}

#[test]
fn rejects_invalid_catalog_semantics_with_specific_codes() {
    for (field, value, expected_code) in [
        ("schema_version", "2", "catalog.schema.unsupported_version"),
        ("version", "\"\"", "catalog.version.empty"),
        ("models[0].id", "\"\"", "catalog.model.invalid_id"),
        ("models[0].stages", "[]", "catalog.model.empty_stages"),
        ("models[0].efforts", "[]", "catalog.model.empty_efforts"),
        (
            "models[0].quality_micros",
            "1000001",
            "catalog.model.quality_out_of_range",
        ),
        ("models[0].cost_micros", "-1", "catalog.model.negative_cost"),
        (
            "models[0].latency_micros",
            "-1",
            "catalog.model.negative_latency",
        ),
        (
            "skills[0].trust",
            "\"unknown\"",
            "catalog.skill.invalid_trust",
        ),
        ("skills[0].id", "\"\"", "catalog.skill.invalid_id"),
    ] {
        let bytes = invalid_catalog(field, value);
        let error = Catalog::from_json(bytes.as_bytes()).unwrap_err();
        assert_eq!(error.code(), expected_code, "field {field}");
    }
}

fn invalid_catalog(field: &str, value: &str) -> String {
    let catalog = std::str::from_utf8(include_bytes!(
        "../../../assets/catalogs/codex-core-v1.json"
    ))
    .unwrap();
    let mut parsed: serde_json::Value = serde_json::from_str(catalog).unwrap();
    let replacement: serde_json::Value = serde_json::from_str(value).unwrap();

    match field {
        "schema_version" | "version" => parsed[field] = replacement,
        "models[0].id"
        | "models[0].stages"
        | "models[0].efforts"
        | "models[0].quality_micros"
        | "models[0].cost_micros"
        | "models[0].latency_micros" => {
            let field = field.rsplit('.').next().unwrap();
            parsed["models"][0][field] = replacement;
        }
        "skills[0].trust" | "skills[0].id" => {
            let field = field.rsplit('.').next().unwrap();
            parsed["skills"][0][field] = replacement;
        }
        _ => unreachable!("test only supplies supported catalog paths"),
    }

    serde_json::to_string(&parsed).unwrap()
}

#[test]
fn rejects_duplicate_skill_ids() {
    let bytes = include_bytes!("../../../assets/catalogs/codex-core-v1.json");
    let mut catalog: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let duplicate = catalog["skills"][0].clone();
    catalog["skills"].as_array_mut().unwrap().push(duplicate);

    let bytes = serde_json::to_vec(&catalog).unwrap();
    let error = Catalog::from_json(&bytes).unwrap_err();
    assert_eq!(error.code(), "catalog.skill.duplicate_id");
}
