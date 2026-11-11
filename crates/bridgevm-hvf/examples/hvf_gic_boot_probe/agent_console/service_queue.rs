//! User/control and clipboard traffic keeps its order ahead of background file transfer:
//! shared-folder sync, Finder drops, large clipboard file copies and print jobs, which take turns
//! in arrival order.

use super::*;

/// The next request to send; one `held` returns true for stays queued, in
/// its place.
pub(super) fn pop_next_request(
    queue: &mut VecDeque<ServiceReq>,
    held: impl Fn(&ServiceReq) -> bool,
) -> Option<ServiceReq> {
    let position = queue
        .iter()
        .position(|request| {
            !held(request)
                && !is_share_req(request)
                && !is_drop_req(request)
                && !is_large_clip_files_req(request)
                && !is_print_req(request)
        })
        .or_else(|| queue.iter().position(|request| !held(request)))?;
    queue.remove(position)
}

pub(super) fn has_foreground_capacity(queue: &VecDeque<ServiceReq>) -> bool {
    queue
        .iter()
        .filter(|request| !is_share_req(request))
        .take(MAX_SERVICE_QUEUE_DEPTH)
        .count()
        < MAX_SERVICE_QUEUE_DEPTH
}

#[cfg(test)]
#[path = "service_queue_tests.rs"]
mod tests;
