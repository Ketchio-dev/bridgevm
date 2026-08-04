//! Arm SMCCC TRNG firmware interface (Arm DEN 0098, version 1.0).
//!
//! This is the guest-visible random number service. A guest that seeds a
//! CSPRNG from it is entitled to assume the bits are unpredictable, so this
//! module is deliberately split into two halves:
//!
//! * the **protocol**, implemented here as pure logic over an
//!   [`EntropySource`], so every request/response rule is unit-testable on any
//!   host; and
//! * the **entropy provider**, which must be the host OS CSPRNG.
//!
//! There is no fallback path. If the provider fails, the call returns
//! `NO_ENTROPY` and the data registers are zero; it never degrades to a
//! predictable value. A previous implementation derived guest randomness from
//! the vCPU exit counter, which is fully predictable from inside the guest.
//!
//! Register layout follows the specification: entropy fills X3 first, then X2,
//! then X1, and bits above the requested count are zero.

/// Function IDs, as assigned by DEN 0098.
pub mod func {
    pub const VERSION: u64 = 0x8400_0050;
    pub const FEATURES: u64 = 0x8400_0051;
    pub const GET_UUID: u64 = 0x8400_0052;
    pub const RND32: u64 = 0x8400_0053;
    pub const RND64: u64 = 0xc400_0053;
}

/// Return codes. These are TRNG-specific and deliberately separate from the
/// PSCI status namespace, even where the numeric values coincide.
pub mod status {
    pub const SUCCESS: u64 = 0;
    pub const NOT_SUPPORTED: u64 = (-1i64) as u64;
    pub const INVALID_PARAMETER: u64 = (-2i64) as u64;
    pub const NO_ENTROPY: u64 = (-3i64) as u64;
}

/// TRNG v1.0: major 1 in bits [30:16], minor 0 in bits [15:0].
pub const VERSION_1_0: u64 = 0x1_0000;

/// Maximum bits returnable in one call: three registers wide.
pub const MAX_BITS_RND64: u32 = 192;
pub const MAX_BITS_RND32: u32 = 96;

/// The service UUID from the specification, as four little-endian words.
pub const UUID_WORDS: [u64; 4] = [0x0d21_e000, 0x4384_11eb, 0x8070_5244, 0x554e_5a4c];

/// A source of unpredictable bytes.
///
/// Implementations must come from a cryptographically secure generator. The
/// only reason this is a trait is so tests can drive the protocol
/// deterministically and prove the failure path returns no data.
pub trait EntropySource {
    /// Fill `out` completely, or fail. Partial fills are a failure.
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntropyError {
    /// The provider exists but could not produce entropy for this call.
    Unavailable,
    /// There is no entropy provider on this platform at all.
    Unsupported,
}

/// The four values an SMCCC call returns in X0..=X3.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmcccReturn {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
}

impl SmcccReturn {
    fn status(code: u64) -> Self {
        Self {
            x0: code,
            x1: 0,
            x2: 0,
            x3: 0,
        }
    }
}

/// Handle one TRNG function call.
///
/// `x1` is the call's first argument: the requested bit count for `RND32`
/// and `RND64`, or the queried function ID for `FEATURES`.
pub fn handle_call<S: EntropySource>(
    function_id: u64,
    x1: u64,
    entropy: &mut S,
) -> Option<SmcccReturn> {
    match function_id {
        func::VERSION => Some(SmcccReturn::status(VERSION_1_0)),
        func::FEATURES => Some(SmcccReturn::status(features(x1))),
        func::GET_UUID => Some(SmcccReturn {
            x0: UUID_WORDS[0],
            x1: UUID_WORDS[1],
            x2: UUID_WORDS[2],
            x3: UUID_WORDS[3],
        }),
        func::RND32 => Some(rnd(x1, 32, entropy)),
        func::RND64 => Some(rnd(x1, 64, entropy)),
        _ => None,
    }
}

/// `TRNG_FEATURES` reports only the functions this implementation actually
/// serves. Advertising an unimplemented call here is how a guest ends up
/// trusting a service that does not exist.
fn features(queried: u64) -> u64 {
    match queried {
        func::VERSION | func::FEATURES | func::GET_UUID | func::RND32 | func::RND64 => {
            status::SUCCESS
        }
        _ => status::NOT_SUPPORTED,
    }
}

fn rnd<S: EntropySource>(requested: u64, register_bits: u32, entropy: &mut S) -> SmcccReturn {
    let max_bits = 3 * register_bits;

    // The specification takes the bit count from the low 32 bits of X1, and a
    // request larger than three registers is invalid. Zero bits is a valid
    // request that returns success with no entropy bits set.
    if requested > u64::from(max_bits) {
        return SmcccReturn::status(status::INVALID_PARAMETER);
    }
    let requested = requested as u32;

    if requested == 0 {
        return SmcccReturn::status(status::SUCCESS);
    }

    let mut bytes = [0u8; 24];
    let needed = requested.div_ceil(8) as usize;
    if let Err(err) = entropy.fill(&mut bytes[..needed]) {
        let code = match err {
            EntropyError::Unavailable => status::NO_ENTROPY,
            EntropyError::Unsupported => status::NOT_SUPPORTED,
        };
        // Deliberately return no data alongside the error.
        return SmcccReturn::status(code);
    }

    let mut words = [0u64; 3];
    for (index, word) in words.iter_mut().enumerate() {
        let start = index * 8;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&bytes[start..start + 8]);
        *word = u64::from_le_bytes(raw);
    }

    // Clear every bit above the request so the guest never receives entropy it
    // did not ask for, and so unused registers read as zero.
    mask_to_requested_bits(&mut words, requested, register_bits);

    if register_bits == 32 {
        SmcccReturn {
            x0: status::SUCCESS,
            x1: words[2] & 0xffff_ffff,
            x2: words[1] & 0xffff_ffff,
            x3: words[0] & 0xffff_ffff,
        }
    } else {
        SmcccReturn {
            x0: status::SUCCESS,
            x1: words[2],
            x2: words[1],
            x3: words[0],
        }
    }
}

/// Zero the bits above `requested`, treating the three registers as one
/// little-endian bit string that fills X3 first.
fn mask_to_requested_bits(words: &mut [u64; 3], requested: u32, register_bits: u32) {
    for (index, word) in words.iter_mut().enumerate() {
        let already = index as u32 * register_bits;
        if already >= requested {
            *word = 0;
            continue;
        }
        let usable = (requested - already).min(register_bits);
        if usable < 64 {
            *word &= (1u64 << usable) - 1;
        }
    }
}

#[cfg(test)]
#[path = "smccc_trng_tests.rs"]
mod tests;
