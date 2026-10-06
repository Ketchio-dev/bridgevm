//! Inspect owned media without making native raw imports depend on QEMU.

use crate::import_paths::invalid;
use crate::*;
use serde_json::Value;
use std::path::Path;

pub(crate) fn command(args: Vec<String>) -> Result<Value, StorageError> {
    let output = run_command("qemu-img", &args)?;
    if !output.status.success() {
        return Err(invalid(format!(
            "import qemu-img {} failed: {}",
            args[0],
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    if output.stdout.is_empty() {
        return Ok(Value::Null);
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn unsupported(value: &Value) -> bool {
    match value {
        Value::Object(values) => values.iter().any(|(key, value)| {
            matches!(key.as_str(), "data-file" | "data-file-raw" | "encrypt")
                || (key == "encrypted" && value != &Value::Bool(false))
                || unsupported(value)
        }),
        Value::Array(values) => values.iter().any(unsupported),
        _ => false,
    }
}

pub(crate) fn info(path: &Path, format: &str) -> Result<Value, StorageError> {
    if format == "raw" {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() {
            return Err(invalid("imported raw media is not an owned regular file"));
        }
        return Ok(serde_json::json!({"format": "raw", "virtual-size": metadata.len()}));
    }
    let value = command(vec![
        "info".into(),
        "-f".into(),
        format.into(),
        "--output=json".into(),
        path.display().to_string(),
    ])?;
    if value["format"] != format || unsupported(&value) || value["virtual-size"].as_u64().is_none()
    {
        return Err(invalid(
            "unsupported encrypted, external-data or malformed imported image",
        ));
    }
    Ok(value)
}
