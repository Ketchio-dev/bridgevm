use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::xhci::{
    PointerInputAction, PointerPosition, XhciPointerInputQueueError, XhciPointerInputReportStats,
};

use super::marker::{MarkerEnvError, ProbeMarker, MARKER_MAX_BYTES};
use super::report_text::{contains_bytes, IncrementalMarkerScan};

const POINTER_INPUT_DEFAULT_MARKER: &[u8] = b"BdsDxe: starting Boot0001";
const POINTER_INPUT_ENV_MAX_BYTES: usize = 128;
const POINTER_INPUT_MAX_ACTIONS: usize = 16;
const POINTER_INPUT_AXIS_MAX: u32 = 0x7fff;
const POINTER_INPUT_AXIS_CENTER: u16 = 0x3fff;
const POINTER_INPUT_FIRE_DELAY_ENV: &str = "BRIDGEVM_XHCI_POINTER_INPUT_FIRE_DELAY_MS";
const POINTER_INPUT_FIRE_DELAY_ENV_MAX_BYTES: usize = 32;
const POINTER_INPUT_FIRE_MAX_DELAY_MS: u64 = 600_000;
const POINTER_INPUT_RAMFB_DELAY_ENV: &str = "BRIDGEVM_XHCI_POINTER_INPUT_RAMFB_DELAY_MS";
const POINTER_INPUT_RAMFB_DELAY_ENV_MAX_BYTES: usize = 128;
const POINTER_INPUT_RAMFB_MAX_DELAYS: usize = 16;
const POINTER_INPUT_RAMFB_MAX_DELAY_MS: u64 = 120_000;
const POINTER_INPUT_RAMFB_DEFAULT_DELAY_MS: &[u64] = &[1_000, 5_000, 15_000];

const _: () = {
    assert!(!POINTER_INPUT_DEFAULT_MARKER.is_empty());
    assert!(POINTER_INPUT_DEFAULT_MARKER.len() <= MARKER_MAX_BYTES);
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum XhciPointerInputEnvError {
    Empty,
    TooLong { len: usize, max: usize },
    TooManyActions { requested: usize, max: usize },
    UnsupportedToken { token: String },
    InvalidCoordinate { token: String },
    CoordinateOutOfRange { token: String },
    FireDelayTooLong { len: usize, max: usize },
    FireDelayInvalid { token: String },
    FireDelayTooLarge { requested_ms: u64, max_ms: u64 },
    RamfbDelayEmpty,
    RamfbDelayTooLong { len: usize, max: usize },
    RamfbDelayTooMany { requested: usize, max: usize },
    RamfbDelayInvalid { token: String },
    RamfbDelayTooLarge { requested_ms: u64, max_ms: u64 },
    RamfbDelayDuplicate { delay_ms: u64 },
    Marker(MarkerEnvError),
}

impl XhciPointerInputEnvError {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong { .. } => "too_long",
            Self::TooManyActions { .. } => "too_many_actions",
            Self::UnsupportedToken { .. } => "unsupported_token",
            Self::InvalidCoordinate { .. } => "invalid_coordinate",
            Self::CoordinateOutOfRange { .. } => "coordinate_out_of_range",
            Self::FireDelayTooLong { .. } => "fire_delay_too_long",
            Self::FireDelayInvalid { .. } => "fire_delay_invalid",
            Self::FireDelayTooLarge { .. } => "fire_delay_too_large",
            Self::RamfbDelayEmpty => "ramfb_delay_empty",
            Self::RamfbDelayTooLong { .. } => "ramfb_delay_too_long",
            Self::RamfbDelayTooMany { .. } => "ramfb_delay_too_many",
            Self::RamfbDelayInvalid { .. } => "ramfb_delay_invalid",
            Self::RamfbDelayTooLarge { .. } => "ramfb_delay_too_large",
            Self::RamfbDelayDuplicate { .. } => "ramfb_delay_duplicate",
            Self::Marker(error) => error.name(),
        }
    }
}

#[derive(Debug)]
struct PointerRamfbDelayCheckpoint {
    label: String,
    delay: Duration,
    emitted: bool,
}

