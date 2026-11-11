//! The Mac's power sources through IOKit's IOPowerSources API.

use std::ffi::{c_char, c_void};
use std::ptr::null_mut;
use std::time::Duration;

use bridgevm_hvf::host_power::{HostBattery, HostPower};

/// What one power source's description dictionary says (IOPSKeys.h), with
/// missing or mistyped values as `None`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct SourceFields {
    /// `Type`: `InternalBattery`, `UPS`, ...
    pub(super) kind: Option<String>,
    /// `Is Present`.
    pub(super) present: Option<bool>,
    /// `Power Source State`: `AC Power`, `Battery Power` or `Off Line`.
    pub(super) state: Option<String>,
    /// `Current Capacity`, out of `Max Capacity` (100 on a Mac: a percent).
    pub(super) current: Option<i64>,
    pub(super) max: Option<i64>,
    /// `Is Charging`.
    pub(super) charging: Option<bool>,
    /// `Time to Empty` / `Time to Full Charge`, minutes; -1 while macOS
    /// calculates.
    pub(super) time_to_empty: Option<i64>,
    pub(super) time_to_full: Option<i64>,
}

/// The reading for these sources: the first present internal battery, or no
/// battery (and the adapter online) when there is none.
pub(super) fn host_power_from_sources(sources: &[SourceFields]) -> HostPower {
    let battery = sources.iter().find(|source| {
        source.kind.as_deref() == Some("InternalBattery") && source.present != Some(false)
    });
    let Some((source, current)) = battery.and_then(|source| Some((source, source.current?))) else {
        return HostPower {
            ac_online: true,
            battery: None,
        };
    };
    let max = source.max.filter(|max| *max > 0).unwrap_or(100);
    let percent = u8::try_from(current.clamp(0, max) * 100 / max).unwrap_or(100);
    let minutes = |value: Option<i64>| value.and_then(|m| u32::try_from(m).ok()).filter(|m| *m > 0);
    HostPower {
        ac_online: source.state.as_deref() != Some("Battery Power"),
        battery: Some(HostBattery {
            percent,
            charging: source.charging == Some(true),
            minutes_to_empty: minutes(source.time_to_empty),
            minutes_to_full: minutes(source.time_to_full),
        }),
    }
}

/// One reading of the Mac's power sources; `None` when IOKit has none to give.
pub(super) fn read() -> Option<HostPower> {
    // SAFETY: Copy results are released here; Get results are borrowed from
    // the blob, which outlives their use.
    unsafe {
        let blob = IOPSCopyPowerSourcesInfo();
        if blob.is_null() {
            return None;
        }
        let list = IOPSCopyPowerSourcesList(blob);
        let mut sources = Vec::new();
        if !list.is_null() {
            for index in 0..CFArrayGetCount(list) {
                let description =
                    IOPSGetPowerSourceDescription(blob, CFArrayGetValueAtIndex(list, index));
                if !description.is_null() {
                    sources.push(fields_from_description(description));
                }
            }
            CFRelease(list);
        }
        CFRelease(blob);
        Some(host_power_from_sources(&sources))
    }
}

/// # Safety
/// `description` is a live CFDictionary.
pub(super) unsafe fn fields_from_description(description: *const c_void) -> SourceFields {
    let value = |key: &str| dictionary_value(description, key);
    SourceFields {
        kind: cf_string(value("Type")),
        present: cf_bool(value("Is Present")),
        state: cf_string(value("Power Source State")),
        current: cf_i64(value("Current Capacity")),
        max: cf_i64(value("Max Capacity")),
        charging: cf_bool(value("Is Charging")),
        time_to_empty: cf_i64(value("Time to Empty")),
        time_to_full: cf_i64(value("Time to Full Charge")),
    }
}

/// A new CFString the caller releases (null on failure).
pub(super) unsafe fn cf_string_from(text: &str) -> *const c_void {
    CFStringCreateWithBytes(null_mut(), text.as_ptr(), text.len() as isize, UTF8, 0)
}

