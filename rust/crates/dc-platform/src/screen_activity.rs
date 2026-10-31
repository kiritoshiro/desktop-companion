//! Port of app/screen_activity.py: which monitors are really in use, so
//! missions only populate those.
//!
//! Two read-only checks: Windows' own flags (`EnumDisplayDevices`: a monitor
//! Windows no longer drives is not marked active) and the monitor itself
//! over DDC/CI (VCP 0xD6 power mode through dxva2). Monitors that cannot be
//! asked count as active. The main screen is always used.

use std::collections::{HashMap, HashSet};

use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
    GetVCPFeatureAndVCPFeatureReply, PHYSICAL_MONITOR,
};
use windows::Win32::Graphics::Gdi::{DISPLAY_DEVICEW, EnumDisplayDevicesW, HMONITOR};
use windows::core::PCWSTR;

use crate::monitors::monitors;
use crate::win::from_wide;

/// VCP 0xD6 power mode: 1 on, 2 standby, 3 suspend, 4 off (DPM), 5 off.
pub const POWER_ON: u32 = 1;
pub const VCP_POWER_MODE: u8 = 0xD6;
/// Adapter: part of the desktop; monitor: active.
pub const ATTACHED: u32 = 0x0000_0001;

/// A monitor's state as the Python strings ("active", "asleep", "off").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MonitorState {
    Active,
    Asleep,
    Off,
}

impl MonitorState {
    pub fn as_str(self) -> &'static str {
        match self {
            MonitorState::Active => "active",
            MonitorState::Asleep => "asleep",
            MonitorState::Off => "off",
        }
    }
}

/// Merge the Windows flags with DDC power modes (`monitor_states`' rule).
pub fn merge_states(
    flags: HashMap<String, MonitorState>,
    modes: &HashMap<String, u32>,
) -> HashMap<String, MonitorState> {
    let mut states = flags;
    for (name, &mode) in modes {
        let current = states.get(name).copied().unwrap_or(MonitorState::Active);
        if mode != POWER_ON && current == MonitorState::Active {
            states.insert(name.clone(), if mode == 2 || mode == 3 { MonitorState::Asleep } else { MonitorState::Off });
        } else {
            states.entry(name.clone()).or_insert(MonitorState::Active);
        }
    }
    states
}

/// Screen name (`\\.\DISPLAY2`) -> state. Monitors that cannot be asked are
/// left out, meaning active. DDC/CI is slow (tens of ms per monitor), so
/// call this once per mission, not per frame.
pub fn monitor_states() -> HashMap<String, MonitorState> {
    merge_states(windows_flags(), &ddc_power_modes())
}

/// The screens a mission may populate, as indices into `names`: the main
/// one always; the others when active and not switched off by the player.
pub fn playable_screens(
    names: &[String],
    primary: usize,
    disabled: &HashSet<String>,
    states: &HashMap<String, MonitorState>,
) -> Vec<usize> {
    names
        .iter()
        .enumerate()
        .filter(|(i, name)| {
            *i == primary
                || (!disabled.contains(*name)
                    && states.get(*name).copied().unwrap_or(MonitorState::Active) == MonitorState::Active)
        })
        .map(|(i, _)| i)
        .collect()
}

fn display_device() -> DISPLAY_DEVICEW {
    DISPLAY_DEVICEW { cb: std::mem::size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() }
}

/// `EnumDisplayDevices` flags: an attached adapter with monitors is
/// "active" if any of its monitors is attached, else "off".
pub fn windows_flags() -> HashMap<String, MonitorState> {
    let mut states = HashMap::new();
    let mut i = 0;
    loop {
        let mut adapter = display_device();
        if !unsafe { EnumDisplayDevicesW(PCWSTR::null(), i, &mut adapter, 0) }.as_bool() {
            break;
        }
        i += 1;
        if adapter.StateFlags.0 & ATTACHED == 0 {
            continue;
        }
        let (mut j, mut any_active, mut any_monitor) = (0, false, false);
        loop {
            let mut monitor = display_device();
            if !unsafe { EnumDisplayDevicesW(PCWSTR(adapter.DeviceName.as_ptr()), j, &mut monitor, 0) }.as_bool() {
                break;
            }
            j += 1;
            any_monitor = true;
            any_active = any_active || monitor.StateFlags.0 & ATTACHED != 0;
        }
        if any_monitor {
            states.insert(
                from_wide(&adapter.DeviceName),
                if any_active { MonitorState::Active } else { MonitorState::Off },
            );
        }
    }
    states
}

/// VCP 0xD6 of each monitor that answers over DDC/CI.
pub fn ddc_power_modes() -> HashMap<String, u32> {
    let mut modes = HashMap::new();
    for m in monitors() {
        let hmonitor = HMONITOR(m.handle as *mut core::ffi::c_void);
        let mut count = 0u32;
        if unsafe { GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count) }.is_err() || count == 0 {
            continue;
        }
        let mut physical = vec![PHYSICAL_MONITOR::default(); count as usize];
        if unsafe { GetPhysicalMonitorsFromHMONITOR(hmonitor, &mut physical) }.is_err() {
            continue;
        }
        for p in &physical {
            let mut current = 0u32;
            let mut maximum = 0u32;
            let ok = unsafe {
                GetVCPFeatureAndVCPFeatureReply(
                    p.hPhysicalMonitor,
                    VCP_POWER_MODE,
                    None,
                    &mut current,
                    Some(&mut maximum),
                )
            };
            if ok != 0 {
                modes.insert(m.name.clone(), current);
            }
        }
        unsafe {
            let _ = DestroyPhysicalMonitors(&physical);
        }
    }
    modes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["\\\\.\\DISPLAY1", "\\\\.\\DISPLAY2", "\\\\.\\DISPLAY3"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ddc_modes_override_only_active_flags() {
        let mut flags = HashMap::new();
        flags.insert("A".to_string(), MonitorState::Active);
        flags.insert("B".to_string(), MonitorState::Off);
        let modes: HashMap<String, u32> =
            [("A".to_string(), 2), ("B".to_string(), 1), ("C".to_string(), 4), ("D".to_string(), 1)].into();
        let s = merge_states(flags, &modes);
        assert_eq!(s["A"], MonitorState::Asleep);
        assert_eq!(s["B"], MonitorState::Off, "Windows' own 'off' wins");
        assert_eq!(s["C"], MonitorState::Off);
        assert_eq!(s["D"], MonitorState::Active);
        assert_eq!(MonitorState::Asleep.as_str(), "asleep");
    }

    #[test]
    fn the_main_screen_is_always_playable() {
        let n = names();
        let mut states = HashMap::new();
        states.insert(n[0].clone(), MonitorState::Off);
        states.insert(n[1].clone(), MonitorState::Asleep);
        assert_eq!(playable_screens(&n, 0, &HashSet::new(), &states), vec![0, 2]);
        let disabled: HashSet<String> = [n[2].clone()].into();
        assert_eq!(playable_screens(&n, 0, &disabled, &states), vec![0]);
        assert_eq!(playable_screens(&n, 2, &disabled, &HashMap::new()), vec![0, 1, 2]);
    }

    #[test]
    fn smoke_flags_name_display_devices() {
        for name in windows_flags().keys() {
            assert!(name.starts_with("\\\\.\\"), "{name}");
        }
    }
}