#[derive(Debug)]
pub(crate) struct XhciPointerInputTrigger {
    name: &'static str,
    marker: ProbeMarker,
    marker_scan: IncrementalMarkerScan,
    actions: Vec<PointerInputAction>,
    attempted: bool,
    attempted_controller_reset_generation: u64,
    pending_emitted_reports_at_attempt: Option<u64>,
    fired: bool,
    reported_queue_rejection: bool,
    marker_seen_at: Option<Instant>,
    fire_delay: Duration,
    fired_at: Option<Instant>,
    ramfb_delay_checkpoints: Vec<PointerRamfbDelayCheckpoint>,
}

impl XhciPointerInputTrigger {
    pub(crate) fn from_env(
        name: &'static str,
        actions_env: &'static str,
        marker_env: &'static str,
    ) -> Option<Result<Self, XhciPointerInputEnvError>> {
        Self::from_env_with_timing_envs(
            name,
            actions_env,
            marker_env,
            POINTER_INPUT_FIRE_DELAY_ENV,
            POINTER_INPUT_RAMFB_DELAY_ENV,
        )
    }

    fn from_env_with_timing_envs(
        name: &'static str,
        actions_env: &'static str,
        marker_env: &'static str,
        fire_delay_env: &'static str,
        ramfb_delay_env: &'static str,
    ) -> Option<Result<Self, XhciPointerInputEnvError>> {
        let value = std::env::var(actions_env).ok()?;
        let marker = match ProbeMarker::custom_from_env(marker_env) {
            Ok(Some(marker)) => marker,
            Ok(None) => ProbeMarker::default_bytes(POINTER_INPUT_DEFAULT_MARKER),
            Err(error) => return Some(Err(XhciPointerInputEnvError::Marker(error))),
        };
        let fire_delay = match parse_pointer_input_fire_delay_named_env(fire_delay_env) {
            Ok(delay) => delay,
            Err(error) => return Some(Err(error)),
        };
        let ramfb_delay_checkpoints =
            match parse_pointer_input_ramfb_delay_named_env(ramfb_delay_env) {
                Ok(checkpoints) => checkpoints,
                Err(error) => return Some(Err(error)),
            };
        Some(Self::from_env_value_with_marker_delay_and_ramfb(
            name,
            &value,
            marker,
            fire_delay,
            ramfb_delay_checkpoints,
        ))
    }

    #[cfg(test)]
    fn from_env_value_with_marker_and_delay(
        name: &'static str,
        value: &str,
        marker: ProbeMarker,
        fire_delay: Duration,
    ) -> Result<Self, XhciPointerInputEnvError> {
        Self::from_env_value_with_marker_delay_and_ramfb(
            name,
            value,
            marker,
            fire_delay,
            default_pointer_ramfb_delay_checkpoints(),
        )
    }

    fn from_env_value_with_marker_delay_and_ramfb(
        name: &'static str,
        value: &str,
        marker: ProbeMarker,
        fire_delay: Duration,
        ramfb_delay_checkpoints: Vec<PointerRamfbDelayCheckpoint>,
    ) -> Result<Self, XhciPointerInputEnvError> {
        Ok(Self {
            name,
            marker,
            marker_scan: IncrementalMarkerScan::default(),
            actions: parse_pointer_input_actions(value)?,
            attempted: false,
            attempted_controller_reset_generation: 0,
            pending_emitted_reports_at_attempt: None,
            fired: false,
            reported_queue_rejection: false,
            marker_seen_at: None,
            fire_delay,
            fired_at: None,
            ramfb_delay_checkpoints,
        })
    }

    pub(crate) fn maybe_fire_with_mem_at(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) -> bool {
        if !self.fire_delay_elapsed_at(platform, now) {
            return false;
        }
        self.mark_attempted_at_controller_generation(platform);
        let before_stats = platform.xhci_pointer_input_report_stats();
        match platform.queue_xhci_pointer_input_actions_with_mem(&self.actions, mem) {
            Ok(()) => {
                let after_stats = platform.xhci_pointer_input_report_stats();
                if emitted_reports(after_stats) > emitted_reports(before_stats) {
                    self.record_fire(platform)
                } else {
                    self.pending_emitted_reports_at_attempt = Some(emitted_reports(before_stats));
                    false
                }
            }
            Err(error) => {
                self.report_queue_rejection(platform, error);
                false
            }
        }
    }

