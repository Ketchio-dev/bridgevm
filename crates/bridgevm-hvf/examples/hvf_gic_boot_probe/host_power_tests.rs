use super::mac::{fields_from_description, host_power_from_sources, SourceFields};
use super::*;
use std::ffi::c_void;
use std::ptr::null;

/// A CFDictionary like one `IOPSGetPowerSourceDescription` returns.
enum Value {
    Text(&'static str),
    Number(i64),
    Flag(bool),
}

fn description(entries: &[(&str, Value)]) -> SourceFields {
    // SAFETY: every created CF object is released after the read.
    unsafe {
        let mut keys = Vec::new();
        let mut values = Vec::new();
        for (key, value) in entries {
            keys.push(mac::cf_string_from(key));
            values.push(match value {
                Value::Text(text) => mac::cf_string_from(text),
                Value::Number(number) => CFNumberCreate(null(), 4, (number as *const i64).cast()),
                Value::Flag(true) => CFRetain(kCFBooleanTrue),
                Value::Flag(false) => CFRetain(kCFBooleanFalse),
            });
        }
        let dictionary = CFDictionaryCreate(
            null(),
            keys.as_ptr(),
            values.as_ptr(),
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        assert!(!dictionary.is_null());
        let fields = fields_from_description(dictionary);
        for object in keys.into_iter().chain(values).chain([dictionary]) {
            CFRelease(object);
        }
        fields
    }
}

/// What a MacBook on battery reports (`pmset -g ps -xml` shape).
fn macbook(state: &'static str, charging: bool, current: i64) -> Vec<(&'static str, Value)> {
    vec![
        ("Type", Value::Text("InternalBattery")),
        ("Transport Type", Value::Text("Internal")),
        ("Name", Value::Text("InternalBattery-0")),
        ("Is Present", Value::Flag(true)),
        ("Power Source State", Value::Text(state)),
        ("Current Capacity", Value::Number(current)),
        ("Max Capacity", Value::Number(100)),
        ("Is Charging", Value::Flag(charging)),
        ("Is Charged", Value::Flag(false)),
        (
            "Time to Empty",
            Value::Number(if charging { 0 } else { 205 }),
        ),
        (
            "Time to Full Charge",
            Value::Number(if charging { 47 } else { -1 }),
        ),
        ("BatteryHealth", Value::Text("Good")),
    ]
}

#[test]
fn a_macbook_power_source_dictionary_becomes_a_battery_reading() {
    let fields = description(&macbook("Battery Power", false, 57));
    assert_eq!(fields.kind.as_deref(), Some("InternalBattery"));
    assert_eq!(fields.current, Some(57));
    assert_eq!(
        host_power_from_sources(&[fields]),
        HostPower {
            ac_online: false,
            battery: Some(HostBattery {
                percent: 57,
                charging: false,
                minutes_to_empty: Some(205),
                minutes_to_full: None,
            }),
        }
    );

    let charging = description(&macbook("AC Power", true, 80));
    assert_eq!(
        host_power_from_sources(&[charging]),
        HostPower {
            ac_online: true,
            battery: Some(HostBattery {
                percent: 80,
                charging: true,
                minutes_to_empty: None,
                minutes_to_full: Some(47),
            }),
        }
    );
}

#[test]
fn mistyped_or_missing_values_read_as_absent() {
    let fields = description(&[
        ("Type", Value::Number(3)),
        ("Is Present", Value::Text("yes")),
        ("Current Capacity", Value::Flag(true)),
    ]);
    assert_eq!(fields, SourceFields::default());
}

#[test]
fn a_mac_without_an_internal_battery_has_none() {
    let desktop = HostPower {
        ac_online: true,
        battery: None,
    };
    // Mac mini or Studio: no power sources at all.
    assert_eq!(host_power_from_sources(&[]), desktop);
    // A USB UPS is not the Mac's battery.
    let ups = description(&[
        ("Type", Value::Text("UPS")),
        ("Power Source State", Value::Text("Battery Power")),
        ("Current Capacity", Value::Number(90)),
    ]);
    assert_eq!(host_power_from_sources(&[ups]), desktop);
    // An internal battery that is not present, or reports no capacity.
    let mut absent = description(&macbook("AC Power", false, 50));
    absent.present = Some(false);
    assert_eq!(host_power_from_sources(&[absent]), desktop);
    let mut no_capacity = description(&macbook("AC Power", false, 50));
    no_capacity.current = None;
    assert_eq!(host_power_from_sources(&[no_capacity]), desktop);
}

#[test]
fn the_fake_battery_is_a_triple_or_a_file_holding_one() {
    let fake = |percent, charging, ac_online| HostPower {
        ac_online,
        battery: Some(HostBattery {
            percent,
            charging,
            minutes_to_empty: None,
            minutes_to_full: None,
        }),
    };
    assert_eq!(parse_fake("80,0,0"), Some(fake(80, false, false)));
    assert_eq!(parse_fake(" 100 , 1 , true\n"), Some(fake(100, true, true)));
    for bad in ["", "80", "80,0", "101,0,1", "-1,0,1", "80,2,1", "80,0,1,0"] {
        assert_eq!(parse_fake(bad), None, "{bad:?}");
    }

    assert_eq!(PowerSource::from_value(None), PowerSource::Mac);
    assert_eq!(PowerSource::from_value(Some(" ")), PowerSource::Mac);
    assert_eq!(PowerSource::from_value(Some("nonsense")), PowerSource::Mac);
    assert_eq!(
        PowerSource::from_value(Some("42,1,1")),
        PowerSource::Fixed(fake(42, true, true))
    );

    let path = std::env::temp_dir().join(format!("bvm-fake-battery-{}", std::process::id()));
    let source = PowerSource::from_value(path.to_str());
    assert_eq!(source, PowerSource::File(path.clone()));
    assert_eq!(source.read(), None, "no file yet");
    std::fs::write(&path, "80,0,1\n").unwrap();
    assert_eq!(source.read(), Some(fake(80, false, true)));
    std::fs::write(&path, "35,0,0").unwrap();
    assert_eq!(source.read(), Some(fake(35, false, false)));
    std::fs::write(&path, "35,0,0".repeat(100)).unwrap();
    assert_eq!(source.read(), None, "an oversized file is not a reading");
    std::fs::remove_file(&path).unwrap();
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: *const c_void;
    static kCFBooleanFalse: *const c_void;
    static kCFTypeDictionaryKeyCallBacks: [usize; 6];
    static kCFTypeDictionaryValueCallBacks: [usize; 5];
    fn CFRetain(object: *const c_void) -> *const c_void;
    fn CFRelease(object: *const c_void);
    fn CFNumberCreate(alloc: *const c_void, kind: isize, value: *const c_void) -> *const c_void;
    fn CFDictionaryCreate(
        alloc: *const c_void,
        keys: *const *const c_void,
        values: *const *const c_void,
        count: isize,
        key_callbacks: *const [usize; 6],
        value_callbacks: *const [usize; 5],
    ) -> *const c_void;
}
