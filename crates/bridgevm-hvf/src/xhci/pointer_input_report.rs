use super::XhciController;

pub(super) const HID_ABSOLUTE_POINTER_REPORT_LEN: u32 = 5;
pub const XHCI_HID_ABSOLUTE_POINTER_REPORT_BYTES: u32 = HID_ABSOLUTE_POINTER_REPORT_LEN;
const POINTER_AXIS_MAX: u16 = 0x7fff;
const MAX_POINTER_INPUT_ACTIONS: usize = 16;
const MAX_POINTER_INPUT_REPORTS: usize = MAX_POINTER_INPUT_ACTIONS * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointerPosition {
    x: u16,
    y: u16,
}

impl PointerPosition {
    pub const fn new(x: u16, y: u16) -> Option<Self> {
        if x <= POINTER_AXIS_MAX && y <= POINTER_AXIS_MAX {
            Some(Self { x, y })
        } else {
            None
        }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }

    pub const fn center() -> Self {
        Self {
            x: POINTER_AXIS_MAX / 2,
            y: POINTER_AXIS_MAX / 2,
        }
    }

    const fn report(self, buttons: u8) -> [u8; 5] {
        let x = self.x.to_le_bytes();
        let y = self.y.to_le_bytes();
        [buttons, x[0], x[1], y[0], y[1]]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerInputAction {
    Move(PointerPosition),
    Click(PointerPosition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XhciPointerInputQueueError {
    EmptySequence,
    TooManyActions { requested: usize, max: usize },
    Busy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct XhciPointerInputReportStats {
    pub controller_reset_generation: u64,
    pub queued_actions: u64,
    pub queued_reports: u64,
    pub emitted_move_reports: u64,
    pub emitted_button_reports: u64,
    pub emitted_release_reports: u64,
    pub empty_sequence_rejections: u64,
    pub too_many_action_rejections: u64,
    pub busy_rejections: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PointerInputReportKind {
    Move,
    Button,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PointerInputReport {
    position: PointerPosition,
    kind: PointerInputReportKind,
}

impl PointerInputReport {
    const EMPTY: Self = Self {
        position: PointerPosition::center(),
        kind: PointerInputReportKind::Move,
    };

    pub(super) const fn bytes(self) -> [u8; 5] {
        match self.kind {
            PointerInputReportKind::Move | PointerInputReportKind::Release => {
                self.position.report(0)
            }
            PointerInputReportKind::Button => self.position.report(1),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct PointerInputReportQueue {
    reports: [PointerInputReport; MAX_POINTER_INPUT_REPORTS],
    head: usize,
    len: usize,
    current_position: PointerPosition,
}

impl Default for PointerInputReportQueue {
    fn default() -> Self {
        Self {
            reports: [PointerInputReport::EMPTY; MAX_POINTER_INPUT_REPORTS],
            head: 0,
            len: 0,
            current_position: PointerPosition::center(),
        }
    }
}

impl PointerInputReportQueue {
    fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn peek(&self) -> Option<PointerInputReport> {
        (!self.is_empty()).then_some(self.reports[self.head])
    }

    pub(super) fn idle_report(&self) -> [u8; 5] {
        self.current_position.report(0)
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

    fn record_emitted_position(&mut self, report: PointerInputReport) {
        self.current_position = report.position;
    }

    fn queue_actions(
        &mut self,
        actions: &[PointerInputAction],
    ) -> Result<usize, XhciPointerInputQueueError> {
        if actions.is_empty() {
            return Err(XhciPointerInputQueueError::EmptySequence);
        }
        if actions.len() > MAX_POINTER_INPUT_ACTIONS {
            return Err(XhciPointerInputQueueError::TooManyActions {
                requested: actions.len(),
                max: MAX_POINTER_INPUT_ACTIONS,
            });
        }
        if !self.is_empty() {
            return Err(XhciPointerInputQueueError::Busy);
        }
        for action in actions {
            match *action {
                PointerInputAction::Move(position) => {
                    self.push_back(PointerInputReport {
                        position,
                        kind: PointerInputReportKind::Move,
                    });
                }
                PointerInputAction::Click(position) => {
                    self.push_back(PointerInputReport {
                        position,
                        kind: PointerInputReportKind::Button,
                    });
                    self.push_back(PointerInputReport {
                        position,
                        kind: PointerInputReportKind::Release,
                    });
                }
            }
        }
        Ok(self.len)
    }

    fn push_back(&mut self, report: PointerInputReport) {
        let index = (self.head + self.len) % self.reports.len();
        self.reports[index] = report;
        self.len += 1;
    }
}

impl XhciController {
    pub fn queue_pointer_input_actions(
        &mut self,
        actions: &[PointerInputAction],
    ) -> Result<(), XhciPointerInputQueueError> {
        match self.pointer_input_report_queue.queue_actions(actions) {
            Ok(queued_reports) => {
                self.pointer_input_report_stats.queued_actions = self
                    .pointer_input_report_stats
                    .queued_actions
                    .saturating_add(actions.len() as u64);
                self.pointer_input_report_stats.queued_reports = self
                    .pointer_input_report_stats
                    .queued_reports
                    .saturating_add(queued_reports as u64);
                Ok(())
            }
            Err(error) => {
                self.record_pointer_input_queue_rejection(error);
                Err(error)
            }
        }
    }

    pub fn pointer_input_report_stats(&self) -> XhciPointerInputReportStats {
        self.pointer_input_report_stats
    }

    pub(super) fn has_queued_pointer_input_report(&self) -> bool {
        !self.pointer_input_report_queue.is_empty()
    }

    pub(super) fn record_pointer_input_report_emitted(&mut self, report: PointerInputReport) {
        match report.kind {
            PointerInputReportKind::Move => {
                self.pointer_input_report_stats.emitted_move_reports = self
                    .pointer_input_report_stats
                    .emitted_move_reports
                    .saturating_add(1);
            }
            PointerInputReportKind::Button => {
                self.pointer_input_report_stats.emitted_button_reports = self
                    .pointer_input_report_stats
                    .emitted_button_reports
                    .saturating_add(1);
            }
            PointerInputReportKind::Release => {
                self.pointer_input_report_stats.emitted_release_reports = self
                    .pointer_input_report_stats
                    .emitted_release_reports
                    .saturating_add(1);
            }
        }
        self.pointer_input_report_queue
            .record_emitted_position(report);
    }

    fn record_pointer_input_queue_rejection(&mut self, error: XhciPointerInputQueueError) {
        match error {
            XhciPointerInputQueueError::EmptySequence => {
                self.pointer_input_report_stats.empty_sequence_rejections = self
                    .pointer_input_report_stats
                    .empty_sequence_rejections
                    .saturating_add(1);
            }
            XhciPointerInputQueueError::TooManyActions { .. } => {
                self.pointer_input_report_stats.too_many_action_rejections = self
                    .pointer_input_report_stats
                    .too_many_action_rejections
                    .saturating_add(1);
            }
            XhciPointerInputQueueError::Busy => {
                self.pointer_input_report_stats.busy_rejections = self
                    .pointer_input_report_stats
                    .busy_rejections
                    .saturating_add(1);
            }
        }
    }
}
