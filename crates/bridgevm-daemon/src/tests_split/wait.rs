//! Bounded waiting shared by the daemon tests. Split out of helpers.rs.

/// Poll `condition` for ten seconds, returning whether it ever held.
///
/// A bare `for _ in 0..N` loop that falls through on timeout turns a slow
/// child process into a later assertion failure that names the state rather
/// than the wait. Ten seconds only bounds a failure: a healthy reconcile
/// settles in milliseconds, and two seconds was not enough under an eight-way
/// parallel test load.
pub(super) fn wait_up_to_ten_seconds(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..400 {
        if condition() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    false
}
