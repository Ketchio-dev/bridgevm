//! Power-cycling swtpm when a helper generation boots its guest.
//!
//! swtpm keeps volatile TPM state (PCRs, sessions, platform authorization)
//! across client reconnects, and product runs start a new helper process for
//! every guest reset against the same swtpm process. A generation that boots
//! its guest from reset must therefore send CMD_INIT before any guest
//! instruction runs, as the in-process reset path already does.

use super::*;

impl SwtpmUnixBackend {
    /// Connect for a guest that boots from reset. With a control socket the
    /// TPM is power-cycled before this returns, so the firmware's
    /// TPM2_Startup(CLEAR) starts from cleared PCRs and platform authorization.
    /// A failed power cycle latches the backend and is returned; the caller
    /// must not boot. Without a control socket nothing is sent.
    pub fn connect_for_boot(
        data_path: impl AsRef<Path>,
        control_path: Option<impl AsRef<Path>>,
    ) -> Result<Self, TpmBackendError> {
        let mut backend = Self::connect_with_control(data_path, control_path)?;
        backend.reset()?;
        Ok(backend)
    }
}
