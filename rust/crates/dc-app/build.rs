//! Embeds `assets/ui/app_icon.ico` as the exe's icon, as the PyInstaller
//! spec's `icon='assets/ui/app_icon.ico'` does for the Python build.
//!
//! No resource compiler is needed: the script writes the compiled resource
//! file (`.res`) itself, one `RT_ICON` per image in the `.ico` plus the
//! `RT_GROUP_ICON` (id 1) that lists them, and hands it to the MSVC linker,
//! which takes `.res` files as inputs. Other targets get no icon.

use std::path::PathBuf;

/// `RT_ICON` and `RT_GROUP_ICON`.
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
/// The icon group's id: what Explorer shows, and `LoadIconW(hinstance, 1)`.
const APP_ICON_ID: u16 = 1;
/// US English, what `rc.exe` writes by default.
const LANGUAGE: u16 = 0x0409;
/// `MOVEABLE | DISCARDABLE` (and `PURE` for the group), as `rc.exe` writes.
const ICON_FLAGS: u16 = 0x1010;
const GROUP_FLAGS: u16 = 0x1030;

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let ico = manifest.join("../../../assets/ui/app_icon.ico");
    println!("cargo:rerun-if-changed={}", ico.display());
    println!("cargo:rerun-if-changed=build.rs");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }
    let bytes = std::fs::read(&ico).unwrap_or_else(|e| panic!("{}: {e}", ico.display()));
    let res = icon_resources(&bytes).unwrap_or_else(|e| panic!("{}: {e}", ico.display()));
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("app_icon.res");
    std::fs::write(&out, res).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    // Binaries only: the exe (and its test harness).
    println!("cargo:rustc-link-arg-bins={}", out.display());
}

/// The `.res` file for one `.ico` (`ICONDIR` + `ICONDIRENTRY`s + images).
fn icon_resources(ico: &[u8]) -> Result<Vec<u8>, String> {
    let word = |at: usize| ico.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let dword = |at: usize| ico.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if word(0) != Some(0) || word(2) != Some(1) {
        return Err("not an .ico file".into());
    }
    let count = word(4).ok_or("truncated header")? as usize;
    if count == 0 {
        return Err("no images".into());
    }
    let mut res = Vec::new();
    // Every .res file starts with an empty resource.
    push_resource(&mut res, 0, 0, 0, &[]);
    let mut group = Vec::with_capacity(6 + count * 14);
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let entry = 6 + i * 16;
        let head = ico.get(entry..entry + 8).ok_or("truncated directory")?;
        let size = dword(entry + 8).ok_or("truncated directory")? as usize;
        let offset = dword(entry + 12).ok_or("truncated directory")? as usize;
        let image = ico.get(offset..offset + size).ok_or("image outside the file")?;
        let id = i as u16 + 1;
        push_resource(&mut res, RT_ICON, id, ICON_FLAGS, image);
        // GRPICONDIRENTRY: the ICONDIRENTRY's first 12 bytes, then the id.
        group.extend_from_slice(head);
        group.extend_from_slice(&(size as u32).to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    push_resource(&mut res, RT_GROUP_ICON, APP_ICON_ID, GROUP_FLAGS, &group);
    Ok(res)
}

/// One `RESOURCEHEADER` (ordinal type and name) and its data, padded to
/// a DWORD boundary.
fn push_resource(res: &mut Vec<u8>, kind: u16, name: u16, flags: u16, data: &[u8]) {
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&[0xFF, 0xFF]);
    res.extend_from_slice(&kind.to_le_bytes());
    res.extend_from_slice(&[0xFF, 0xFF]);
    res.extend_from_slice(&name.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    res.extend_from_slice(&flags.to_le_bytes());
    res.extend_from_slice(&(if kind == 0 { 0 } else { LANGUAGE }).to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes()); // Version
    res.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
    res.extend_from_slice(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}
