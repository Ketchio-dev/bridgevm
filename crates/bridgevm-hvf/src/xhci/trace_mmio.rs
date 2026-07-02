use std::cell::RefCell;
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ReadAccess {
    offset: u64,
    size: u8,
    value: u64,
}

thread_local! {
    static PENDING_READ: RefCell<Option<(ReadAccess, u64)>> = const { RefCell::new(None) };
}

fn mmio_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_TRACE_XHCI_MMIO").ok().as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
        )
    })
}

pub(crate) fn mmio_read(offset: u64, size: u8, value: u64) {
    if !mmio_trace_enabled() {
        return;
    }
    let access = ReadAccess {
        offset,
        size,
        value,
    };
    PENDING_READ.with(|pending| {
        let mut pending = pending.borrow_mut();
        match pending.as_mut() {
            Some((last, extra_repeats)) if *last == access => {
                *extra_repeats += 1;
            }
            _ => {
                if let Some(line) = flushed_repeat_line(pending.take()) {
                    println!("{line}");
                }
                println!(
                    "{}",
                    format_mmio_read(access.offset, access.size, access.value)
                );
                *pending = Some((access, 0));
            }
        }
    });
}

pub(crate) fn mmio_write(offset: u64, size: u8, value: u64) {
    if !mmio_trace_enabled() {
        return;
    }
    PENDING_READ.with(|pending| {
        if let Some(line) = flushed_repeat_line(pending.borrow_mut().take()) {
            println!("{line}");
        }
    });
    println!("{}", format_mmio_write(offset, size, value));
}

fn flushed_repeat_line(pending: Option<(ReadAccess, u64)>) -> Option<String> {
    match pending {
        Some((access, extra_repeats)) if extra_repeats > 0 => Some(format_mmio_read_repeated(
            access.offset,
            access.size,
            access.value,
            extra_repeats,
        )),
        _ => None,
    }
}

fn format_mmio_read(offset: u64, size: u8, value: u64) -> String {
    format!("XHCI MMIO read offset={offset:#x} size={size} value={value:#x}")
}

fn format_mmio_read_repeated(offset: u64, size: u8, value: u64, extra_repeats: u64) -> String {
    format!(
        "XHCI MMIO read repeated offset={offset:#x} size={size} value={value:#x} extra_repeats={extra_repeats}"
    )
}

fn format_mmio_write(offset: u64, size: u8, value: u64) -> String {
    format!("XHCI MMIO write offset={offset:#x} size={size} value={value:#x}")
}

#[cfg(test)]
pub(super) fn assert_mmio_trace_format_includes_parseable_access() {
    let read = format_mmio_read(0x44, 4, 0x1);
    assert_eq!(read, "XHCI MMIO read offset=0x44 size=4 value=0x1");

    let repeated = format_mmio_read_repeated(0x44, 4, 0x1, 42);
    assert_eq!(
        repeated,
        "XHCI MMIO read repeated offset=0x44 size=4 value=0x1 extra_repeats=42"
    );

    let write = format_mmio_write(0x40, 4, 0x2);
    assert_eq!(write, "XHCI MMIO write offset=0x40 size=4 value=0x2");
}

#[cfg(test)]
pub(super) fn assert_mmio_read_repeat_flush_summarizes_extra_repeats() {
    assert_eq!(flushed_repeat_line(None), None);
    let access = ReadAccess {
        offset: 0x44,
        size: 4,
        value: 0x1,
    };
    assert_eq!(flushed_repeat_line(Some((access, 0))), None);
    assert_eq!(
        flushed_repeat_line(Some((access, 3))),
        Some("XHCI MMIO read repeated offset=0x44 size=4 value=0x1 extra_repeats=3".to_string())
    );
}
