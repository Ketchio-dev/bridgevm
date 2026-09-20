use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Args)]
pub(crate) struct AppSnapshotExportArgs {
    pub(crate) id: String,
    #[arg(value_name = "OUTPUT", value_parser = absolute_export)]
    pub(crate) output: PathBuf,
}

fn absolute_export(value: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("OUTPUT requires an absolute path without '..'".into());
    }
    if path.parent().is_none() {
        return Err("OUTPUT must name an export directory".into());
    }
    Ok(path)
}

impl AppSnapshotExportArgs {
    pub(crate) fn into_native_arguments(self) -> Vec<OsString> {
        [
            OsString::from("--cli"),
            OsString::from("snapshot-export"),
            OsString::from(self.id),
            self.output.into_os_string(),
        ]
        .into()
    }
}
