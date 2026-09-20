//! Structured inspection of every bounded native app candidate.

use super::*;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CandidateDiagnosis {
    pub(crate) path: PathBuf,
    pub(crate) status: &'static str,
    pub(crate) detail: Option<String>,
    pub(crate) selected: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DiscoveryDiagnosis {
    pub(crate) candidates: Vec<CandidateDiagnosis>,
    pub(crate) selected: Option<PathBuf>,
}

pub(crate) fn diagnose() -> Result<DiscoveryDiagnosis> {
    if !cfg!(target_os = "macos") {
        bail!("native app commands require macOS and a compatible BridgeVM app");
    }
    let executable = env::current_exe()
        .context("could not locate the current bridgevm executable")?
        .canonicalize()
        .context("could not resolve the current bridgevm executable")?;
    Ok(diagnose_candidates(&candidates(
        &executable,
        account_home().as_deref(),
    )))
}

fn diagnose_candidates(paths: &[PathBuf]) -> DiscoveryDiagnosis {
    let mut selection_open = true;
    let mut selected = None;
    let candidates = paths
        .iter()
        .map(|path| {
            let (status, detail, is_present) = inspect(path);
            let is_selected = selection_open && is_present && status == "compatible";
            if selection_open && is_present {
                selection_open = false;
                if is_selected {
                    selected = Some(path.clone());
                }
            }
            CandidateDiagnosis {
                path: path.clone(),
                status,
                detail,
                selected: is_selected,
            }
        })
        .collect();
    DiscoveryDiagnosis {
        candidates,
        selected,
    }
}

fn inspect(path: &Path) -> (&'static str, Option<String>, bool) {
    match fs::symlink_metadata(path) {
        Ok(metadata) if !metadata.file_type().is_file() => (
            "invalid",
            Some("native app executable is not a regular file".to_string()),
            true,
        ),
        Ok(_) => match validate_marker(path) {
            Ok(()) => ("compatible", None, true),
            Err(error) => ("incompatible", Some(format!("{error:#}")), true),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ("missing", None, false),
        Err(error) => ("unreadable", Some(error.to_string()), true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::slice::from_ref;

    #[test]
    fn earlier_incompatible_install_blocks_later_compatible_copy() {
        let fixture = super::super::tests::Fixture::new();
        let old = fixture.executable("old", b"old native app");
        let current = fixture.executable("current", PROTOCOL_MARKER);
        let diagnosis = diagnose_candidates(&[old, current]);
        assert_eq!(diagnosis.selected, None);
        assert_eq!(diagnosis.candidates[0].status, "incompatible");
        assert_eq!(diagnosis.candidates[1].status, "compatible");
        assert!(!diagnosis.candidates[1].selected);
    }

    #[test]
    fn compatible_candidate_is_selected() {
        let fixture = super::super::tests::Fixture::new();
        let current = fixture.executable("current", PROTOCOL_MARKER);
        let diagnosis = diagnose_candidates(from_ref(&current));
        assert_eq!(diagnosis.selected, Some(current));
        assert!(diagnosis.candidates[0].selected);
    }
}
