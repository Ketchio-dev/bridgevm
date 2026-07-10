use std::time::Duration;

const RAMFB_SAMPLE_ENV_MAX_BYTES: usize = 128;
const RAMFB_SAMPLE_MAX_CHECKPOINTS: usize = 16;
const RAMFB_SAMPLE_MAX_MS: u64 = 120_000;
const RAMFB_SAMPLE_DEFAULT_MS: &[u64] = &[1_000, 5_000, 15_000];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RamfbSampleEnvError {
    Empty,
    TooLong { len: usize, max: usize },
    TooMany { requested: usize, max: usize },
    Invalid { token: String },
    TooLarge { requested_ms: u64, max_ms: u64 },
    Duplicate { sample_ms: u64 },
}

impl RamfbSampleEnvError {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong { .. } => "too_long",
            Self::TooMany { .. } => "too_many",
            Self::Invalid { .. } => "invalid",
            Self::TooLarge { .. } => "too_large",
            Self::Duplicate { .. } => "duplicate",
        }
    }
}

#[derive(Debug)]
pub struct RamfbSampleSchedule {
    checkpoints: Vec<RamfbSampleCheckpoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RamfbShellObservation {
    ContinueSampling { message: &'static str },
    StopNow { reason: &'static str },
}

#[derive(Debug)]
struct RamfbSampleCheckpoint {
    delay: Duration,
    label: String,
    emitted: bool,
}

impl Default for RamfbSampleSchedule {
    fn default() -> Self {
        Self {
            checkpoints: RAMFB_SAMPLE_DEFAULT_MS
                .iter()
                .map(|sample_ms| ramfb_sample_checkpoint(*sample_ms))
                .collect(),
        }
    }
}

impl RamfbSampleSchedule {
    pub fn from_env(env_name: &str) -> Result<Self, RamfbSampleEnvError> {
        match std::env::var(env_name) {
            Ok(value) => Self::from_env_value(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(std::env::VarError::NotUnicode(_)) => Err(RamfbSampleEnvError::Invalid {
                token: String::from("<non-unicode>"),
            }),
        }
    }

    pub fn from_env_value(value: &str) -> Result<Self, RamfbSampleEnvError> {
        if value.len() > RAMFB_SAMPLE_ENV_MAX_BYTES {
            return Err(RamfbSampleEnvError::TooLong {
                len: value.len(),
                max: RAMFB_SAMPLE_ENV_MAX_BYTES,
            });
        }
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(RamfbSampleEnvError::Empty);
        }
        let mut sample_ms = Vec::new();
        for token in trimmed
            .split([',', ' ', '\t', '\n'])
            .filter(|token| !token.is_empty())
        {
            let Ok(parsed_ms) = token.parse::<u64>() else {
                return Err(RamfbSampleEnvError::Invalid {
                    token: token.to_string(),
                });
            };
            sample_ms.push(parsed_ms);
        }
        Self::from_millis_values(&sample_ms)
    }

    pub fn from_millis_values(sample_ms: &[u64]) -> Result<Self, RamfbSampleEnvError> {
        if sample_ms.is_empty() {
            return Err(RamfbSampleEnvError::Empty);
        }
        let mut unique_sample_ms = Vec::new();
        for value in sample_ms {
            validate_sample_ms(*value)?;
            if unique_sample_ms.contains(value) {
                return Err(RamfbSampleEnvError::Duplicate { sample_ms: *value });
            }
            unique_sample_ms.push(*value);
            if unique_sample_ms.len() > RAMFB_SAMPLE_MAX_CHECKPOINTS {
                return Err(RamfbSampleEnvError::TooMany {
                    requested: unique_sample_ms.len(),
                    max: RAMFB_SAMPLE_MAX_CHECKPOINTS,
                });
            }
        }
        Ok(Self {
            checkpoints: unique_sample_ms
                .iter()
                .map(|sample_ms| ramfb_sample_checkpoint(*sample_ms))
                .collect(),
        })
    }

    pub fn emit_due<F>(&mut self, elapsed: Duration, mut emit_checkpoint: F)
    where
        F: FnMut(&str),
    {
        for checkpoint in &mut self.checkpoints {
            if !checkpoint.emitted && elapsed >= checkpoint.delay {
                checkpoint.emitted = true;
                emit_checkpoint(&checkpoint.label);
            }
        }
    }

    pub fn has_due_checkpoint(&self, elapsed: Duration) -> bool {
        self.checkpoints
            .iter()
            .any(|checkpoint| !checkpoint.emitted && elapsed >= checkpoint.delay)
    }

    pub fn is_complete(&self) -> bool {
        self.checkpoints.iter().all(|checkpoint| checkpoint.emitted)
    }

    pub fn next_deadline_after(&self, elapsed: Duration) -> Option<Duration> {
        self.checkpoints
            .iter()
            .filter(|checkpoint| !checkpoint.emitted && checkpoint.delay > elapsed)
            .map(|checkpoint| checkpoint.delay)
            .min()
    }

    pub fn uefi_shell_observation(
        &self,
        sample_until_complete: bool,
        shell_was_observed: bool,
    ) -> RamfbShellObservation {
        if !sample_until_complete {
            return RamfbShellObservation::StopNow {
                reason: "serial reached UEFI shell",
            };
        }
        if !self.is_complete() {
            return RamfbShellObservation::ContinueSampling {
                message: "serial reached UEFI shell before RAMFB sample schedule complete",
            };
        }
        if shell_was_observed {
            RamfbShellObservation::StopNow {
                reason: "ramfb sample schedule complete after UEFI shell",
            }
        } else {
            RamfbShellObservation::StopNow {
                reason: "serial reached UEFI shell",
            }
        }
    }
}

fn validate_sample_ms(sample_ms: u64) -> Result<(), RamfbSampleEnvError> {
    if sample_ms == 0 {
        return Err(RamfbSampleEnvError::Invalid {
            token: sample_ms.to_string(),
        });
    }
    if sample_ms > RAMFB_SAMPLE_MAX_MS {
        return Err(RamfbSampleEnvError::TooLarge {
            requested_ms: sample_ms,
            max_ms: RAMFB_SAMPLE_MAX_MS,
        });
    }
    Ok(())
}

fn ramfb_sample_checkpoint(sample_ms: u64) -> RamfbSampleCheckpoint {
    RamfbSampleCheckpoint {
        delay: Duration::from_millis(sample_ms),
        label: format!("ramfb-sample-{sample_ms}ms"),
        emitted: false,
    }
}
