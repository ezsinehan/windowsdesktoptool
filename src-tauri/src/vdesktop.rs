use anyhow::{Context, Result};
use std::ffi::c_void;
use windows::core::{Interface, GUID};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED,
};

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

    // `move_to_desktop` via the public IVirtualDesktopManager is intentionally
    // not exposed: it returns E_ACCESSDENIED for any window not owned by the
    // calling process, which is every window we restore. Use the
    // module-level `move_window_to_desktop_internal` instead.
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

// ---------------------------------------------------------------------------
// Undocumented internal API: IVirtualDesktopManagerInternal
//
// The public IVirtualDesktopManager::MoveWindowToDesktop only works for
// windows owned by the calling process — for everything else it returns
// E_ACCESSDENIED (0x80070005). To move *arbitrary* HWNDs we have to go
// through the undocumented IVirtualDesktopManagerInternal, reached via
// IServiceProvider on the ImmersiveShell COM object.
//
// IIDs differ per Windows build. The block below targets Windows 10
// (any build < 20348, i.e. up to and including 22H2 / build 19045).
// On Windows 11 or future Win10 servicing updates the IIDs may change
// and these calls will fail — the caller should fall back to the public
// API in that case.
//
// Reference: https://github.com/MScholtes/PSVirtualDesktop (the
// "Windows 10" branch of the conditional InterfaceType blocks).
// ---------------------------------------------------------------------------

const CLSID_IMMERSIVE_SHELL: GUID = GUID::from_values(
    0xC2F03A33,
    0x21F5,
    0x47FA,
    [0xB4, 0xBB, 0x15, 0x63, 0x62, 0xA2, 0xF2, 0x39],
);

const CLSID_VIRTUAL_DESKTOP_MANAGER_INTERNAL: GUID = GUID::from_values(
    0xC5E0CDCA,
    0x7B6E,
    0x41B2,
    [0x9F, 0xC4, 0xD9, 0x39, 0x75, 0xCC, 0x46, 0x7B],
);

const IID_ISERVICE_PROVIDER: GUID = GUID::from_values(
    0x6D5140C1,
    0x7436,
    0x11CE,
    [0x80, 0x34, 0x00, 0xAA, 0x00, 0x60, 0x09, 0xFA],
);

// Win10 1809+ (includes 22H2 / build 19045)
const IID_IAPPLICATION_VIEW_COLLECTION_WIN10_1809: GUID = GUID::from_values(
    0x1841C6D7,
    0x4F9D,
    0x42C0,
    [0xAF, 0x41, 0x87, 0x47, 0x53, 0x8F, 0x10, 0xE5],
);

// Win10 build < 20348
const IID_IVIRTUAL_DESKTOP_MANAGER_INTERNAL_WIN10: GUID = GUID::from_values(
    0xF31574D6,
    0xB682,
    0x4CDC,
    [0xBD, 0x56, 0x18, 0x27, 0x86, 0x0A, 0xBE, 0xC6],
);

#[repr(C)]
struct IServiceProviderVtbl {
    query_interface: *const c_void,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    query_service: unsafe extern "system" fn(
        this: *mut c_void,
        guid_service: *const GUID,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> i32,
}

// IApplicationViewCollection on Win10 1809+. We only call get_view_for_hwnd
// but the full vtable layout (incl. the 1803+ Unknown1 slot) must match what
// COM hands us, otherwise call offsets are wrong.
#[repr(C)]
struct IApplicationViewCollectionVtbl {
    query_interface: *const c_void,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_views: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    get_views_by_z_order: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    get_views_by_app_user_model_id:
        unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
    get_view_for_hwnd:
        unsafe extern "system" fn(*mut c_void, HWND, *mut *mut c_void) -> i32,
    get_view_for_application:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    get_view_for_app_user_model_id:
        unsafe extern "system" fn(*mut c_void, *const u16, *mut *mut c_void) -> i32,
    get_view_in_focus: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    // 1803+ extra slot (kept on 1809+ too — 1809 only removed the *position*
    // changes registration, not this one).
    unknown1: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    refresh_collection: unsafe extern "system" fn(*mut c_void) -> i32,
    register_for_application_view_changes:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32) -> i32,
    unregister_for_application_view_changes: unsafe extern "system" fn(*mut c_void, i32) -> i32,
}

// IVirtualDesktopManagerInternal — Windows 10 vtable.
#[repr(C)]
struct IVirtualDesktopManagerInternalWin10Vtbl {
    query_interface: *const c_void,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_count: unsafe extern "system" fn(*mut c_void, *mut i32) -> i32,
    move_view_to_desktop:
        unsafe extern "system" fn(*mut c_void, view: *mut c_void, desktop: *mut c_void) -> i32,
    can_view_move_desktops:
        unsafe extern "system" fn(*mut c_void, view: *mut c_void, *mut i32) -> i32,
    get_current_desktop: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    get_desktops: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    get_adjacent_desktop: unsafe extern "system" fn(
        *mut c_void,
        from: *mut c_void,
        direction: i32,
        out_desktop: *mut *mut c_void,
    ) -> i32,
    switch_desktop: unsafe extern "system" fn(*mut c_void, desktop: *mut c_void) -> i32,
    create_desktop: unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> i32,
    remove_desktop: unsafe extern "system" fn(
        *mut c_void,
        desktop: *mut c_void,
        fallback: *mut c_void,
    ) -> i32,
    find_desktop: unsafe extern "system" fn(
        *mut c_void,
        desktop_id: *const GUID,
        out_desktop: *mut *mut c_void,
    ) -> i32,
}

