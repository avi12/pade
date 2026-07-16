//! PADE's Windows 11 **modern** context-menu handler — an in-process COM server
//! implementing [`IExplorerCommand`] for the "Open in PADE" verb on folders.
//!
//! Windows 11 shows this menu first (before "Show more options"). Unlike the
//! legacy menu, it will only load a handler that has *package identity*, so this
//! DLL is registered through a sparse MSIX manifest (`../AppxManifest.xml`) rather
//! than the registry. The manifest maps our [`CLSID_OPEN_IN_PADE`] to this DLL and
//! declares the `Directory` / `Directory\Background` verbs that point at it; File
//! Explorer then `CoCreateInstance`s us in-proc via [`DllGetClassObject`].
//!
//! Flow: right-click a folder → Explorer builds us via the class factory →
//! [`IExplorerCommand_Impl::GetTitle`] labels the item → on click,
//! [`IExplorerCommand_Impl::Invoke`] reads the folder path out of the passed
//! `IShellItemArray` and launches the co-located `pade.exe` with it (PADE's
//! `launch_context` opens that path as the project).
//!
//! COM demands `unsafe` (raw vtable pointers, FFI), so — unlike the main crate —
//! `unsafe_code` is not forbidden here; every other lint stays at the strict bar.

// The `#[implement]` macro expands to helper impls that trip a couple of pedantic
// lints (a reference-to-raw-pointer cast, `#[inline(always)]`) which we cannot
// annotate at their source because they are generated. Allow exactly those two;
// all hand-written code stays under the full pedantic bar.
#![allow(clippy::ref_as_ptr, clippy::inline_always)]

use core::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};

