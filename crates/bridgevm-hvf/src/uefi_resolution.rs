//! Default guest console resolution. The bundled firmware's ramfb driver offers
//! 640x480, 800x600 and 1024x768 and starts at 800x600; Windows' Basic Display
//! Adapter keeps whatever mode the firmware left. OVMF's `PlatformConfig`
//! variable (the firmware setup's "Preferred Resolution") makes the console
//! pick 1024x768 instead, so a VM without one gets it added. A `PlatformConfig`
//! the user saved in the firmware setup is kept.

/// The largest mode the bundled ramfb driver offers.
pub const DEFAULT_RESOLUTION: (u32, u32) = (1024, 768);

const AUTH_VARIABLE_STORE_GUID: [u8; 16] = guid(
    0xaaf3_2c78,
    0x947b,
    0x439a,
    [0xa1, 0x80, 0x2e, 0x14, 0x4e, 0xc3, 0x77, 0x92],
);
const OVMF_PLATFORM_CONFIG_GUID: [u8; 16] = guid(
    0x7235_c51c,
    0x0c80,
    0x4cab,
    [0x87, 0xac, 0x3b, 0x08, 0x4a, 0x63, 0x04, 0xb1],
);
const VARIABLE_STORE_HEADER_LEN: usize = 28;
const VARIABLE_HEADER_LEN: usize = 60;
const START_ID: u16 = 0x55aa;
const VAR_ADDED: u8 = 0x3f;
const VAR_ADDED_IN_DELETED_TRANSITION: u8 = 0x3e;
/// Non-volatile, boot-service and runtime access, as the firmware saves it.
const NV_BS_RT: u32 = 0x7;

const fn guid(a: u32, b: u16, c: u16, d: [u8; 8]) -> [u8; 16] {
    let a = a.to_le_bytes();
    let b = b.to_le_bytes();
    let c = c.to_le_bytes();
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6],
        d[7],
    ]
}

