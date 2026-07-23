//! The AppleVzLauncher trait and the unsupported no-op implementation.

use crate::*;

pub trait AppleVzLauncher {
    fn launch(
        &self,
        handoff: AppleVzLaunchHandoff,
    ) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedAppleVzLauncher;

impl AppleVzLauncher for UnsupportedAppleVzLauncher {
    fn launch(
        &self,
        handoff: AppleVzLaunchHandoff,
    ) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError> {
        Err(AppleVzLaunchError::Unsupported {
            message:
                "Apple Virtualization.framework launch requires --apple-vz-runner to point at a signed AppleVzRunner"
                    .to_string(),
            handoff: Box::new(handoff),
        })
    }
}
