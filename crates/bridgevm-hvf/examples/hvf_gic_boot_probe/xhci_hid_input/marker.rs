use std::fmt::{Display, Formatter, Result as FmtResult};

pub(crate) const MARKER_MAX_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerSource {
    Default,
    Custom,
}

impl MarkerSource {
    const fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeMarker {
    bytes: Vec<u8>,
    source: MarkerSource,
}

impl ProbeMarker {
    pub(crate) fn default_bytes(bytes: &'static [u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            source: MarkerSource::Default,
        }
    }

    pub(crate) fn custom_from_env(env_name: &'static str) -> Result<Option<Self>, MarkerEnvError> {
        match std::env::var(env_name) {
            Ok(value) => Self::custom_from_string(value).map(Some),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(MarkerEnvError::NotUnicode { env_name }),
        }
    }

    fn custom_from_string(value: String) -> Result<Self, MarkerEnvError> {
        Self::custom_from_bytes(value.into_bytes())
    }

    fn custom_from_bytes(bytes: Vec<u8>) -> Result<Self, MarkerEnvError> {
        if bytes.is_empty() {
            return Err(MarkerEnvError::Empty);
        }
        if bytes.len() > MARKER_MAX_BYTES {
            return Err(MarkerEnvError::TooLong {
                len: bytes.len(),
                max: MARKER_MAX_BYTES,
            });
        }
        Ok(Self {
            bytes,
            source: MarkerSource::Custom,
        })
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) fn log_summary(&self) -> MarkerLogSummary<'_> {
        MarkerLogSummary { marker: self }
    }

    #[cfg(test)]
    pub(crate) fn custom_for_test(bytes: &[u8]) -> Result<Self, MarkerEnvError> {
        Self::custom_from_bytes(bytes.to_vec())
    }

    #[cfg(test)]
    pub(crate) fn source_name(&self) -> &'static str {
        self.source.name()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkerEnvError {
    Empty,
    TooLong { len: usize, max: usize },
    NotUnicode { env_name: &'static str },
}

impl MarkerEnvError {
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Empty => "marker_empty",
            Self::TooLong { .. } => "marker_too_long",
            Self::NotUnicode { .. } => "marker_not_unicode",
        }
    }

    pub(crate) fn rejection_summary(&self) -> MarkerRejectionSummary<'_> {
        MarkerRejectionSummary { error: self }
    }
}

pub(crate) struct MarkerLogSummary<'a> {
    marker: &'a ProbeMarker,
}

impl Display for MarkerLogSummary<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(
            f,
            "marker_source={} marker_bytes={} marker_hash={:08x}",
            self.marker.source.name(),
            self.marker.bytes.len(),
            marker_hash32(&self.marker.bytes)
        )
    }
}

pub(crate) struct MarkerRejectionSummary<'a> {
    error: &'a MarkerEnvError,
}

impl Display for MarkerRejectionSummary<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self.error {
            MarkerEnvError::Empty => write!(
                f,
                "marker_source=custom marker_bytes=0 marker_limit={MARKER_MAX_BYTES} marker_hash=redacted"
            ),
            MarkerEnvError::TooLong { len, max } => write!(
                f,
                "marker_source=custom marker_bytes={len} marker_limit={max} marker_hash=redacted"
            ),
            MarkerEnvError::NotUnicode { env_name } => write!(
                f,
                "marker_env={env_name} marker_source=custom marker_bytes=redacted marker_limit={MARKER_MAX_BYTES} marker_hash=redacted"
            ),
        }
    }
}

fn marker_hash32(bytes: &[u8]) -> u32 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash as u32
}
