//! User/control and clipboard traffic keeps its order ahead of background file transfer:
//! shared-folder sync and Finder drops, which take turns in arrival order.

use super::*;

pub(super) fn pop_next_request(queue: &mut VecDeque<ServiceReq>) -> Option<ServiceReq> {
    let position = queue
        .iter()
        .position(|request| !is_share_req(request) && !is_drop_req(request))
        .unwrap_or(0);
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