    pub(crate) fn maybe_fire_with_mem_and_ramfb_checkpoints_at<F>(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
        mut emit_checkpoint: F,
    ) -> bool
    where
        F: FnMut(&VirtPlatform, &str, &dyn GuestMemoryMut),
    {
        let can_checkpoint = self.ready_for_ramfb_checkpoints_at(platform, now);
        if can_checkpoint {
            emit_checkpoint(platform, "pointer-input-before", mem);
        }
        let fired = self.maybe_fire_with_mem_at(platform, mem, now);
        if fired {
            self.fired_at = Some(now);
            if can_checkpoint {
                emit_checkpoint(platform, "pointer-input-after", mem);
            }
        }
        self.emit_due_ramfb_delay_checkpoints(now, &mut |label| {
            emit_checkpoint(platform, label, mem)
        });
        fired
    }

    pub(crate) fn pending_host_wake_deadline_at(
        &mut self,
        platform: &VirtPlatform,
        now: Instant,
    ) -> Option<Instant> {
        self.complete_pending_fire_if_report_emitted_at(platform, now);
        if self.fired
            || self.attempted_in_current_controller_generation(platform)
            || !self
                .marker_scan
                .contains_new(platform.uart_output(), self.marker.as_bytes())
        {
            return None;
        }
        let marker_seen_at = *self.marker_seen_at.get_or_insert(now);
        let deadline = marker_seen_at.checked_add(self.fire_delay)?;
        (deadline > now).then_some(deadline)
    }

    pub(crate) fn print_summary(&self, platform: &VirtPlatform) {
        let stats = platform.xhci_pointer_input_report_stats();
        let marker_seen = contains_bytes(platform.uart_output(), self.marker.as_bytes());
        println!(
            "xHCI pointer-input injection {}: fired={} attempted={} marker_seen={} actions={} queued_actions={} queued_reports={} emitted_move_reports={} emitted_button_reports={} emitted_release_reports={} emitted_wheel_reports={} empty_sequence_rejections={} too_many_action_rejections={} busy_rejections={} controller_reset_generation={} endpoint_id=5 report_bytes=6 {}",
            self.name,
            self.fired,
            self.attempted,
            marker_seen,
            format_pointer_action_names(&self.actions),
            stats.queued_actions,
            stats.queued_reports,
            stats.emitted_move_reports,
            stats.emitted_button_reports,
            stats.emitted_release_reports,
            stats.emitted_wheel_reports,
            stats.empty_sequence_rejections,
            stats.too_many_action_rejections,
            stats.busy_rejections,
            stats.controller_reset_generation,
            self.marker.log_summary()
        );
    }

    fn fire_delay_elapsed_at(&mut self, platform: &VirtPlatform, now: Instant) -> bool {
        self.complete_pending_fire_if_report_emitted_at(platform, now);
        if self.fired
            || self.attempted_in_current_controller_generation(platform)
            || !self
                .marker_scan
                .contains_new(platform.uart_output(), self.marker.as_bytes())
        {
            return false;
        }
        let marker_seen_at = *self.marker_seen_at.get_or_insert(now);
        match now.checked_duration_since(marker_seen_at) {
            Some(elapsed) => elapsed >= self.fire_delay,
            None => false,
        }
    }

    fn attempted_in_current_controller_generation(&self, platform: &VirtPlatform) -> bool {
        let stats = platform.xhci_pointer_input_report_stats();
        self.attempted
            && self.attempted_controller_reset_generation == stats.controller_reset_generation
    }

    fn mark_attempted_at_controller_generation(&mut self, platform: &VirtPlatform) {
        let stats = platform.xhci_pointer_input_report_stats();
        self.attempted = true;
        self.attempted_controller_reset_generation = stats.controller_reset_generation;
    }

    fn complete_pending_fire_if_report_emitted_at(
        &mut self,
        platform: &VirtPlatform,
        now: Instant,
    ) -> bool {
        let Some(emitted_at_attempt) = self.pending_emitted_reports_at_attempt else {
            return false;
        };
        let stats = platform.xhci_pointer_input_report_stats();
        if emitted_reports(stats) <= emitted_at_attempt {
            return false;
        }
        self.pending_emitted_reports_at_attempt = None;
        self.fired_at.get_or_insert(now);
        self.record_fire(platform)
    }

