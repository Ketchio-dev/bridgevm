//! Read-only diagnostics for native app executable discovery.

use crate::*;
use std::os::unix::process::CommandExt;

pub(super) fn run_or_forward(args: AppArgs) -> Result<()> {
    if !matches!(&args.command, AppCommand::Doctor) {
        let executable = app_cli_resolver::resolve()?;
        let error = ProcessCommand::new(&executable)
            .args(super::arguments::native_arguments(args))
            .exec();
        return Err(error).with_context(|| {
            format!("could not execute native app CLI: {}", executable.display())
        });
    }
    run(args.json, args.library.as_deref())
}

fn run(json: bool, library: Option<&Path>) -> Result<()> {
    let diagnosis = app_cli_resolver::diagnose()?;
    if json {
        println!("{}", json_report(&diagnosis, library));
    } else {
        print!("{}", text_report(&diagnosis, library));
    }
    if diagnosis.selected.is_none() {
        bail!("native app discovery is blocked; see the diagnostic report above");
    }
    Ok(())
}

fn json_report(
    diagnosis: &app_cli_resolver::DiscoveryDiagnosis,
    library: Option<&Path>,
) -> serde_json::Value {
    let candidates = diagnosis
        .candidates
        .iter()
        .map(|candidate| {
            serde_json::json!({
                "path": candidate.path,
                "status": candidate.status,
                "selected": candidate.selected,
                "detail": candidate.detail,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "schema": "bridgevm.app-doctor.v1",
        "ready": diagnosis.selected.is_some(),
        "selectedExecutable": diagnosis.selected,
        "requestedLibrary": library,
        "libraryChecked": false,
        "signatureVerified": false,
        "guestHealthChecked": false,
        "candidates": candidates,
    })
}

fn text_report(diagnosis: &app_cli_resolver::DiscoveryDiagnosis, library: Option<&Path>) -> String {
    let mut output = String::new();
    output.push_str(if diagnosis.selected.is_some() {
        "Native app discovery: READY\n"
    } else {
        "Native app discovery: BLOCKED\n"
    });
    if let Some(path) = &diagnosis.selected {
        output.push_str(&format!("Selected executable: {}\n", path.display()));
    }
    output.push_str("Candidates:\n");
    for candidate in &diagnosis.candidates {
        let selected = if candidate.selected {
            " (selected)"
        } else {
            ""
        };
        output.push_str(&format!(
            "  [{}] {}{}\n",
            candidate.status,
            candidate.path.display(),
            selected
        ));
        if let Some(detail) = &candidate.detail {
            output.push_str(&format!("      {detail}\n"));
        }
    }
    if let Some(path) = library {
        output.push_str(&format!(
            "Requested library: {} (not inspected)\n",
            path.display()
        ));
    }
    output.push_str(
        "Scope: CLI protocol marker only; code signature, library contents, and guest health were not checked.\n",
    );
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_cli_resolver::diagnostics::CandidateDiagnosis;

    fn diagnosis(selected: bool) -> app_cli_resolver::DiscoveryDiagnosis {
        let path = PathBuf::from("/Applications/BridgeVM.app/Contents/MacOS/BridgeVMControl");
        app_cli_resolver::DiscoveryDiagnosis {
            candidates: vec![CandidateDiagnosis {
                path: path.clone(),
                status: if selected {
                    "compatible"
                } else {
                    "incompatible"
                },
                detail: (!selected).then(|| "protocol marker missing".to_string()),
                selected,
            }],
            selected: selected.then_some(path),
        }
    }

    #[test]
    fn json_is_versioned_and_discloses_unchecked_scopes() {
        let value = json_report(&diagnosis(true), Some(Path::new("/VM Library")));
        assert_eq!(value["schema"], "bridgevm.app-doctor.v1");
        assert_eq!(value["ready"], true);
        assert_eq!(value["libraryChecked"], false);
        assert_eq!(value["signatureVerified"], false);
        assert_eq!(value["guestHealthChecked"], false);
        assert_eq!(value["requestedLibrary"], "/VM Library");
        assert_eq!(value["candidates"][0]["selected"], true);
    }

    #[test]
    fn text_reports_blocker_without_claiming_broader_health() {
        let report = text_report(&diagnosis(false), None);
        assert!(report.contains("Native app discovery: BLOCKED"));
        assert!(report.contains("[incompatible]"));
        assert!(report.contains("protocol marker missing"));
        assert!(report.contains("guest health were not checked"));
    }
}
