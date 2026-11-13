// SPDX-License-Identifier: GPL-3.0-or-later

//! The schemas in `schemas/`: up to date with the code, and true of what walkit reads and
//! writes.

use std::process::Command;

use serde_json::Value;
use walkit_cli::schema;

const CLASSIC: &str = "tests/skins/walkit-classic.wsz";
const MODERN: &str = "tests/skins/walkit-modern.wal";

#[test]
fn the_schema_files_are_up_to_date() {
    for (name, made) in [("params", schema::params()), ("info", schema::info())] {
        let path = format!("schemas/{name}.schema.json");
        let kept = std::fs::read_to_string(&path)
            .unwrap()
            .replace("\r\n", "\n");
        assert!(
            kept == made,
            "{path} is out of date: run `cargo run -- schema {name} > {path}`"
        );
    }
}

fn validator(text: &str) -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(text).unwrap();
    jsonschema::validator_for(&schema).expect("a valid schema")
}

fn check(validator: &jsonschema::Validator, value: &Value) {
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn the_museum_parameters_follow_the_schema() {
    let validator = validator(&schema::params());
    let museum: Value = serde_json::from_str(include_str!("../params/museum.json")).unwrap();
    check(&validator, &museum);
    // a field the tool does not know is refused by both
    let wrong: Value = serde_json::json!({ "output": { "sclae": 2 } });
    assert!(!validator.is_valid(&wrong));
}

#[test]
fn what_info_prints_follows_the_schema() {
    let validator = validator(&schema::info());
    let result = Command::new(env!("CARGO_BIN_EXE_walkit"))
        .args(["info", "--json", CLASSIC, MODERN])
        .output()
        .unwrap();
    assert!(result.status.success());
    let infos: Value = serde_json::from_slice(&result.stdout).unwrap();
    for info in infos.as_array().unwrap() {
        check(&validator, info);
    }
}
