use super::*;

pub(super) fn native_id_arguments(verb: &str, id: Option<String>) -> Vec<OsString> {
    let mut result = vec![OsString::from("--cli"), verb.into()];
    if let Some(id) = id {
        result.push(id.into());
    }
    result
}

impl AppCreateWindowsArgs {
    pub(super) fn into_native_arguments(self) -> Vec<OsString> {
        let mut result = vec![
            "--cli".into(),
            "create-windows".into(),
            self.name.into(),
            "--iso".into(),
            self.iso.into_os_string(),
            "--disk-gib".into(),
            self.disk_gib.to_string().into(),
            "--cpus".into(),
            self.cpus.to_string().into(),
            "--resolution".into(),
            self.resolution.into(),
        ];
        // Without --memory-mib the app picks the size for this Mac, as its Create sheet does.
        if let Some(memory_mib) = self.memory_mib {
            result.extend(["--memory-mib".into(), memory_mib.to_string().into()]);
        }
        if self.no_network {
            result.push("--no-network".into());
        }
        if self.fetch_drivers {
            result.push("--fetch-drivers".into());
        }
        result
    }
}

#[cfg(test)]
#[path = "app_cli_create_tests.rs"]
mod tests;
