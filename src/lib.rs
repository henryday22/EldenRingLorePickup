#![cfg(windows)]

mod icons;
mod guidance;
mod insight;
mod collection;
mod journal;
mod player;
mod overlay;
mod pickup;
mod presentation;
mod runtime;

use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

static MODULE_HANDLE: AtomicIsize = AtomicIsize::new(0);

pub fn module_dir() -> PathBuf {
    use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;

    let module = MODULE_HANDLE.load(Ordering::Relaxed) as *mut c_void;
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }
    buffer.truncate(length as usize);
    PathBuf::from(String::from_utf16_lossy(&buffer))
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    hmodule: HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> bool {
    if reason != DLL_PROCESS_ATTACH {
        return true;
    }

    MODULE_HANDLE.store(hmodule.0 as isize, Ordering::Relaxed);

    thread::spawn(move || {
        runtime::init_log();
        runtime::log_line(
            "LorePickup v0.10.0 bootstrap: recipes, character context and collection journal; v0.8.2 trigger preserved.",
        );

        let runtime = (0..120).find_map(|_| match runtime::detect_runtime() {
            Ok(found) => Some(found),
            Err(_) => {
                thread::sleep(Duration::from_millis(250));
                None
            }
        });

        let Some(runtime) = runtime else {
            runtime::log_line(
                "LorePickup: compatible Elden Ring runtime signatures were not found; mod left inert.",
            );
            return;
        };

        if runtime::RUNTIME.set(runtime).is_err() {
            runtime::log_line("LorePickup: runtime was already initialized.");
            return;
        }

        journal::start();
        if let Err(err) = overlay::start() {
            runtime::log_line(&format!("LorePickup: Win32 overlay failed to start: {err}"));
        } else {
            runtime::log_line("LorePickup: Win32 overlay thread started.");
        }

        // Avoid touching the item presentation path while Elden Ring, ERSS and the rest of the
        // native stack are still initialising. Missing startup grants is harmless; world pickups
        // happen later.
        runtime::log_line("LorePickup: waiting 8 seconds before installing item-panel hook.");
        thread::sleep(Duration::from_secs(8));

        if let Err(err) = pickup::install() {
            runtime::log_line(&format!("LorePickup: item hook not installed: {err}"));
            return;
        }

        runtime::log_line(
            "LorePickup: item hook installed; filtering per-item presentation metadata.",
        );
    });

    true
}