    fn record_fire(&mut self, platform: &VirtPlatform) -> bool {
        self.mark_attempted_at_controller_generation(platform);
        self.fired = true;
        let stats = platform.xhci_pointer_input_report_stats();
        println!(
            "xHCI pointer-input injection {} fired: actions={} queued_actions={} queued_reports={} emitted_move_reports={} emitted_button_reports={} emitted_release_reports={} emitted_wheel_reports={} rejected_count=0 endpoint_id=5 report_bytes=6 {}",
            self.name,
            format_pointer_action_names(&self.actions),
            stats.queued_actions,
            stats.queued_reports,
            stats.emitted_move_reports,
            stats.emitted_button_reports,
            stats.emitted_release_reports,
            stats.emitted_wheel_reports,
            self.marker.log_summary()
        );
        true
    }

    fn report_queue_rejection(
        &mut self,
        platform: &VirtPlatform,
        error: XhciPointerInputQueueError,
    ) {
        self.mark_attempted_at_controller_generation(platform);
        if self.reported_queue_rejection {
            return;
        }
        self.reported_queue_rejection = true;
        println!(
            "xHCI pointer-input injection {} rejected: actions={} queue_error={} queued_actions=0 queued_reports=0 rejected_count=1 endpoint_id=5 report_bytes=6 {}",
            self.name,
            format_pointer_action_names(&self.actions),
            pointer_queue_error_name(error),
            self.marker.log_summary()
        );
    }

    fn ready_for_ramfb_checkpoints_at(&mut self, platform: &VirtPlatform, now: Instant) -> bool {
        if !self.fire_delay_elapsed_at(platform, now) {
            return false;
        }
        let stats = platform.xhci_pointer_input_report_stats();
        stats.queued_reports == emitted_reports(stats)
    }

