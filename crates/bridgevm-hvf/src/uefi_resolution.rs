//! Guest desktop resolution. The bundled firmware's ramfb driver offers the
//! modes in [`RESOLUTIONS`] (firmware patch 0002) and starts at 800x600;
//! Windows' Basic Display Adapter keeps whatever mode the firmware left. OVMF's
//! `PlatformConfig` variable (the firmware setup's "Preferred Resolution") makes
//! the console pick another mode: a VM without one gets [`DEFAULT_RESOLUTION`],
//! a `PlatformConfig` the user saved in the firmware setup is kept, and the
//! user's choice in the app ([`with_resolution`]) replaces it.

/// The modes the bundled ramfb driver offers, in its order.
pub const RESOLUTIONS: [(u32, u32); 15] = [
    (640, 480),
    (800, 600),
    (1024, 768),
    (1280, 720),
    (1280, 800),
    (1280, 1024),
    (1366, 768),
    (1440, 900),
    (1600, 900),
    (1600, 1200),
    (1680, 1050),
    (1920, 1080),
    (1920, 1200),
    (2560, 1440),
    (2560, 1600),
];

/// A VM's resolution until the user picks one: 16:10 like Mac displays, and
/// small enough for the window to show it 1:1 on a 13-inch MacBook.
pub const DEFAULT_RESOLUTION: (u32, u32) = (1280, 800);

/// `"1920x1080"` as a firmware mode; anything else is `None`.
pub fn parse_resolution(text: &str) -> Option<(u32, u32)> {
    let (width, height) = text.trim().split_once('x')?;
    let mode = (width.parse().ok()?, height.parse().ok()?);
    RESOLUTIONS.contains(&mode).then_some(mode)
}

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
/// Cleared in `State` when a variable is deleted (flash bits only clear).
const VAR_DELETED: u8 = 0xfd;
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
    let scan = scan(vars)?;
    if scan.live.is_some() {
        return None;
    }
    append(vars, scan.free, DEFAULT_RESOLUTION)
}

/// `vars` whose live `PlatformConfig` asks for `resolution`: a different one
/// is marked deleted and the new one appended, as the firmware's own variable
/// driver does. `None` when it already asks for `resolution`, `vars` is not an
/// EDK2 authenticated variable store, or the store has no room.
pub fn with_resolution(vars: &[u8], resolution: (u32, u32)) -> Option<Vec<u8>> {
    let scan = scan(vars)?;
    if let Some((at, current)) = scan.live {
        if current == Some(resolution) {
            return None;
        }
        let mut out = append(vars, scan.free, resolution)?;
        out[at + 2] &= VAR_DELETED;
        return Some(out);
    }
    append(vars, scan.free, resolution)
}

fn append(vars: &[u8], free: usize, resolution: (u32, u32)) -> Option<Vec<u8>> {
    let record = platform_config_record(resolution);
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

struct Scan {
    /// Offset and resolution (when 8 bytes) of the live `PlatformConfig`.
    live: Option<(usize, Option<(u32, u32)>)>,
    /// Where the erased space after the last variable starts.
    free: usize,
}

fn scan(vars: &[u8]) -> Option<Scan> {
    let end = store_end(vars)?;
    let mut at = store_start(vars)? + VARIABLE_STORE_HEADER_LEN;
    let mut live_config = None;
    while at + VARIABLE_HEADER_LEN <= end && read_u16(vars, at)? == START_ID {
        let state = vars[at + 2];
        let name_len = usize::try_from(read_u32(vars, at + 36)?).ok()?;
        let data_len = usize::try_from(read_u32(vars, at + 40)?).ok()?;
        let name_at = at + VARIABLE_HEADER_LEN;
        let name = vars.get(name_at..name_at + name_len)?;
        let live = state == VAR_ADDED || state == VAR_ADDED_IN_DELETED_TRANSITION;
        if live
            && vars.get(at + 44..at + 60)? == OVMF_PLATFORM_CONFIG_GUID
            && name == utf16z("PlatformConfig")
        {
            let data = name_at + name_len;
            let value = (data_len == 8)
                .then(|| Some((read_u32(vars, data)?, read_u32(vars, data + 4)?)))
                .flatten();
            live_config = Some((at, value));
        }
        at = (name_at + name_len + data_len).checked_add(3)? & !3;
    }
    Some(Scan {
        live: live_config,
        free: at,
    })
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
    fn a_store_without_a_preferred_resolution_gets_the_default_after_its_variables() {
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
    fn the_users_choice_replaces_the_preferred_resolution_once() {
        let vars = with_default_resolution(&empty_store(0x2000)).expect("seeded");
        let chosen = with_resolution(&vars, (1920, 1080)).expect("replaced");
        assert_eq!(platform_config(&chosen), Some((1920, 1080)));
        let old = 0x48 + VARIABLE_STORE_HEADER_LEN;
        assert_eq!(
            chosen[old + 2],
            VAR_ADDED & VAR_DELETED,
            "the old one is deleted"
        );
        assert_eq!(
            with_resolution(&chosen, (1920, 1080)),
            None,
            "already chosen"
        );
        let again = with_resolution(&chosen, (1280, 800)).expect("changed again");
        assert_eq!(platform_config(&again), Some((1280, 800)));
        assert_eq!(
            with_resolution(&empty_store(0x2000), (2560, 1600)).map(|v| platform_config(&v)),
            Some(Some((2560, 1600))),
            "a store without one gets it"
        );
        assert_eq!(with_resolution(b"saved-vars", (1920, 1080)), None);
    }

    #[test]
    fn only_the_firmwares_modes_parse() {
        assert_eq!(parse_resolution("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_resolution(" 1280x800\n"), Some((1280, 800)));
        assert_eq!(parse_resolution("1920x1081"), None);
        assert_eq!(parse_resolution("3840x2160"), None);
        assert_eq!(parse_resolution("1920"), None);
        assert!(RESOLUTIONS.contains(&DEFAULT_RESOLUTION));
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
