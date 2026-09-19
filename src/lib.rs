#![cfg(windows)]

mod overlay;
mod pickup;
mod runtime;

use std::ffi::c_void;
use std::thread;
use std::time::Duration;

use hudhook::{hooks, Hudhook};
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    hmodule: HINSTANCE,
    reason: u32,
    _reserved: *mut c_void,
) -> bool {
    if reason != DLL_PROCESS_ATTACH {
        return true;
    }

    let hmodule_raw = hmodule.0 as usize;

    thread::spawn(move || {
        runtime::init_log();

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

        if let Err(err) = pickup::install() {
            runtime::log_line(&format!("LorePickup: item hook not installed: {err}"));
            return;
        }

        runtime::log_line("LorePickup: item hook installed.");

        let hmodule = HINSTANCE(hmodule_raw as _);
        if let Err(err) = Hudhook::builder()
            .with::<hooks::dx12::ImguiDx12Hooks>(overlay::LoreOverlay::new())
            .with_hmodule(hmodule)
            .build()
            .apply()
        {
            runtime::log_line(&format!(
                "LorePickup: DX12 overlay hook failed ({err:?}). Pickup hook remains active but nothing will be drawn."
            ));
            return;
        }

        runtime::log_line("LorePickup: DX12 overlay installed.");
    });

    true
}