unsafe fn dictionary_value(dictionary: *const c_void, key: &str) -> *const c_void {
    let key = cf_string_from(key);
    if key.is_null() {
        return null_mut();
    }
    let value = CFDictionaryGetValue(dictionary, key);
    CFRelease(key);
    value
}

unsafe fn cf_string(value: *const c_void) -> Option<String> {
    if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
        return None;
    }
    let mut buffer = [0 as c_char; 128];
    if CFStringGetCString(value, buffer.as_mut_ptr(), buffer.len() as isize, UTF8) == 0 {
        return None;
    }
    std::ffi::CStr::from_ptr(buffer.as_ptr())
        .to_str()
        .ok()
        .map(str::to_owned)
}

unsafe fn cf_i64(value: *const c_void) -> Option<i64> {
    if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
        return None;
    }
    let mut out = 0i64;
    (CFNumberGetValue(value, NUMBER_SINT64, (&mut out as *mut i64).cast()) != 0).then_some(out)
}

unsafe fn cf_bool(value: *const c_void) -> Option<bool> {
    if value.is_null() || CFGetTypeID(value) != CFBooleanGetTypeID() {
        return None;
    }
    Some(CFBooleanGetValue(value) != 0)
}

/// macOS' power-source change notification on the calling thread's run loop.
pub(super) struct ChangeWait {
    source: *mut c_void,
}

impl ChangeWait {
    pub(super) fn install() -> Self {
        // SAFETY: the source is retained by us and by the run loop it joins.
        let source = unsafe { IOPSNotificationCreateRunLoopSource(on_change, null_mut()) };
        if !source.is_null() {
            unsafe { CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopDefaultMode) };
        }
        Self { source }
    }

    /// Until macOS posts a power-source change (true) or `timeout` passes.
    pub(super) fn wait(&self, timeout: Duration) -> bool {
        if self.source.is_null() {
            std::thread::sleep(timeout);
            return true;
        }
        // SAFETY: runs this thread's run loop, which holds our source.
        let result = unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, timeout.as_secs_f64(), 1) };
        result == RUN_LOOP_HANDLED_SOURCE
    }
}

impl Drop for ChangeWait {
    fn drop(&mut self) {
        if !self.source.is_null() {
            // SAFETY: invalidating removes it from the run loop; then our reference goes.
            unsafe {
                CFRunLoopSourceInvalidate(self.source);
                CFRelease(self.source);
            }
        }
    }
}

/// The run loop returning is the signal; the read happens on the loop's thread.
extern "C" fn on_change(_context: *mut c_void) {}

/// `kCFRunLoopRunHandledSource`: our source fired.
const RUN_LOOP_HANDLED_SOURCE: i32 = 4;
const UTF8: u32 = 0x0800_0100;
const NUMBER_SINT64: isize = 4;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> *const c_void;
    fn IOPSCopyPowerSourcesList(blob: *const c_void) -> *const c_void;
    fn IOPSGetPowerSourceDescription(blob: *const c_void, source: *const c_void) -> *const c_void;
    fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(*mut c_void),
        context: *mut c_void,
    ) -> *mut c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: *const c_void;
    fn CFRelease(object: *const c_void);
    fn CFGetTypeID(object: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    fn CFStringCreateWithBytes(
        alloc: *const c_void,
        bytes: *const u8,
        num_bytes: isize,
        encoding: u32,
        is_external: u8,
    ) -> *const c_void;
    fn CFStringGetCString(
        string: *const c_void,
        buffer: *mut c_char,
        size: isize,
        encoding: u32,
    ) -> u8;
    fn CFNumberGetValue(number: *const c_void, kind: isize, out: *mut c_void) -> u8;
    fn CFBooleanGetValue(boolean: *const c_void) -> u8;
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopRunInMode(
        mode: *const c_void,
        seconds: f64,
        return_after_source_handled: u8,
    ) -> i32;
    fn CFRunLoopSourceInvalidate(source: *mut c_void);
}