/// `vars` with a `PlatformConfig` of [`DEFAULT_RESOLUTION`] appended, or `None`
/// when it is not an EDK2 authenticated variable store, already has a live
/// `PlatformConfig`, or has no room.
pub fn with_default_resolution(vars: &[u8]) -> Option<Vec<u8>> {
    let free = first_free_record_unless_configured(vars)?;
    let record = platform_config_record(DEFAULT_RESOLUTION);
    let end = free.checked_add(record.len())?;
    let store_end = store_end(vars)?;
    if end > store_end || vars[free..end].iter().any(|&b| b != 0xff) {
        return None;
    }
    let mut out = vars.to_vec();
    out[free..end].copy_from_slice(&record);
    Some(out)
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Offset of the variable store header inside the firmware volume.
fn store_start(vars: &[u8]) -> Option<usize> {
    if vars.get(0x28..0x2c)? != b"_FVH" {
        return None;
    }
    let start = usize::from(read_u16(vars, 0x30)?);
    (vars.get(start..start + 16)? == AUTH_VARIABLE_STORE_GUID).then_some(start)
}

fn store_end(vars: &[u8]) -> Option<usize> {
    let start = store_start(vars)?;
    let size = usize::try_from(read_u32(vars, start + 16)?).ok()?;
    Some(start.checked_add(size)?.min(vars.len()))
}

fn first_free_record_unless_configured(vars: &[u8]) -> Option<usize> {
    let end = store_end(vars)?;
    let mut at = store_start(vars)? + VARIABLE_STORE_HEADER_LEN;
    while at + VARIABLE_HEADER_LEN <= end && read_u16(vars, at)? == START_ID {
        let state = vars[at + 2];
        let name_len = usize::try_from(read_u32(vars, at + 36)?).ok()?;
        let data_len = usize::try_from(read_u32(vars, at + 40)?).ok()?;
        let name = vars.get(at + VARIABLE_HEADER_LEN..at + VARIABLE_HEADER_LEN + name_len)?;
        let live = state == VAR_ADDED || state == VAR_ADDED_IN_DELETED_TRANSITION;
        if live
            && vars.get(at + 44..at + 60)? == OVMF_PLATFORM_CONFIG_GUID
            && name == utf16z("PlatformConfig")
        {
            return None;
        }
        at = (at + VARIABLE_HEADER_LEN + name_len + data_len).checked_add(3)? & !3;
    }
    Some(at)
}

fn utf16z(name: &str) -> Vec<u8> {
    name.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn platform_config_record((width, height): (u32, u32)) -> Vec<u8> {
    let name = utf16z("PlatformConfig");
    let data: Vec<u8> = [width, height]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let mut record = Vec::with_capacity(VARIABLE_HEADER_LEN + name.len() + data.len() + 3);
    record.extend_from_slice(&START_ID.to_le_bytes());
    record.extend_from_slice(&[VAR_ADDED, 0]);
    record.extend_from_slice(&NV_BS_RT.to_le_bytes());
    record.extend_from_slice(&[0; 8 + 16 + 4]); // MonotonicCount, TimeStamp, PubKeyIndex
    record.extend_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_le_bytes());
    record.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_le_bytes());
    record.extend_from_slice(&OVMF_PLATFORM_CONFIG_GUID);
    record.extend_from_slice(&name);
    record.extend_from_slice(&data);
    while record.len() % 4 != 0 {
        record.push(0xff);
    }
    record
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty EDK2 authenticated store: FV header, store header, erased space.
    fn empty_store(len: usize) -> Vec<u8> {
        let mut vars = vec![0xff; len];
        vars[..0x48].fill(0);
        vars[0x28..0x2c].copy_from_slice(b"_FVH");
        vars[0x30..0x32].copy_from_slice(&0x48u16.to_le_bytes());
        vars[0x48..0x58].copy_from_slice(&AUTH_VARIABLE_STORE_GUID);
        let size = u32::try_from(len - 0x48).unwrap();
        vars[0x58..0x5c].copy_from_slice(&size.to_le_bytes());
        vars[0x5c..0x64].fill(0);
        vars
    }

    fn platform_config(vars: &[u8]) -> Option<(u32, u32)> {
        let start = 0x48 + VARIABLE_STORE_HEADER_LEN;
        let name = utf16z("PlatformConfig");
        let mut at = start;
        while read_u16(vars, at)? == START_ID {
            let name_len = read_u32(vars, at + 36)? as usize;
            let data_len = read_u32(vars, at + 40)? as usize;
            if vars[at + 2] == VAR_ADDED && vars[at + 60..at + 60 + name_len] == name[..] {
                assert_eq!(read_u32(vars, at + 4), Some(NV_BS_RT));
                let data = at + 60 + name_len;
                return Some((read_u32(vars, data)?, read_u32(vars, data + 4)?));
            }
            at = (at + 60 + name_len + data_len + 3) & !3;
        }
        None
    }

    #[test]
    fn a_store_without_a_preferred_resolution_gets_1024x768_after_its_variables() {
        let mut vars = empty_store(0x2000);
        let other = {
            let mut r = platform_config_record((1, 1));
            r[60..60 + 30].copy_from_slice(&utf16z("OtherVariable1"));
            r
        };
        vars[0x64..0x64 + other.len()].copy_from_slice(&other);

        let seeded = with_default_resolution(&vars).expect("seeded");
        assert_eq!(platform_config(&seeded), Some(DEFAULT_RESOLUTION));
        assert_eq!(
            &seeded[..0x64 + other.len()],
            &vars[..0x64 + other.len()],
            "nothing before it moves"
        );
        assert_eq!(
            with_default_resolution(&seeded),
            None,
            "a live PlatformConfig is kept"
        );
    }

    #[test]
    fn a_users_own_resolution_and_foreign_bytes_are_left_alone() {
        let mut vars = empty_store(0x2000);
        let users = platform_config_record((800, 600));
        vars[0x64..0x64 + users.len()].copy_from_slice(&users);
        assert_eq!(with_default_resolution(&vars), None);

        assert_eq!(with_default_resolution(b"saved-vars"), None);
        assert_eq!(with_default_resolution(&empty_store(0x80)), None, "no room");
    }
}