use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_NOTIMPL, E_POINTER, HINSTANCE,
    HMODULE, S_FALSE, S_OK,
};
use windows::Win32::System::Com::{CoTaskMemFree, IBindCtx, IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::UI::Shell::{
    IEnumExplorerCommand, IExplorerCommand, IExplorerCommand_Impl, IShellItem, IShellItemArray,
    SHStrDupW, ECS_ENABLED, SIGDN_FILESYSPATH,
};
use windows_core::{w, Error, IUnknown, Interface, Ref, Result, BOOL, GUID, HRESULT, PWSTR};
use windows_implement::implement;

/// PADE's fixed context-menu CLSID. **Authoritative copy** — the exact same GUID
/// appears (with braces) in `../AppxManifest.xml` (`com:Class Id` and every
/// `desktop5:Verb Clsid`) and in `src/contextmenu/modern.rs`. Change all three
/// together. Value: `{C6FD5832-8BA5-4FDE-A5CC-A74C36AD27AC}`.
const CLSID_OPEN_IN_PADE: GUID = GUID::from_u128(0xC6FD_5832_8BA5_4FDE_A5CC_A74C_36AD_27AC);

/// `fdwReason` value Windows passes `DllMain` on load — we capture our own module
/// handle then (named, so the `== 1` test below reads as intent).
const DLL_PROCESS_ATTACH: u32 = 1;

/// Console-suppression flag for the launched `pade.exe`; keeps a stray `conhost`
/// window from flashing out of Explorer's process. Mirrors `pade`'s `util::command`.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// This DLL's own module handle, captured in [`DllMain`], so `Invoke` can find the
/// `pade.exe` sitting next to us. `AtomicPtr` stores the raw handle without an
/// `as` cast.
static DLL_INSTANCE: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

/// Live COM objects + server locks. COM may unload us only when this hits zero.
static DLL_REFERENCES: AtomicIsize = AtomicIsize::new(0);

// ---------------------------------------------------------------------------
// The "Open in PADE" command.
// ---------------------------------------------------------------------------

#[implement(IExplorerCommand)]
struct OpenInPadeCommand;

impl OpenInPadeCommand {
    fn new() -> Self {
        DLL_REFERENCES.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for OpenInPadeCommand {
    fn drop(&mut self) {
        DLL_REFERENCES.fetch_sub(1, Ordering::SeqCst);
    }
}

// windows-rs 0.59+ implements the `_Impl` trait for the macro-generated wrapper
// (`OpenInPadeCommand_Impl`), and hands interface args as `windows::core::Ref`.
impl IExplorerCommand_Impl for OpenInPadeCommand_Impl {
    /// The visible label. Explorer frees the returned buffer, so it must be
    /// `CoTaskMemAlloc`'d — `SHStrDupW` copies the literal into COM task memory.
    fn GetTitle(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        unsafe { SHStrDupW(w!("Open in PADE")) }
    }

    /// Icon as `"path,index"`; we reuse the co-located `pade.exe`'s own icon.
    fn GetIcon(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        let Some(exe) = locate_pade_exe() else {
            return Err(E_NOTIMPL.into());
        };
        let reference = format!("{},0", exe.display());
        unsafe { SHStrDupW(windows_core::PCWSTR(to_wide(&reference).as_ptr())) }
    }

    fn GetToolTip(&self, _items: Ref<'_, IShellItemArray>) -> Result<PWSTR> {
        Err(E_NOTIMPL.into())
    }

    fn GetCanonicalName(&self) -> Result<GUID> {
        Ok(CLSID_OPEN_IN_PADE)
    }

    /// Always enabled: the manifest only offers us on `Directory` targets, so
    /// there is nothing further to gate on. `ECS_ENABLED` is `0`.
    fn GetState(&self, _items: Ref<'_, IShellItemArray>, _ok_to_be_slow: BOOL) -> Result<u32> {
        u32::try_from(ECS_ENABLED.0).map_err(|_| Error::from(E_FAIL))
    }

    /// Read every selected folder's filesystem path and open it in PADE.
    fn Invoke(&self, items: Ref<'_, IShellItemArray>, _context: Ref<'_, IBindCtx>) -> Result<()> {
        let exe = locate_pade_exe().ok_or_else(|| Error::from(E_FAIL))?;
        let items = items.ok()?;
        let count = unsafe { items.GetCount()? };
        for index in 0..count {
            let item: IShellItem = unsafe { items.GetItemAt(index)? };
            let wide: PWSTR = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
            // We own `wide`; convert then free before anything can early-return.
            let path = unsafe { wide.to_string() };
            unsafe { CoTaskMemFree(Some(wide.as_ptr().cast::<c_void>())) };
            let Ok(path) = path else { continue };
            // Best-effort, detached — Explorer must never block on us.
            let _ = Command::new(&exe)
                .arg(&path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
        Ok(())
    }

    /// No flyout / sub-commands — a single flat verb (`ECF_DEFAULT` is `0`).
    fn GetFlags(&self) -> Result<u32> {
        Ok(0)
    }

    fn EnumSubCommands(&self) -> Result<IEnumExplorerCommand> {
        Err(E_NOTIMPL.into())
    }
}

/// Absolute path of `pade.exe`, resolved relative to this DLL (they are
/// co-located at the sparse package's external location). Falls back to the sole
/// `.exe` beside us so a bundle that names it `PADE.exe` still works.
fn locate_pade_exe() -> Option<PathBuf> {
    let directory = current_module_path()?.parent()?.to_path_buf();
    let preferred = directory.join("pade.exe");
    if preferred.is_file() {
        return Some(preferred);
    }
    std::fs::read_dir(&directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        })
}

/// This DLL's full path, from the module handle captured in [`DllMain`].
fn current_module_path() -> Option<PathBuf> {
    let module = HMODULE(DLL_INSTANCE.load(Ordering::SeqCst));
    if module.0.is_null() {
        return None;
    }
    let mut buffer = [0u16; 1024];
    let length = usize::try_from(unsafe { GetModuleFileNameW(Some(module), &mut buffer) }).ok()?;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buffer[..length])))
}

/// UTF-16, NUL-terminated, for passing a Rust string to a `PCWSTR` Win32 arg.
fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// Class factory + DLL exports (the in-proc COM server contract).
// ---------------------------------------------------------------------------

#[implement(IClassFactory)]
struct OpenInPadeFactory;

impl IClassFactory_Impl for OpenInPadeFactory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        interface_id: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let command: IExplorerCommand = OpenInPadeCommand::new().into();
        // `query` is QueryInterface: checks the IID, AddRefs, writes `object`.
        unsafe { command.query(interface_id, object).ok() }
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            DLL_REFERENCES.fetch_add(1, Ordering::SeqCst);
        } else {
            DLL_REFERENCES.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

/// Captures this DLL's module handle so [`current_module_path`] can locate the
/// neighbouring `pade.exe`. Windows calls this on load/unload.
#[no_mangle]
pub extern "system" fn DllMain(instance: HINSTANCE, reason: u32, _reserved: *mut c_void) -> BOOL {
    if reason == DLL_PROCESS_ATTACH {
        DLL_INSTANCE.store(instance.0, Ordering::SeqCst);
    }
    BOOL(1)
}

/// Hands Explorer our [`IClassFactory`] for [`CLSID_OPEN_IN_PADE`].
///
/// # Safety
/// Per the COM `DllGetClassObject` contract `class_id` / `interface_id` must point
/// to valid [`GUID`]s and `object` to a valid `*mut *mut c_void` out-parameter.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    class_id: *const GUID,
    interface_id: *const GUID,
    object: *mut *mut c_void,
) -> HRESULT {
    if object.is_null() {
        return E_POINTER;
    }
    unsafe { *object = null_mut() };
    if class_id.is_null() || interface_id.is_null() {
        return E_POINTER;
    }
    if unsafe { *class_id } != CLSID_OPEN_IN_PADE {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = OpenInPadeFactory.into();
    unsafe { factory.query(interface_id, object) }
}

/// COM may unload this DLL only when no objects and no server locks remain.
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if DLL_REFERENCES.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}
