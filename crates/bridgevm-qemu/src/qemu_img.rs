//! QemuImgCommand constructors for create, backed-create, info, check and convert.

use serde::Deserialize;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QemuImgCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl QemuImgCommand {
    pub fn create_disk(path: &Path, format: impl Into<String>, size: impl Into<String>) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "create".to_string(),
                "-f".to_string(),
                format.into(),
                path.display().to_string(),
                size.into(),
            ],
        }
    }

    pub fn create_backed_disk(
        path: &Path,
        format: impl Into<String>,
        backing_format: impl Into<String>,
        backing_file: &Path,
    ) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "create".to_string(),
                "-f".to_string(),
                format.into(),
                "-F".to_string(),
                backing_format.into(),
                "-b".to_string(),
                backing_file.display().to_string(),
                path.display().to_string(),
            ],
        }
    }

    pub fn info_json(path: &Path) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "info".to_string(),
                "--output=json".to_string(),
                path.display().to_string(),
            ],
        }
    }

    pub fn check_json(path: &Path) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "check".to_string(),
                "--output=json".to_string(),
                path.display().to_string(),
            ],
        }
    }

    pub fn convert_compact(source: &Path, output: &Path, format: impl Into<String>) -> Self {
        Self {
            program: "qemu-img".to_string(),
            args: vec![
                "convert".to_string(),
                "-O".to_string(),
                format.into(),
                source.display().to_string(),
                output.display().to_string(),
            ],
        }
    }

    pub fn render_shell_words(&self) -> Vec<String> {
        std::iter::once(self.program.clone())
            .chain(self.args.iter().cloned())
            .collect()
    }
}
