use anyhow::{Context, Result};
use std::ffi::c_void;
use windows::core::{Interface, GUID};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

// IVirtualDesktopManager COM interface
const CLSID_VIRTUAL_DESKTOP_MANAGER: GUID = GUID::from_values(
    0xAA509086,
    0x5CA9,
    0x4C25,
    [0x8F, 0x95, 0x58, 0x9D, 0x3C, 0x07, 0xB4, 0x8A],
);

const IID_IVIRTUAL_DESKTOP_MANAGER: GUID = GUID::from_values(
    0xA5CD92FF,
    0x29BE,
    0x454C,
    [0x8D, 0x04, 0xD8, 0x28, 0x79, 0xFB, 0x3F, 0x1B],
);

// IVirtualDesktopManager vtable layout (IUnknown + 3 methods)
#[repr(C)]
struct IVirtualDesktopManagerVtbl {
    // IUnknown
    query_interface: *const c_void,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    // IVirtualDesktopManager
    is_window_on_current_virtual_desktop:
        unsafe extern "system" fn(*mut c_void, HWND, *mut i32) -> i32,
    get_window_desktop_id: unsafe extern "system" fn(*mut c_void, HWND, *mut GUID) -> i32,
    move_window_to_desktop: unsafe extern "system" fn(*mut c_void, HWND, *const GUID) -> i32,
}

pub struct VirtualDesktopManager {
    ptr: *mut c_void,
    vtbl: *const IVirtualDesktopManagerVtbl,
}

// The COM pointer is thread-safe for our usage (single-threaded calls).
unsafe impl Send for VirtualDesktopManager {}
unsafe impl Sync for VirtualDesktopManager {}

impl VirtualDesktopManager {
    pub fn new() -> Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let mut ptr: *mut c_void = std::ptr::null_mut();
            let hr = windows::Win32::System::Com::CoCreateInstance::<_, windows::core::IUnknown>(
                &CLSID_VIRTUAL_DESKTOP_MANAGER,
                None,
                CLSCTX_ALL,
            );
            let unknown = hr.context("Failed to create IVirtualDesktopManager")?;

            // QueryInterface for IVirtualDesktopManager
            unknown
                .query(&IID_IVIRTUAL_DESKTOP_MANAGER, &mut ptr)
                .ok()
                .context("QueryInterface for IVirtualDesktopManager failed")?;

            if ptr.is_null() {
                anyhow::bail!("CoCreateInstance returned null pointer");
            }

            // The vtable pointer is the first field of the COM object
            let vtbl = *(ptr as *const *const IVirtualDesktopManagerVtbl);

            Ok(Self { ptr, vtbl })
        }
    }

    pub fn get_desktop_id(&self, hwnd: HWND) -> Result<GUID> {
        unsafe {
            let mut guid = GUID::zeroed();
            let hr = ((*self.vtbl).get_window_desktop_id)(self.ptr, hwnd, &mut guid);
            if hr < 0 {
                anyhow::bail!("GetWindowDesktopId failed with HRESULT 0x{:08X}", hr as u32);
            }
            Ok(guid)
        }
    }

    pub fn move_to_desktop(&self, hwnd: HWND, desktop_id: &GUID) -> Result<()> {
        unsafe {
            let hr = ((*self.vtbl).move_window_to_desktop)(self.ptr, hwnd, desktop_id);
            if hr < 0 {
                anyhow::bail!("MoveWindowToDesktop failed with HRESULT 0x{:08X}", hr as u32);
            }
            Ok(())
        }
    }
}

impl Drop for VirtualDesktopManager {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                ((*self.vtbl).release)(self.ptr);
            }
        }
    }
}

/// Get the virtual desktop index for a window (0-based).
pub fn get_window_desktop_index(hwnd: HWND) -> Result<u32> {
    let vdm = VirtualDesktopManager::new()?;
    let target_guid = vdm.get_desktop_id(hwnd)?;

    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let vd_key = hkcu
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops")?;
    let ids_bytes: Vec<u8> = vd_key.get_raw_value("VirtualDesktopIDs")?.bytes;

    let guid_size = 16;
    let count = ids_bytes.len() / guid_size;

    for i in 0..count {
        let offset = i * guid_size;
        let slice = &ids_bytes[offset..offset + guid_size];

        let guid = GUID::from_values(
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]),
            u16::from_le_bytes([slice[4], slice[5]]),
            u16::from_le_bytes([slice[6], slice[7]]),
            [
                slice[8], slice[9], slice[10], slice[11], slice[12], slice[13], slice[14],
                slice[15],
            ],
        );

        if guid == target_guid {
            return Ok(i as u32);
        }
    }

    anyhow::bail!("Desktop GUID not found in registry")
}

/// Get the GUID of the virtual desktop at the given index.
pub fn get_desktop_guid_by_index(index: u32) -> Result<GUID> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let vd_key = hkcu
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\VirtualDesktops")?;
    let ids_bytes: Vec<u8> = vd_key.get_raw_value("VirtualDesktopIDs")?.bytes;

    let guid_size = 16;
    let offset = (index as usize) * guid_size;

    if offset + guid_size > ids_bytes.len() {
        anyhow::bail!(
            "Virtual desktop index {} does not exist (only {} desktops)",
            index,
            ids_bytes.len() / guid_size
        );
    }

    let slice = &ids_bytes[offset..offset + guid_size];
    Ok(GUID::from_values(
        u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]),
        u16::from_le_bytes([slice[4], slice[5]]),
        u16::from_le_bytes([slice[6], slice[7]]),
        [
            slice[8], slice[9], slice[10], slice[11], slice[12], slice[13], slice[14], slice[15],
        ],
    ))
}
