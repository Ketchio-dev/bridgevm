use super::{union_rect, Rect};

pub(super) fn merge_pending(
    pending: Option<(u32, Rect)>,
    resource_id: u32,
    rect: Rect,
    fresh: bool,
) -> ((u32, Rect), bool) {
    match pending {
        Some((pending_id, pending_rect)) if pending_id == resource_id => {
            ((resource_id, union_rect(pending_rect, rect)), fresh)
        }
        _ => ((resource_id, rect), true),
    }
}
