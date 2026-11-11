use super::*;

fn on_battery(percent: u8, minutes_to_empty: Option<u32>) -> HostPower {
    HostPower {
        ac_online: false,
        battery: Some(HostBattery {
            percent,
            charging: false,
            minutes_to_empty,
            minutes_to_full: None,
        }),
    }
}

fn dword(page: &HostPowerPage, reg: usize) -> u64 {
    page.mmio_read(4 * reg as u64, 4)
}

#[test]
fn a_discharging_mac_encodes_bst_and_bif_in_mwh() {
    // 57 % of 60 Wh with macOS estimating 3 h 25 min left: 34.2 Wh at 10 W.
    let page = HostPowerPage::new(on_battery(57, Some(205)));
    assert_eq!(dword(&page, REG_FLAGS), u64::from(FLAG_BATTERY_PRESENT));
    assert_eq!(dword(&page, REG_STATE), u64::from(STATE_DISCHARGING));
    assert_eq!(dword(&page, REG_REMAINING_MWH), 34_200);
    assert_eq!(dword(&page, REG_RATE_MW), 34_200 * 60 / 205);
    assert_eq!(dword(&page, REG_VOLTAGE_MV), 11_400);
    assert_eq!(dword(&page, REG_DESIGN_CAPACITY_MWH), 60_000);
    assert_eq!(dword(&page, REG_FULL_CAPACITY_MWH), 60_000);
    assert_eq!(dword(&page, REG_DESIGN_VOLTAGE_MV), 11_400);
    assert_eq!(dword(&page, REG_WARNING_CAPACITY_MWH), 6_000);
    assert_eq!(dword(&page, REG_LOW_CAPACITY_MWH), 3_000);
}

#[test]
fn charging_on_the_adapter_and_held_charge_and_no_estimate() {
    let charging = HostPower {
        ac_online: true,
        battery: Some(HostBattery {
            percent: 80,
            charging: true,
            minutes_to_empty: None,
            minutes_to_full: Some(30),
        }),
    };
    let page = HostPowerPage::new(charging);
    assert_eq!(
        dword(&page, REG_FLAGS),
        u64::from(FLAG_AC_ONLINE | FLAG_BATTERY_PRESENT)
    );
    assert_eq!(dword(&page, REG_STATE), u64::from(STATE_CHARGING));
    // The missing 12 Wh in half an hour.
    assert_eq!(dword(&page, REG_RATE_MW), 24_000);

    // macOS holding the charge at 80 % on the adapter: neither state, rate 0.
    let mut held = charging;
    held.battery.as_mut().unwrap().charging = false;
    let page = HostPowerPage::new(held);
    assert_eq!(dword(&page, REG_STATE), 0);
    assert_eq!(dword(&page, REG_RATE_MW), 0);

    // "Calculating" after an unplug: the rate is unknown, not zero.
    let page = HostPowerPage::new(on_battery(100, None));
    assert_eq!(dword(&page, REG_RATE_MW), u64::from(UNKNOWN));
    assert_eq!(dword(&page, REG_REMAINING_MWH), 60_000);
}

#[test]
fn no_battery_clears_the_present_flag_and_keeps_a_valid_bif() {
    let page = HostPowerPage::new(HostPower {
        ac_online: true,
        battery: None,
    });
    assert_eq!(dword(&page, REG_FLAGS), u64::from(FLAG_AC_ONLINE));
    assert_eq!(dword(&page, REG_REMAINING_MWH), u64::from(UNKNOWN));
    assert_eq!(dword(&page, REG_DESIGN_CAPACITY_MWH), 60_000);
}

#[test]
fn reads_are_bounded_to_the_registers() {
    let page = HostPowerPage::new(on_battery(57, Some(205)));
    // 60_000 = 0x0000_EA60, little-endian.
    let design = 4 * REG_DESIGN_CAPACITY_MWH as u64;
    assert_eq!(page.mmio_read(design, 1), 0x60);
    assert_eq!(page.mmio_read(design + 1, 2), 0xEA);
    // An 8-byte read spans two registers: design and full capacity.
    assert_eq!(page.mmio_read(design, 8), 60_000 | 60_000 << 32);
    // The last register's tail and everything after it read zero.
    assert_eq!(page.mmio_read(PAGE_BYTES as u64 - 2, 4), 0);
    assert_eq!(page.mmio_read(PAGE_BYTES as u64, 4), 0);
    assert_eq!(page.mmio_read(0xFFC, 4), 0);
    assert_eq!(page.mmio_read(u64::MAX, 8), 0);
    // Odd sizes read zero.
    assert_eq!(page.mmio_read(design, 3), 0);
    assert_eq!(page.mmio_read(design, 0), 0);
    assert_eq!(page.mmio_read(design, 16), 0);
}

#[test]
fn update_reports_which_acpi_objects_changed() {
    let mut page = HostPowerPage::new(on_battery(57, Some(205)));
    assert_eq!(page.update(on_battery(57, Some(205))), 0);
    // A new percent or estimate is a _BST change only.
    assert_eq!(
        page.update(on_battery(56, Some(205))),
        GED_EVENT_BATTERY_STATUS
    );
    assert_eq!(
        page.update(on_battery(56, Some(190))),
        GED_EVENT_BATTERY_STATUS
    );
    // Plugging in is a _PSR and a _BST change.
    let mut plugged = on_battery(56, Some(190));
    plugged.ac_online = true;
    assert_eq!(
        page.update(plugged),
        GED_EVENT_AC_ADAPTER | GED_EVENT_BATTERY_STATUS
    );
    // The battery going away is a _STA change, and _BST reads unknown.
    let gone = HostPower {
        ac_online: true,
        battery: None,
    };
    assert_eq!(
        page.update(gone),
        GED_EVENT_BATTERY_STATUS | GED_EVENT_BATTERY_INFO
    );
    // Unplugged without a battery: the adapter alone.
    let unplugged = HostPower {
        ac_online: false,
        battery: None,
    };
    assert_eq!(page.update(unplugged), GED_EVENT_AC_ADAPTER);
    assert_eq!(
        page,
        HostPowerPage::new(unplugged),
        "the page shows the latest reading"
    );
}
