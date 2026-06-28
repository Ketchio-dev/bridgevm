use super::{trace, XhciController};

pub(super) const HID_BOOT_KEYBOARD_REPORT_LEN: u32 = 8;
pub(super) const HID_BOOT_KEYBOARD_NO_KEY_REPORT: [u8; 8] = [0; 8];
const MAX_SETUP_INPUT_ACTIONS: usize = 8;
const MAX_SETUP_INPUT_REPORTS: usize = MAX_SETUP_INPUT_ACTIONS * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupInputAction {
    Tab,
    Enter,
    Space,
}

impl SetupInputAction {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Enter => "enter",
            Self::Space => "space",
        }
    }

    pub const fn usage(self) -> u8 {
        match self {
            Self::Tab => 0x2b,
            Self::Enter => 0x28,
            Self::Space => 0x2c,
        }
    }

    const fn key_report(self) -> [u8; 8] {
        [0, 0, self.usage(), 0, 0, 0, 0, 0]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XhciSetupInputQueueError {
    EmptySequence,
    TooManyActions { requested: usize, max: usize },
    Busy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XhciSetupInputReportStats {
    pub queued_actions: u64,
    pub queued_reports: u64,
    pub emitted_key_reports: u64,
    pub emitted_release_reports: u64,
    pub empty_sequence_rejections: u64,
    pub too_many_action_rejections: u64,
    pub busy_rejections: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SetupInputReportKind {
    Key,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SetupInputReport {
    action: SetupInputAction,
    kind: SetupInputReportKind,
}

impl SetupInputReport {
    const EMPTY: Self = Self {
        action: SetupInputAction::Space,
        kind: SetupInputReportKind::Release,
    };

    pub(super) const fn bytes(self) -> [u8; 8] {
        match self.kind {
            SetupInputReportKind::Key => self.action.key_report(),
            SetupInputReportKind::Release => HID_BOOT_KEYBOARD_NO_KEY_REPORT,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct BootKeyboardReportQueue {
    reports: [SetupInputReport; MAX_SETUP_INPUT_REPORTS],
    head: usize,
    len: usize,
}

impl Default for BootKeyboardReportQueue {
    fn default() -> Self {
        Self {
            reports: [SetupInputReport::EMPTY; MAX_SETUP_INPUT_REPORTS],
            head: 0,
            len: 0,
        }
    }
}

impl BootKeyboardReportQueue {
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn peek(&self) -> Option<SetupInputReport> {
        if self.len == 0 {
            None
        } else {
            Some(self.reports[self.head])
        }
    }

    pub(super) fn pop_front(&mut self) {
        if self.len == 0 {
            return;
        }
        self.head = (self.head + 1) % self.reports.len();
        self.len -= 1;
        if self.len == 0 {
            self.head = 0;
        }
    }

    pub(super) fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }

    fn queue_actions(
        &mut self,
        actions: &[SetupInputAction],
    ) -> Result<usize, XhciSetupInputQueueError> {
        if actions.is_empty() {
            return Err(XhciSetupInputQueueError::EmptySequence);
        }
        if actions.len() > MAX_SETUP_INPUT_ACTIONS {
            return Err(XhciSetupInputQueueError::TooManyActions {
                requested: actions.len(),
                max: MAX_SETUP_INPUT_ACTIONS,
            });
        }
        if !self.is_empty() {
            return Err(XhciSetupInputQueueError::Busy);
        }
        for action in actions {
            self.push_back(SetupInputReport {
                action: *action,
                kind: SetupInputReportKind::Key,
            });
            self.push_back(SetupInputReport {
                action: *action,
                kind: SetupInputReportKind::Release,
            });
        }
        Ok(actions.len() * 2)
    }

    fn push_back(&mut self, report: SetupInputReport) {
        let index = (self.head + self.len) % self.reports.len();
        self.reports[index] = report;
        self.len += 1;
    }
}

impl XhciController {
    pub fn queue_boot_keyboard_space(&mut self) -> bool {
        self.queue_setup_input_actions(&[SetupInputAction::Space])
            .is_ok()
    }

    pub fn queue_setup_input_actions(
        &mut self,
        actions: &[SetupInputAction],
    ) -> Result<(), XhciSetupInputQueueError> {
        match self.boot_keyboard_report_queue.queue_actions(actions) {
            Ok(queued_reports) => {
                self.setup_input_report_stats.queued_actions = self
                    .setup_input_report_stats
                    .queued_actions
                    .saturating_add(actions.len() as u64);
                self.setup_input_report_stats.queued_reports = self
                    .setup_input_report_stats
                    .queued_reports
                    .saturating_add(queued_reports as u64);
                for action in actions {
                    trace::setup_input_action_queued(
                        action.name(),
                        action.usage(),
                        action.key_report(),
                        HID_BOOT_KEYBOARD_NO_KEY_REPORT,
                        self.setup_input_report_stats.queued_actions,
                        self.setup_input_report_stats.queued_reports,
                    );
                }
                Ok(())
            }
            Err(error) => {
                self.record_setup_input_queue_rejection(error);
                Err(error)
            }
        }
    }

    pub fn setup_input_report_stats(&self) -> XhciSetupInputReportStats {
        self.setup_input_report_stats
    }

    pub(super) fn has_queued_setup_input_report(&self) -> bool {
        !self.boot_keyboard_report_queue.is_empty()
    }

    pub(super) fn record_setup_input_report_emitted(
        &mut self,
        queued_report: SetupInputReport,
        report: [u8; 8],
        trb_gpa: u64,
        buffer_gpa: u64,
    ) {
        match queued_report.kind {
            SetupInputReportKind::Key => {
                self.setup_input_report_stats.emitted_key_reports = self
                    .setup_input_report_stats
                    .emitted_key_reports
                    .saturating_add(1);
            }
            SetupInputReportKind::Release => {
                self.setup_input_report_stats.emitted_release_reports = self
                    .setup_input_report_stats
                    .emitted_release_reports
                    .saturating_add(1);
            }
        }
        trace::setup_input_report_emitted(trace::SetupInputReportEmittedTrace {
            action: queued_report.action.name(),
            usage: queued_report.action.usage(),
            report_kind: queued_report.kind.name(),
            report,
            dci3_trb_gpa: trb_gpa,
            buffer_gpa,
            emitted_key_reports: self.setup_input_report_stats.emitted_key_reports,
            emitted_release_reports: self.setup_input_report_stats.emitted_release_reports,
        });
    }

    fn record_setup_input_queue_rejection(&mut self, error: XhciSetupInputQueueError) {
        match error {
            XhciSetupInputQueueError::EmptySequence => {
                self.setup_input_report_stats.empty_sequence_rejections = self
                    .setup_input_report_stats
                    .empty_sequence_rejections
                    .saturating_add(1);
            }
            XhciSetupInputQueueError::TooManyActions { .. } => {
                self.setup_input_report_stats.too_many_action_rejections = self
                    .setup_input_report_stats
                    .too_many_action_rejections
                    .saturating_add(1);
            }
            XhciSetupInputQueueError::Busy => {
                self.setup_input_report_stats.busy_rejections = self
                    .setup_input_report_stats
                    .busy_rejections
                    .saturating_add(1);
            }
        }
    }
}

impl SetupInputReportKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Release => "release",
        }
    }
}
