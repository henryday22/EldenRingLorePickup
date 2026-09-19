#![cfg(windows)]

mod overlay;
mod pickup;
mod runtime;

use std::ffi::c_void;
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    _hmodule: HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> bool {
    if reason != DLL_PROCESS_ATTACH {
        return true;
    }

    thread::spawn(move || {
        runtime::init_log();
        runtime::log_line("LorePickup v0.2 bootstrap: no DirectX overlay hooks.");

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

        if let Err(err) = overlay::start() {
            runtime::log_line(&format!("LorePickup: Win32 overlay failed to start: {err}"));
        } else {
            runtime::log_line("LorePickup: Win32 overlay thread started.");
        }

        // Avoid touching AddItem while Elden Ring, ERSS and the rest of the native stack are
        // still initialising. Missing startup grants is harmless; world pickups happen later.
        runtime::log_line("LorePickup: waiting 8 seconds before installing item hook.");
        thread::sleep(Duration::from_secs(8));

        if let Err(err) = pickup::install() {
            runtime::log_line(&format!("LorePickup: item hook not installed: {err}"));
            return;
        }

        runtime::log_line("LorePickup: item hook installed; ready for pickups.");
    });

    true
}
