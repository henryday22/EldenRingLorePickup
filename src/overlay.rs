use std::collections::VecDeque;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetStockObject, InvalidateRect, SelectObject, SetBkMode, SetTextColor, DEFAULT_GUI_FONT,
    DT_LEFT, DT_NOPREFIX, DT_WORDBREAK, HDC, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
    GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, PeekMessageW, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW,
    CS_VREDRAW, HTTRANSPARENT,
    HWND_TOPMOST, LWA_ALPHA, LWA_COLORKEY, MSG, PM_REMOVE, SW_HIDE, SW_SHOWNA, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const DISPLAY_TIME: Duration = Duration::from_secs(12);
const CLASS_NAME: &[u16] = &[
    b'E' as u16, b'R' as u16, b'L' as u16, b'o' as u16, b'r' as u16, b'e' as u16,
    b'P' as u16, b'i' as u16, b'c' as u16, b'k' as u16, b'u' as u16, b'p' as u16,
    b'O' as u16, b'v' as u16, b'e' as u16, b'r' as u16, b'l' as u16, b'a' as u16,
    b'y' as u16, 0,
];

#[derive(Clone, Debug)]
pub struct LoreEntry {
    pub raw_id: u32,
    pub param_id: u32,
    pub quantity: i32,
    pub kind: &'static str,
    pub name: String,
    pub info: Option<String>,
    pub description: String,
}

struct DisplayState {
    current: Option<(LoreEntry, Instant)>,
    pending: VecDeque<LoreEntry>,
}

impl DisplayState {
    fn new() -> Self {
        Self {
            current: None,
            pending: VecDeque::new(),
        }
    }

    fn advance(&mut self, now: Instant) {
        let expired = self
            .current
            .as_ref()
            .map(|(_, shown_at)| now.duration_since(*shown_at) >= DISPLAY_TIME)
            .unwrap_or(true);

        if expired {
            self.current = self.pending.pop_front().map(|entry| (entry, now));
        }
    }
}

static DISPLAY: OnceLock<Mutex<DisplayState>> = OnceLock::new();
static OVERLAY_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn enqueue(entry: LoreEntry) {
    let display = DISPLAY.get_or_init(|| Mutex::new(DisplayState::new()));
    if let Ok(mut state) = display.lock() {
        crate::runtime::log_line(&format!(
            "LorePickup: queued {} ({:#x}, param {}, qty {}).",
            entry.name, entry.raw_id, entry.param_id, entry.quantity
        ));
        state.pending.push_back(entry);
        if state.current.is_none() {
            state.advance(Instant::now());
        }
    }

    let hwnd = OVERLAY_HWND.load(Ordering::Relaxed) as HWND;
    if !hwnd.is_null() {
        unsafe {
            InvalidateRect(hwnd, null(), 0);
        }
    }
}

pub fn start() -> Result<(), String> {
    thread::Builder::new()
        .name("EldenRingLorePickupOverlay".to_string())
        .spawn(|| {
            if let Err(err) = overlay_thread() {
                crate::runtime::log_line(&format!("LorePickup: overlay thread stopped: {err}"));
            }
        })
        .map_err(|e| format!("could not spawn overlay thread: {e}"))?;
    Ok(())
}

fn overlay_thread() -> Result<(), String> {
    unsafe {
        let hinstance = GetModuleHandleW(null());
        if hinstance.is_null() {
            return Err("GetModuleHandleW failed".to_string());
        }

        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: CLASS_NAME.as_ptr(),
            ..std::mem::zeroed()
        };

        if RegisterClassW(&wc) == 0 {
            return Err("RegisterClassW failed".to_string());
        }

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOPMOST
                | WS_EX_NOACTIVATE
                | WS_EX_TOOLWINDOW,
            CLASS_NAME.as_ptr(),
            CLASS_NAME.as_ptr(),
            WS_POPUP,
            0,
            0,
            100,
            100,
            null_mut(),
            null_mut(),
            hinstance,
            null_mut(),
        );

        if hwnd.is_null() {
            return Err("CreateWindowExW failed".to_string());
        }

        OVERLAY_HWND.store(hwnd as isize, Ordering::Relaxed);

        // Black is the transparent colour key; everything else is slightly translucent.
        SetLayeredWindowAttributes(hwnd, 0, 235, LWA_COLORKEY | LWA_ALPHA);
        ShowWindow(hwnd, SW_HIDE);

        let mut msg: MSG = std::mem::zeroed();

        loop {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            update_alignment(hwnd);

            if let Ok(mut state) = DISPLAY.get_or_init(|| Mutex::new(DisplayState::new())).lock() {
                state.advance(Instant::now());
            }

            InvalidateRect(hwnd, null(), 0);
            thread::sleep(Duration::from_millis(50));
        }
    }
}

