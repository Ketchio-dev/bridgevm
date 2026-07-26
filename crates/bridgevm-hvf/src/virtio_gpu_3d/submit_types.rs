// Bounded renderer submission results carried to JSONL diagnostics.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Submit3dDiagnostic {
    pub renderer_status: i32,
    pub command_offset_dwords: Option<u32>,
    pub command_id: Option<u32>,
    pub command_header: Option<u32>,
    pub resource_id: Option<u32>,
    pub resource_found: Option<bool>,
    pub resource_backed: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Submit3dResult {
    pub accepted: bool,
    pub diagnostic: Option<Submit3dDiagnostic>,
}

impl Submit3dResult {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            diagnostic: None,
        }
    }

    pub fn rejected(renderer_status: i32) -> Self {
        Self {
            accepted: false,
            diagnostic: Some(Submit3dDiagnostic {
                renderer_status,
                ..Submit3dDiagnostic::default()
            }),
        }
    }
}
