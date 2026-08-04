//! Host entropy provider for the guest-visible SMCCC TRNG service.
//!
//! On macOS this is `SecRandomCopyBytes` from Security.framework, the OS
//! CSPRNG. There is deliberately no fallback: if the call fails, the TRNG
//! service reports `NO_ENTROPY` rather than substituting anything derived from
//! time, PIDs, counters or constants.
//!
//! Using the OS CSPRNG proves the *source*, not the statistical quality of any
//! particular sample. Tests here check wiring and failure behaviour; they are
//! not an entropy-quality argument.

use crate::smccc_trng::{EntropyError, EntropySource};

/// The host OS cryptographic random number generator.
#[derive(Debug, Default, Clone, Copy)]
pub struct HostEntropy;

impl HostEntropy {
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
mod sys {
    use super::EntropyError;

    // Security.framework. `kSecRandomDefault` is a null pointer to the default
    // generator, and the call returns errSecSuccess (0) on success.
    #[link(name = "Security", kind = "framework")]
    extern "C" {
        fn SecRandomCopyBytes(rnd: *const core::ffi::c_void, count: usize, bytes: *mut u8) -> i32;
    }

    const ERR_SEC_SUCCESS: i32 = 0;

    pub(super) fn fill(out: &mut [u8]) -> Result<(), EntropyError> {
        if out.is_empty() {
            return Ok(());
        }
        // SAFETY: `out` is a valid, uniquely borrowed slice of `out.len()`
        // bytes, which is exactly the count passed to the call.
        let status = unsafe { SecRandomCopyBytes(core::ptr::null(), out.len(), out.as_mut_ptr()) };
        if status == ERR_SEC_SUCCESS {
            Ok(())
        } else {
            // Do not leave partially written bytes visible to a caller that
            // ignores the error.
            out.fill(0);
            Err(EntropyError::Unavailable)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod sys {
    use super::EntropyError;

    pub(super) fn fill(out: &mut [u8]) -> Result<(), EntropyError> {
        out.fill(0);
        Err(EntropyError::Unsupported)
    }
}

impl EntropySource for HostEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        sys::fill(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn host_provider_fills_every_requested_length() {
        let mut entropy = HostEntropy::new();
        for len in [1usize, 4, 8, 15, 16, 24] {
            let mut buf = vec![0u8; len];
            entropy.fill(&mut buf).expect("host CSPRNG must succeed");
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn repeated_draws_differ() {
        // A wiring smoke: a stuck or unwired provider returns a constant. This
        // is not a randomness-quality test, and 24 bytes colliding by chance is
        // not a realistic outcome.
        let mut entropy = HostEntropy::new();
        let mut first = [0u8; 24];
        let mut second = [0u8; 24];
        entropy.fill(&mut first).expect("host CSPRNG must succeed");
        entropy.fill(&mut second).expect("host CSPRNG must succeed");
        assert_ne!(first, second);
        assert_ne!(first, [0u8; 24]);
    }

    #[test]
    fn empty_request_is_trivially_satisfied() {
        let mut entropy = HostEntropy::new();
        assert!(entropy.fill(&mut []).is_ok());
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn absent_provider_reports_unsupported() {
        let mut entropy = HostEntropy::new();
        let mut buf = [0u8; 8];
        assert_eq!(entropy.fill(&mut buf), Err(EntropyError::Unsupported));
        assert_eq!(buf, [0u8; 8]);
    }
}
