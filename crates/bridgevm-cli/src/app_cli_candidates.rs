use super::*;

pub(super) fn candidates(executable: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(contents) = executable.ancestors().nth(4) {
        if contents.file_name() == Some(OsStr::new("Contents"))
            && executable == contents.join("Resources/target/release/bridgevm")
            && contents.parent().and_then(Path::extension) == Some(OsStr::new("app"))
        {
            paths.push(contents.join("MacOS/BridgeVMControl"));
        }
    }
    let roots = std::iter::once(PathBuf::from("/Applications"))
        .chain(home.map(|home| home.join("Applications")));
    for root in roots {
        for name in ["BridgeVM.app", "BridgeVMControl.app"] {
            paths.push(root.join(name).join("Contents/MacOS/BridgeVMControl"));
        }
    }
    paths.dedup();
    paths
}
