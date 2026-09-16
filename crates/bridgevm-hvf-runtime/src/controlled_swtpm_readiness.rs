//! One shared startup deadline covers key delivery and control readiness.

use super::super::*;
use std::time::{Duration, Instant};

pub(crate) fn wait_for_sockets(
    process: &mut SwtpmProcess,
    deadline: Instant,
    control: &RuntimeControl<'_>,
) -> Result<(), RuntimeError> {
    loop {
        controlled::io::check(deadline, control).map_err(|source| RuntimeError::Io {
            context: "swtpm socket wait",
            source,
        })?;
        controlled::require_live(process, "swtpm exited before creating its sockets", control)?;
        if process.data_socket.exists()
            && controlled::io::probe(&process.control_socket, deadline, control).map_err(
                |source| RuntimeError::Io {
                    context: "swtpm socket wait",
                    source,
                },
            )?
        {
            return controlled::require_live(
                process,
                "swtpm exited after creating its sockets",
                control,
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
