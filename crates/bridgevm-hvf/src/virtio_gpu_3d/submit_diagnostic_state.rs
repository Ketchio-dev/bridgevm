// Device-side storage for the diagnostic belonging to the command being traced.

impl VirtioGpu3d {
    pub(crate) fn clear_submit_diagnostic(&mut self) {
        self.last_submit_diagnostic = None;
    }

    pub(crate) fn set_submit_diagnostic(&mut self, diagnostic: Option<Submit3dDiagnostic>) {
        self.last_submit_diagnostic = diagnostic;
    }

    pub(crate) fn take_submit_diagnostic(&mut self) -> Option<Submit3dDiagnostic> {
        self.last_submit_diagnostic.take()
    }
}