    fn emit_due_ramfb_delay_checkpoints<F>(&mut self, now: Instant, emit_checkpoint: &mut F)
    where
        F: FnMut(&str),
    {
        let Some(fired_at) = self.fired_at else {
            return;
        };
        let Some(elapsed) = now.checked_duration_since(fired_at) else {
            return;
        };
        for checkpoint in &mut self.ramfb_delay_checkpoints {
            if !checkpoint.emitted && elapsed >= checkpoint.delay {
                checkpoint.emitted = true;
                emit_checkpoint(&checkpoint.label);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn from_env_value(
        name: &'static str,
        value: &str,
    ) -> Result<Self, XhciPointerInputEnvError> {
        Self::from_env_value_with_marker_and_delay(
            name,
            value,
            ProbeMarker::default_bytes(POINTER_INPUT_DEFAULT_MARKER),
            Duration::ZERO,
        )
    }

    #[cfg(test)]
    pub(crate) fn from_env_value_with_custom_marker(
        name: &'static str,
        value: &str,
        marker: &[u8],
    ) -> Result<Self, XhciPointerInputEnvError> {
        let marker =
            ProbeMarker::custom_for_test(marker).map_err(XhciPointerInputEnvError::Marker)?;
        Self::from_env_value_with_marker_and_delay(name, value, marker, Duration::ZERO)
    }

    #[cfg(test)]
    pub(crate) fn from_env_value_with_ramfb_delay_ms(
        name: &'static str,
        value: &str,
        delays_ms: &[u64],
    ) -> Result<Self, XhciPointerInputEnvError> {
        Self::from_env_value_with_marker_delay_and_ramfb(
            name,
            value,
            ProbeMarker::default_bytes(POINTER_INPUT_DEFAULT_MARKER),
            Duration::ZERO,
            pointer_ramfb_delay_checkpoints_from_ms(delays_ms)?,
        )
    }

    #[cfg(test)]
    pub(crate) const fn fired(&self) -> bool {
        self.fired
    }

    #[cfg(test)]
    pub(crate) fn action_names(&self) -> String {
        format_pointer_action_names(&self.actions)
    }
}

pub(crate) fn print_pointer_input_rejection(name: &'static str, error: &XhciPointerInputEnvError) {
    if let XhciPointerInputEnvError::Marker(marker_error) = error {
        println!(
            "xHCI pointer-input injection {name} rejected: parse_error={} {} queued_actions=0 queued_reports=0 emitted_move_reports=0 emitted_button_reports=0 emitted_release_reports=0 emitted_wheel_reports=0 rejected_count=1 endpoint_id=5 report_bytes=6",
            error.name(),
            marker_error.rejection_summary()
        );
        return;
    }
    println!(
        "xHCI pointer-input injection {name} rejected: parse_error={} queued_actions=0 queued_reports=0 emitted_move_reports=0 emitted_button_reports=0 emitted_release_reports=0 emitted_wheel_reports=0 rejected_count=1 endpoint_id=5 report_bytes=6",
        error.name()
    );
}

pub(crate) fn parse_pointer_input_actions(
    value: &str,
) -> Result<Vec<PointerInputAction>, XhciPointerInputEnvError> {
    if value.len() > POINTER_INPUT_ENV_MAX_BYTES {
        return Err(XhciPointerInputEnvError::TooLong {
            len: value.len(),
            max: POINTER_INPUT_ENV_MAX_BYTES,
        });
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(XhciPointerInputEnvError::Empty);
    }
    let mut actions = Vec::new();
    for token in trimmed
        .split([',', ' ', '\t', '\n'])
        .filter(|token| !token.is_empty())
    {
        parse_pointer_input_token(token, &mut actions)?;
    }
    if actions.is_empty() {
        return Err(XhciPointerInputEnvError::Empty);
    }
    Ok(actions)
}

fn parse_pointer_input_token(
    token: &str,
    actions: &mut Vec<PointerInputAction>,
) -> Result<(), XhciPointerInputEnvError> {
    let normalized = token.to_ascii_lowercase();
    let action = if let Some(position) = normalized.strip_prefix("move:") {
        PointerInputAction::Move(parse_pointer_position(position, token)?)
    } else if let Some(position) = normalized.strip_prefix("press:") {
        PointerInputAction::Press {
            position: parse_pointer_position(position, token)?,
            buttons: 1,
        }
    } else if let Some(position) = normalized.strip_prefix("right-press:") {
        PointerInputAction::Press {
            position: parse_pointer_position(position, token)?,
            buttons: 2,
        }
    } else if let Some(position) = normalized.strip_prefix("release:") {
        PointerInputAction::Release(parse_pointer_position(position, token)?)
    } else if let Some(position) = normalized.strip_prefix("right-click:") {
        let position = parse_pointer_position(position, token)?;
        push_pointer_action(
            actions,
            PointerInputAction::Press {
                position,
                buttons: 2,
            },
        )?;
        PointerInputAction::Release(position)
    } else if let Some(value) = normalized.strip_prefix("scroll:") {
        let Some((delta, position)) = value.split_once('@') else {
            return Err(XhciPointerInputEnvError::UnsupportedToken {
                token: token.to_string(),
            });
        };
        let Ok(delta) = delta.parse::<i8>() else {
            return Err(XhciPointerInputEnvError::UnsupportedToken {
                token: token.to_string(),
            });
        };
        if delta == 0 {
            return Err(XhciPointerInputEnvError::UnsupportedToken {
                token: token.to_string(),
            });
        }
        PointerInputAction::Scroll {
            position: parse_pointer_position(position, token)?,
            delta,
        }
    } else if let Some(position) = normalized.strip_prefix("click:") {
        PointerInputAction::Click(parse_pointer_position(position, token)?)
    } else {
        return Err(XhciPointerInputEnvError::UnsupportedToken {
            token: token.to_string(),
        });
    };
    push_pointer_action(actions, action)
}

fn push_pointer_action(
    actions: &mut Vec<PointerInputAction>,
    action: PointerInputAction,
) -> Result<(), XhciPointerInputEnvError> {
    actions.push(action);
    if actions.len() > POINTER_INPUT_MAX_ACTIONS {
        return Err(XhciPointerInputEnvError::TooManyActions {
            requested: actions.len(),
            max: POINTER_INPUT_MAX_ACTIONS,
        });
    }
    Ok(())
}

fn parse_pointer_position(
    value: &str,
    original_token: &str,
) -> Result<PointerPosition, XhciPointerInputEnvError> {
    if value == "center" {
        return Ok(pointer_center());
    }
    let Some((x, y)) = value.split_once('x') else {
        return Err(XhciPointerInputEnvError::InvalidCoordinate {
            token: original_token.to_string(),
        });
    };
    let x = parse_axis(x, original_token)?;
    let y = parse_axis(y, original_token)?;
    PointerPosition::new(x, y).ok_or_else(|| XhciPointerInputEnvError::CoordinateOutOfRange {
        token: original_token.to_string(),
    })
}

fn parse_axis(value: &str, original_token: &str) -> Result<u16, XhciPointerInputEnvError> {
    let Ok(axis) = value.parse::<u32>() else {
        return Err(XhciPointerInputEnvError::InvalidCoordinate {
            token: original_token.to_string(),
        });
    };
    if axis > POINTER_INPUT_AXIS_MAX {
        return Err(XhciPointerInputEnvError::CoordinateOutOfRange {
            token: original_token.to_string(),
        });
    }
    Ok(axis as u16)
}

fn parse_pointer_input_fire_delay_named_env(
    env_name: &'static str,
) -> Result<Duration, XhciPointerInputEnvError> {
    match std::env::var(env_name) {
        Ok(value) => parse_pointer_input_fire_delay_value(&value),
        Err(std::env::VarError::NotPresent) => Ok(Duration::ZERO),
        Err(std::env::VarError::NotUnicode(_)) => Err(XhciPointerInputEnvError::FireDelayInvalid {
            token: String::from("<non-unicode>"),
        }),
    }
}

fn parse_pointer_input_fire_delay_value(value: &str) -> Result<Duration, XhciPointerInputEnvError> {
    if value.len() > POINTER_INPUT_FIRE_DELAY_ENV_MAX_BYTES {
        return Err(XhciPointerInputEnvError::FireDelayTooLong {
            len: value.len(),
            max: POINTER_INPUT_FIRE_DELAY_ENV_MAX_BYTES,
        });
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(XhciPointerInputEnvError::FireDelayInvalid {
            token: value.to_string(),
        });
    }
    let Ok(delay_ms) = trimmed.parse::<u64>() else {
        return Err(XhciPointerInputEnvError::FireDelayInvalid {
            token: trimmed.to_string(),
        });
    };
    if delay_ms > POINTER_INPUT_FIRE_MAX_DELAY_MS {
        return Err(XhciPointerInputEnvError::FireDelayTooLarge {
            requested_ms: delay_ms,
            max_ms: POINTER_INPUT_FIRE_MAX_DELAY_MS,
        });
    }
    Ok(Duration::from_millis(delay_ms))
}

fn parse_pointer_input_ramfb_delay_named_env(
    env_name: &'static str,
) -> Result<Vec<PointerRamfbDelayCheckpoint>, XhciPointerInputEnvError> {
    match std::env::var(env_name) {
        Ok(value) => parse_pointer_input_ramfb_delay_value(&value),
        Err(std::env::VarError::NotPresent) => Ok(default_pointer_ramfb_delay_checkpoints()),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(XhciPointerInputEnvError::RamfbDelayInvalid {
                token: String::from("<non-unicode>"),
            })
        }
    }
}

fn parse_pointer_input_ramfb_delay_value(
    value: &str,
) -> Result<Vec<PointerRamfbDelayCheckpoint>, XhciPointerInputEnvError> {
    if value.len() > POINTER_INPUT_RAMFB_DELAY_ENV_MAX_BYTES {
        return Err(XhciPointerInputEnvError::RamfbDelayTooLong {
            len: value.len(),
            max: POINTER_INPUT_RAMFB_DELAY_ENV_MAX_BYTES,
        });
    }
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(XhciPointerInputEnvError::RamfbDelayEmpty);
    }
    let mut delays_ms = Vec::new();
    for token in trimmed
        .split([',', ' ', '\t', '\n'])
        .filter(|token| !token.is_empty())
    {
        let Ok(delay_ms) = token.parse::<u64>() else {
            return Err(XhciPointerInputEnvError::RamfbDelayInvalid {
                token: token.to_string(),
            });
        };
        delays_ms.push(delay_ms);
    }
    pointer_ramfb_delay_checkpoints_from_ms(&delays_ms)
}

fn default_pointer_ramfb_delay_checkpoints() -> Vec<PointerRamfbDelayCheckpoint> {
    POINTER_INPUT_RAMFB_DEFAULT_DELAY_MS
        .iter()
        .map(|delay_ms| pointer_ramfb_delay_checkpoint(*delay_ms))
        .collect()
}

fn pointer_ramfb_delay_checkpoints_from_ms(
    delays_ms: &[u64],
) -> Result<Vec<PointerRamfbDelayCheckpoint>, XhciPointerInputEnvError> {
    if delays_ms.is_empty() {
        return Err(XhciPointerInputEnvError::RamfbDelayEmpty);
    }
    let mut unique_delays_ms = Vec::new();
    for delay_ms in delays_ms {
        validate_pointer_ramfb_delay_ms(*delay_ms)?;
        if unique_delays_ms.contains(delay_ms) {
            return Err(XhciPointerInputEnvError::RamfbDelayDuplicate {
                delay_ms: *delay_ms,
            });
        }
        unique_delays_ms.push(*delay_ms);
        if unique_delays_ms.len() > POINTER_INPUT_RAMFB_MAX_DELAYS {
            return Err(XhciPointerInputEnvError::RamfbDelayTooMany {
                requested: unique_delays_ms.len(),
                max: POINTER_INPUT_RAMFB_MAX_DELAYS,
            });
        }
    }
    Ok(unique_delays_ms
        .iter()
        .map(|delay_ms| pointer_ramfb_delay_checkpoint(*delay_ms))
        .collect())
}

fn validate_pointer_ramfb_delay_ms(delay_ms: u64) -> Result<(), XhciPointerInputEnvError> {
    if delay_ms == 0 {
        return Err(XhciPointerInputEnvError::RamfbDelayInvalid {
            token: delay_ms.to_string(),
        });
    }
    if delay_ms > POINTER_INPUT_RAMFB_MAX_DELAY_MS {
        return Err(XhciPointerInputEnvError::RamfbDelayTooLarge {
            requested_ms: delay_ms,
            max_ms: POINTER_INPUT_RAMFB_MAX_DELAY_MS,
        });
    }
    Ok(())
}

fn pointer_ramfb_delay_checkpoint(delay_ms: u64) -> PointerRamfbDelayCheckpoint {
    PointerRamfbDelayCheckpoint {
        label: format!("pointer-input-delay-{delay_ms}ms"),
        delay: Duration::from_millis(delay_ms),
        emitted: false,
    }
}

fn pointer_center() -> PointerPosition {
    debug_assert_eq!(PointerPosition::center().x(), POINTER_INPUT_AXIS_CENTER);
    PointerPosition::center()
}

fn format_pointer_action_names(actions: &[PointerInputAction]) -> String {
    actions
        .iter()
        .map(|action| match *action {
            PointerInputAction::Move(position) => {
                format!("move:{}x{}", position.x(), position.y())
            }
            PointerInputAction::Click(position) => {
                format!("click:{}x{}", position.x(), position.y())
            }
            PointerInputAction::Press { position, buttons } => {
                format!("press{buttons}:{}x{}", position.x(), position.y())
            }
            PointerInputAction::Release(position) => {
                format!("release:{}x{}", position.x(), position.y())
            }
            PointerInputAction::Scroll { position, delta } => {
                format!("scroll:{delta}@{}x{}", position.x(), position.y())
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn pointer_queue_error_name(error: XhciPointerInputQueueError) -> &'static str {
    match error {
        XhciPointerInputQueueError::EmptySequence => "empty_sequence",
        XhciPointerInputQueueError::TooManyActions { .. } => "too_many_actions",
        XhciPointerInputQueueError::Busy => "busy",
    }
}

fn emitted_reports(stats: XhciPointerInputReportStats) -> u64 {
    stats
        .emitted_move_reports
        .saturating_add(stats.emitted_button_reports)
        .saturating_add(stats.emitted_release_reports)
        .saturating_add(stats.emitted_wheel_reports)
}
