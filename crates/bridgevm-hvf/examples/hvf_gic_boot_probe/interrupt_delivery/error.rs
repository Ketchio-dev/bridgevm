//! Both interrupt providers stop through the same existing run-loop policy.
use crate::*;

pub(crate) enum InterruptDeliveryError {
    Spi(SpiDeliveryError),
    Msix {
        context: DrainContext,
        failure: MsixDeliveryFailure,
    },
}

impl std::fmt::Display for InterruptDeliveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spi(error) => error.fmt(f),
            Self::Msix { context, failure } => write!(f,
                "MSI-X delivery failed: vector={} address={:#x} data={:#x} status={:#x} location={} exit={} pc={:#x}",
                failure.message.vector, failure.message.address, failure.message.data,
                failure.status, context.location.as_str(), context.exit, context.pc),
        }
    }
}

impl InterruptDeliveryError {
    pub(crate) fn stop_primary(self, fatal: &mut bool, pc: &mut u64) -> String {
        *fatal = true;
        *pc = match &self {
            Self::Spi(error) => error.context.pc,
            Self::Msix { context, .. } => context.pc,
        };
        self.to_string()
    }
    pub(crate) fn stop_secondary(self, record_failure: impl FnOnce()) {
        println!("{self}");
        record_failure();
    }
}
