//! User/control and clipboard traffic runs ahead of background file transfer:
//! shared-folder sync, Finder drops, large clipboard file copies and print jobs, which take turns
//! in arrival order. A pending text copy is a dependency of its paste shortcut.

use super::*;

/// The next request to send; one `held` returns true for stays queued, in
/// its place.
pub(super) fn pop_next_request(
    queue: &mut VecDeque<ServiceReq>,
    held: impl Fn(&ServiceReq) -> bool,
) -> Option<ServiceReq> {
    let mut position = queue
        .iter()
        .position(|request| {
            !held(request)
                && !is_share_req(request)
                && !is_drop_req(request)
                && !is_large_clip_files_req(request)
                && !is_print_req(request)
        })
        .or_else(|| queue.iter().position(|request| !held(request)))?;
    // The pasteboard poll can publish the Mac copy after its shortcut was
    // queued. Complete that copy's CLIPSET before inserting Ctrl+V; a wire
    // receipt establishes clipboard delivery, not application consumption.
    if is_paste_key(&queue[position]) {
        if let Some(copy) = queue.iter().position(|request| matches!(request, ServiceReq::ClipPush(..))) {
            if held(&queue[copy]) {
                return None;
            }
            position = copy;
        }
    }
    queue.remove(position)
}

fn is_paste_key(request: &ServiceReq) -> bool {
    let ServiceReq::Ctl(command) = request else {
        return false;
    };
    let Some((_, payload)) = command.strip_prefix("KEYINPUT ").and_then(|value| value.split_once(' ')) else {
        return false;
    };
    if payload.len() > 32 {
        return false;
    }
    matches!(
        base64_decode(payload).as_deref(),
        Ok(b"ctrl+v" | b"down:ctrl+v" | b"shift+insert" | b"down:shift+insert")
    )
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
