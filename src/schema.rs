// SPDX-License-Identifier: GPL-3.0-or-later

//! JSON Schemas of the parameter file and of `walkit info --json`, made from the types that
//! read and write them. The repository keeps a copy of each in `schemas/`; a test checks the
//! copies against these.

use schemars::{schema_for, JsonSchema};
use serde_json::{json, Value};

use crate::info::Info;
use crate::params::Params;

/// Where the repository keeps a schema: `name` is `params` or `info`.
pub fn url(name: &str) -> String {
    format!("https://raw.githubusercontent.com/mattijsf/walkit-cli/main/schemas/{name}.schema.json")
}

/// The schema of the parameter file. A file may name its schema in `$schema`.
pub fn params() -> String {
    let mut schema = make::<Params>("params");
    if let Some(properties) = schema["properties"].as_object_mut() {
        properties.insert(
            "$schema".into(),
            json!({
                "description": "The schema the file follows; walkit reads past it.",
                "type": "string"
            }),
        );
    }
    text(&schema)
}

/// The schema of what `walkit info --json` prints for one skin; for more it prints an array
/// of them.
pub fn info() -> String {
    text(&make::<Info>("info"))
}

fn make<T: JsonSchema>(name: &str) -> Value {
    let schema = schema_for!(T).to_value();
    let Value::Object(fields) = schema else {
        return schema;
    };
    // $schema and $id first, then the rest as schemars writes it
    let mut out = serde_json::Map::new();
    if let Some(version) = fields.get("$schema") {
        out.insert("$schema".into(), version.clone());
    }
    out.insert("$id".into(), url(name).into());
    out.extend(fields.into_iter().filter(|(k, _)| k != "$schema"));
    Value::Object(out)
}

fn text(schema: &Value) -> String {
    let mut text = serde_json::to_string_pretty(schema).expect("a schema serializes");
    text.push('\n');
    text
}