/// RAII wrapper for a raw COM pointer. Calls Release() on drop.
/// Layout: every COM object's first field is a `*const Vtbl`, and every Vtbl's
/// first three fields are IUnknown — so calling release at vtbl offset 2 is
/// safe regardless of the actual interface.
struct ComPtr {
    ptr: *mut c_void,
}

impl ComPtr {
    fn as_ptr(&self) -> *mut c_void {
        self.ptr
    }
}

impl Drop for ComPtr {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                // First slot in the vtable is QueryInterface, then AddRef, then Release.
                #[repr(C)]
                struct IUnknownVtbl {
                    query_interface: *const c_void,
                    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
                    release: unsafe extern "system" fn(*mut c_void) -> u32,
                }
                let vtbl = *(self.ptr as *const *const IUnknownVtbl);
                ((*vtbl).release)(self.ptr);
            }
        }
    }
}

/// Move an arbitrary top-level window to a specific virtual desktop by GUID,
/// using the undocumented `IVirtualDesktopManagerInternal` interface so it
/// works for windows owned by *other* processes.
///
/// Currently targets Windows 10 IIDs (build < 20348). On Windows 11 these
/// IIDs are different and the call will fail with E_NOINTERFACE.
pub fn move_window_to_desktop_internal(hwnd: HWND, desktop_guid: &GUID) -> Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        // Step 1: CoCreateInstance the ImmersiveShell, then QueryInterface for
        // IServiceProvider. We bypass the `windows` crate's typed wrappers
        // because the shell object exposes many interfaces and we want a raw
        // pointer to the one we'll keep calling QueryService on.
        let shell: windows::core::IUnknown =
            CoCreateInstance(&CLSID_IMMERSIVE_SHELL, None, CLSCTX_LOCAL_SERVER | CLSCTX_ALL)
                .context("CoCreateInstance(ImmersiveShell) failed")?;

        let mut sp_ptr: *mut c_void = std::ptr::null_mut();
        shell
            .query(&IID_ISERVICE_PROVIDER, &mut sp_ptr)
            .ok()
            .context("QueryInterface for IServiceProvider failed")?;
        if sp_ptr.is_null() {
            anyhow::bail!("IServiceProvider pointer was null");
        }
        let service_provider = ComPtr { ptr: sp_ptr };
        let sp_vtbl = *(sp_ptr as *const *const IServiceProviderVtbl);

        // Step 2: QueryService for IVirtualDesktopManagerInternal (Win10 IID).
        let mut vdm_ptr: *mut c_void = std::ptr::null_mut();
        let hr = ((*sp_vtbl).query_service)(
            sp_ptr,
            &CLSID_VIRTUAL_DESKTOP_MANAGER_INTERNAL,
            &IID_IVIRTUAL_DESKTOP_MANAGER_INTERNAL_WIN10,
            &mut vdm_ptr,
        );
        if hr < 0 {
            anyhow::bail!(
                "QueryService(IVirtualDesktopManagerInternal_Win10) failed with HRESULT 0x{:08X}",
                hr as u32
            );
        }
        let vdm = ComPtr { ptr: vdm_ptr };
        let vdm_vtbl = *(vdm_ptr as *const *const IVirtualDesktopManagerInternalWin10Vtbl);

        // Step 3: QueryService for IApplicationViewCollection (service id == iid).
        let mut avc_ptr: *mut c_void = std::ptr::null_mut();
        let hr = ((*sp_vtbl).query_service)(
            sp_ptr,
            &IID_IAPPLICATION_VIEW_COLLECTION_WIN10_1809,
            &IID_IAPPLICATION_VIEW_COLLECTION_WIN10_1809,
            &mut avc_ptr,
        );
        if hr < 0 {
            anyhow::bail!(
                "QueryService(IApplicationViewCollection) failed with HRESULT 0x{:08X}",
                hr as u32
            );
        }
        let avc = ComPtr { ptr: avc_ptr };
        let avc_vtbl = *(avc_ptr as *const *const IApplicationViewCollectionVtbl);

        // Step 4: Get the IApplicationView for our target HWND.
        let mut view_ptr: *mut c_void = std::ptr::null_mut();
        let hr = ((*avc_vtbl).get_view_for_hwnd)(avc_ptr, hwnd, &mut view_ptr);
        if hr < 0 {
            anyhow::bail!(
                "GetViewForHwnd failed with HRESULT 0x{:08X} (window may not have an IApplicationView)",
                hr as u32
            );
        }
        if view_ptr.is_null() {
            anyhow::bail!("GetViewForHwnd returned null view pointer");
        }
        let view = ComPtr { ptr: view_ptr };

        // Step 5: Find the IVirtualDesktop for our target GUID.
        let mut desktop_ptr: *mut c_void = std::ptr::null_mut();
        let hr = ((*vdm_vtbl).find_desktop)(vdm_ptr, desktop_guid, &mut desktop_ptr);
        if hr < 0 {
            anyhow::bail!("FindDesktop failed with HRESULT 0x{:08X}", hr as u32);
        }
        if desktop_ptr.is_null() {
            anyhow::bail!("FindDesktop returned null desktop pointer");
        }
        let desktop = ComPtr { ptr: desktop_ptr };

        // Step 6: Move the view to the desktop.
        let hr = ((*vdm_vtbl).move_view_to_desktop)(vdm_ptr, view.as_ptr(), desktop.as_ptr());
        if hr < 0 {
            anyhow::bail!(
                "MoveViewToDesktop failed with HRESULT 0x{:08X}",
                hr as u32
            );
        }

        // ComPtr Drop runs in reverse declaration order: desktop, view, avc, vdm, service_provider.
        drop(desktop);
        drop(view);
        drop(avc);
        drop(vdm);
        drop(service_provider);
        Ok(())
    }
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