unsafe fn update_alignment(overlay: HWND) {
    let Some(game) = find_game_window(overlay) else {
        ShowWindow(overlay, SW_HIDE);
        return;
    };

    let foreground = GetForegroundWindow();
    if foreground != game {
        ShowWindow(overlay, SW_HIDE);
        return;
    }

    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(game, &mut rect) == 0 {
        ShowWindow(overlay, SW_HIDE);
        return;
    }

    let mut origin = POINT { x: 0, y: 0 };
    if ClientToScreen(game, &mut origin) == 0 {
        ShowWindow(overlay, SW_HIDE);
        return;
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        ShowWindow(overlay, SW_HIDE);
        return;
    }

    SetWindowPos(
        overlay,
        HWND_TOPMOST,
        origin.x,
        origin.y,
        width,
        height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    ShowWindow(overlay, SW_SHOWNA);
}

struct FindContext {
    pid: u32,
    overlay: HWND,
    best: HWND,
    best_area: i64,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut FindContext);

    if hwnd == ctx.overlay || IsWindowVisible(hwnd) == 0 {
        return 1;
    }

    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if pid != ctx.pid {
        return 1;
    }

    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(hwnd, &mut rect) == 0 {
        return 1;
    }

    let width = (rect.right - rect.left).max(0) as i64;
    let height = (rect.bottom - rect.top).max(0) as i64;
    let area = width * height;

    if area > ctx.best_area {
        ctx.best = hwnd;
        ctx.best_area = area;
    }

    1
}

unsafe fn find_game_window(overlay: HWND) -> Option<HWND> {
    let mut ctx = FindContext {
        pid: GetCurrentProcessId(),
        overlay,
        best: null_mut(),
        best_area: 0,
    };

    EnumWindows(Some(enum_window), &mut ctx as *mut _ as LPARAM);

    if ctx.best.is_null() {
        None
    } else {
        Some(ctx.best)
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }

    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);

    let transparent_brush = CreateSolidBrush(rgb(0, 0, 0));
    FillRect(hdc, &client, transparent_brush);
    DeleteObject(transparent_brush);

    let entry = DISPLAY
        .get_or_init(|| Mutex::new(DisplayState::new()))
        .lock()
        .ok()
        .and_then(|state| state.current.as_ref().map(|(entry, _)| entry.clone()));

    if let Some(entry) = entry {
        let width = client.right - client.left;
        let panel_width = 560.min((width - 60).max(360));
        let left = (width - panel_width - 28).max(20);
        let top = 76;
        let right = left + panel_width;
        let bottom = (top + 620).min(client.bottom - 30);

        let panel = RECT {
            left,
            top,
            right,
            bottom,
        };

        let panel_brush = CreateSolidBrush(rgb(24, 24, 26));
        FillRect(hdc, &panel, panel_brush);
        DeleteObject(panel_brush);

        SetBkMode(hdc, TRANSPARENT as i32);
        let old_font = SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));

        let pad = 22;
        let mut y = top + 18;

        SetTextColor(hdc, rgb(180, 180, 185));
        draw_text(hdc, entry.kind, left + pad, y, right - pad, y + 28);
        y += 32;

        SetTextColor(hdc, rgb(245, 245, 245));
        let title = if entry.quantity > 1 {
            format!("{}  x{}", entry.name, entry.quantity)
        } else {
            entry.name.clone()
        };
        draw_text(hdc, &title, left + pad, y, right - pad, y + 54);
        y += 60;

        if let Some(info) = entry.info.as_ref().filter(|s| {
            let t = s.trim();
            !t.is_empty() && t != entry.description.trim()
        }) {
            SetTextColor(hdc, rgb(205, 205, 210));
            draw_text(hdc, info, left + pad, y, right - pad, y + 92);
            y += 102;
        }

        SetTextColor(hdc, rgb(232, 232, 235));
        draw_text(hdc, &entry.description, left + pad, y, right - pad, bottom - 18);

        SelectObject(hdc, old_font);
    }

    EndPaint(hwnd, &ps);
}

unsafe fn draw_text(hdc: HDC, text: &str, left: i32, top: i32, right: i32, bottom: i32) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return;
    }

    let mut rect = RECT {
        left,
        top,
        right,
        bottom,
    };

    DrawTextW(
        hdc,
        wide.as_ptr(),
        wide.len() as i32,
        &mut rect,
        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
    );
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}
