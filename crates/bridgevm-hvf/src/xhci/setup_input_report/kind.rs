#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SetupInputReportKind {
    Key,
    Release,
}

impl SetupInputReportKind {
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Release => "release",
        }
    }
}
