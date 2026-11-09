use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use jsonschema::Validator;
use serde_json::{Value, json};

use crate::Result;

// Decision sources: Define the SpielOrders package topology; Define the Order record and lifecycle contract;
// Define Vendor identity and historical semantics; Define durable metadata schemas and version evolution.
pub(crate) fn check(root: &Path) -> Result<()> {
    let contracts = root.join("contracts");
    let mut validators = BTreeMap::new();
    for artifact in ["version", "order", "vendors", "index"] {
        let schema = read_json(&contracts.join(format!("schemas/v1/{artifact}.schema.json")))?;
        ensure(
            schema["$schema"] == "https://json-schema.org/draft/2020-12/schema",
            artifact,
        )?;
        validators.insert(artifact, jsonschema::draft202012::options().build(&schema)?);
    }
    for example in ["minimal", "full"] {
        for (artifact, validator) in &validators {
            let value =
                read_json(&contracts.join(format!("examples/v1/{example}/{artifact}.json")))?;
            expect_valid(validator, &value, true, &format!("{example}/{artifact}"))?;
        }
    }
    check_input_boundaries(&contracts, &validators)?;
    let rejected = read_json(&contracts.join("fixtures/schema-invalid/manifest.json"))?;
    for fixture in array(&rejected)? {
        let name = text(&fixture["name"])?;
        let artifact = text(&fixture["artifact"])?;
        let validator = validators
            .get(artifact)
            .ok_or_else(|| format!("{name}: unknown artifact {artifact}"))?;
        let value = read_json(
            &contracts
                .join("fixtures/schema-invalid")
                .join(text(&fixture["file"])?),
        )?;
        let keyword = text(&fixture["keyword"])?;
        ensure(
            validator
                .iter_errors(&value)
                .any(|error| error.schema_path().as_str().rsplit('/').next() == Some(keyword)),
            format!("{name}: did not reject for {keyword}"),
        )?;
    }
    check_scenarios(&contracts)?;
    let a = fs::read(contracts.join("fixtures/assets/supporting-a.bin"))?;
    let b = fs::read(contracts.join("fixtures/assets/supporting-b.bin"))?;
    ensure(a != b, "Supporting Material sentinels must differ")?;
    println!("schema and scenario checks passed");
    Ok(())
}

fn check_input_boundaries(contracts: &Path, validators: &BTreeMap<&str, Validator>) -> Result<()> {
    let order = read_json(&contracts.join("examples/v1/minimal/order.json"))?;
    let validator = &validators["order"];
    for (locator, valid) in [("..", false), ("../..", true), ("../../..", true)] {
        let mut value = order.clone();
        value["package_root"] = json!(locator);
        expect_valid(validator, &value, valid, "package_root depth")?;
    }
    for field in ["package_root", "items"] {
        let mut value = order.clone();
        value
            .as_object_mut()
            .ok_or("Order must be an object")?
            .remove(field);
        expect_valid(validator, &value, false, field)?;
    }
    for (field, replacement) in [
        ("unknown", json!(true)),
        ("status", json!("draft")),
        ("order_id", json!("not-a-uuid")),
        ("currency", json!("usd")),
        ("currency", json!("US")),
        ("date_ordered", json!("07/19/2026")),
        ("vendor_id", Value::Null),
    ] {
        let mut value = order.clone();
        value[field] = replacement;
        expect_valid(validator, &value, false, field)?;
    }
    let vendor = json!({"vendor_id": "e0990d31-06aa-42e9-97c8-088e3e894822", "name": "Vendor", "contacts": []});
    let mut missing_contacts = vendor.clone();
    missing_contacts
        .as_object_mut()
        .ok_or("Vendor must be an object")?
        .remove("contacts");
    expect_valid(
        &validators["vendors"],
        &json!({"schema_version": 1, "vendors": [missing_contacts]}),
        false,
        "missing contacts",
    )?;
    expect_valid(
        &validators["vendors"],
        &json!({"schema_version": 1, "vendors": [vendor.clone()]}),
        true,
        "absent visibility",
    )?;
    for (field, visibility, valid) in [
        ("hidden", json!(true), true),
        ("hidden", json!(false), true),
        ("hidden", Value::Null, false),
        ("hidden", json!("true"), false),
        ("visible", json!(true), false),
    ] {
        let mut value = vendor.clone();
        value[field] = visibility;
        expect_valid(
            &validators["vendors"],
            &json!({"schema_version": 1, "vendors": [value]}),
            valid,
            "Vendor visibility",
        )?;
    }
    let index = read_json(&contracts.join("examples/v1/full/index.json"))?;
    for (visibility, valid) in [
        (json!(true), true),
        (json!("true"), false),
        (Value::Null, false),
    ] {
        let mut value = index.clone();
        value["vendors"][0]["hidden"] = visibility;
        expect_valid(
            &validators["index"],
            &value,
            valid,
            "indexed Vendor visibility",
        )?;
    }
    Ok(())
}

fn check_scenarios(contracts: &Path) -> Result<()> {
    let index = read_json(&contracts.join("fixtures/scenarios.json"))?;
    let mut sources = BTreeSet::new();
    for source in array(&index["decision_sources"])? {
        let name = text(&source["name"])?;
        ensure(
            sources.insert(name),
            format!("duplicate decision source {name}"),
        )?;
        ensure(
            contracts.join(text(&source["path"])?).try_exists()?,
            format!("missing decision source {name}"),
        )?;
    }
    ensure(!sources.is_empty(), "scenario decision sources are empty")?;
    let mut names = BTreeSet::new();
    for scenario in array(&index["scenarios"])? {
        let name = text(&scenario["name"])?;
        ensure(names.insert(name), format!("duplicate scenario {name}"))?;
        for field in [
            "condition",
            "schema_validity",
            "semantic_validity",
            "integrity_scope",
            "warning_state",
        ] {
            text(&scenario[field])?;
        }
        array(&scenario["preservation"])?;
        let references = array(&scenario["sources"])?;
        ensure(
            !references.is_empty(),
            format!("{name}: no decision sources"),
        )?;
        for source in references {
            ensure(
                sources.contains(text(source)?),
                format!("{name}: unknown decision source"),
            )?;
        }
        for entry in array(&scenario["tree"])? {
            if let Some(source) = entry.get("source") {
                ensure(
                    contracts.join(text(source)?).try_exists()?,
                    format!("{name}: missing fixture source {source}"),
                )?;
            }
        }
    }
    ensure(!names.is_empty(), "scenario index is empty")
}

fn expect_valid(validator: &Validator, value: &Value, valid: bool, name: &str) -> Result<()> {
    ensure(
        validator.is_valid(value) == valid,
        format!(
            "{name}: unexpected schema validity; {}",
            validator
                .iter_errors(value)
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ),
    )
}

fn read_json(path: &Path) -> Result<Value> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    Ok(serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))?)
}

fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("expected nonblank text, got {value}").into())
}

fn array(value: &Value) -> Result<&Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| format!("expected array, got {value}").into())
}

fn ensure(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into().into())
    }
}
