//! Adopt and observe a helper before any fallible post-spawn action.
use crate::owned_child::{ChildExit, OwnedChild};
use crate::{spawn_helper, ChildRole, HelperLaunch, PreparedVm, RuntimeControl, RuntimeError};
pub(super) fn run(
    prepared: &PreparedVm,
    launch: &HelperLaunch,
    generation: u64,
    control: &RuntimeControl<'_>,
) -> Result<(u32, ChildExit), RuntimeError> {
    let child = spawn_helper(prepared.manifest(), launch, generation).map_err(|source| {
        RuntimeError::Io {
            context: "spawn VM helper",
            source,
        }
    })?;
    // No fallible work may separate spawn from ownership transfer.
    let mut child = OwnedChild::adopt(child, ChildRole::Helper, Some(generation), control);
    let pid = child.id();
    let exit = child.wait(control).map_err(|source| RuntimeError::Io {
        context: "wait for VM helper",
        source,
    })?;
    Ok((pid, exit))
}
