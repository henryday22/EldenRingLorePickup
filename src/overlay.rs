use std::collections::VecDeque;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW,
    Ellipse, EndPaint, FillRect, GetDC, GetStockObject, InvalidateRect, LineTo, MoveToEx,
    Rectangle, ReleaseDC, SelectObject, SetBkMode, SetTextColor, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, DIB_RGB_COLORS,
    DT_CALCRECT, DT_LEFT, DT_NOPREFIX, DT_WORDBREAK, FF_ROMAN, FW_NORMAL, FW_SEMIBOLD, HDC,
    OUT_TT_PRECIS, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipCreateFromHDC, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectI,
    GdipLoadImageFromFile, GdipSetInterpolationMode, GdiplusStartup, GdiplusStartupInput,
    InterpolationModeHighQualityBicubic,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
    GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, PeekMessageW, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, LWA_COLORKEY, MSG, PM_REMOVE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const CANDIDATE_LIFETIME: Duration = Duration::from_secs(6);
const DIALOG_CLOSE_GRACE: Duration = Duration::from_millis(220);
const PROBE_INTERVAL: Duration = Duration::from_millis(50);
const FADE_IN: Duration = Duration::from_millis(180);
const FADE_OUT: Duration = Duration::from_millis(360);
const PANEL_ALPHA: u8 = 206;
const CONTENT_ALPHA: u8 = 255;
// Black is transparent outside the card. The icon has its own dark (not black) tile, which
// preserves true black artwork while giving antialiased text a clean dark fringe.
const FOREGROUND_KEY: u32 = 0;
const CLASS_NAME: &[u16] = &[
    b'E' as u16,
    b'R' as u16,
    b'L' as u16,
    b'o' as u16,
    b'r' as u16,
    b'e' as u16,
    b'P' as u16,
    b'i' as u16,
    b'c' as u16,
    b'k' as u16,
    b'u' as u16,
    b'p' as u16,
    b'O' as u16,
    b'v' as u16,
    b'e' as u16,
    b'r' as u16,
    b'l' as u16,
    b'a' as u16,
    b'y' as u16,
    0,
];
const FONT_FACE: &[u16] = &[
    b'G' as u16,
    b'a' as u16,
    b'r' as u16,
    b'a' as u16,
    b'm' as u16,
    b'o' as u16,
    b'n' as u16,
    b'd' as u16,
    0,
];

#[derive(Clone, Debug)]
pub struct LoreEntry {
    pub raw_id: u32,
    pub param_id: u32,
    pub quantity: i32,
    pub name: String,
    pub description: String,
    pub icon_id: Option<u32>,
    pub details: Vec<String>,
}

struct DisplayState {
    current: Option<CurrentEntry>,
    pending: VecDeque<(LoreEntry, Instant)>,
    last_dialog_seen: Option<Instant>,
}

struct CurrentEntry {
    entry: LoreEntry,
    shown_at: Instant,
    closing_at: Option<Instant>,
}

impl DisplayState {
    fn new() -> Self {
        Self {
            current: None,
            pending: VecDeque::new(),
            last_dialog_seen: None,
        }
    }

    fn has_work(&self) -> bool {
        self.current.is_some() || !self.pending.is_empty()
    }

    fn sync_dialog(&mut self, visible: bool, now: Instant) {
        while self
            .pending
            .front()
            .map(|(_, queued_at)| now.saturating_duration_since(*queued_at) > CANDIDATE_LIFETIME)
            .unwrap_or(false)
        {
            self.pending.pop_front();
        }

        if visible {
            self.last_dialog_seen = Some(now);
            if let Some(current) = self.current.as_mut() {
                current.closing_at = None;
            } else if let Some((entry, _)) = self.pending.pop_back() {
                // The most recent AddItem call is the one whose vanilla dialog has just appeared.
                // Older candidates are commonly currencies or repeat pickups from the same lot.
                self.pending.clear();
                self.current = Some(CurrentEntry {
                    entry,
                    shown_at: now,
                    closing_at: None,
                });
            }
            return;
        }

        if let Some(current) = self.current.as_mut() {
            let grace_elapsed = self
                .last_dialog_seen
                .map(|seen| now.saturating_duration_since(seen) >= DIALOG_CLOSE_GRACE)
                .unwrap_or(true);
            if grace_elapsed && current.closing_at.is_none() {
                current.closing_at = Some(now);
            }
            if current
                .closing_at
                .map(|closing| now.saturating_duration_since(closing) >= FADE_OUT)
                .unwrap_or(false)
            {
                self.current = None;
                self.last_dialog_seen = None;
            }
        }
    }

    fn snapshot(&self, now: Instant) -> Option<DisplaySnapshot> {
        self.current.as_ref().map(|current| DisplaySnapshot {
            entry: current.entry.clone(),
            shown_at: current.shown_at,
            age: now.saturating_duration_since(current.shown_at),
            closing_age: current
                .closing_at
                .map(|closing| now.saturating_duration_since(closing)),
        })
    }
}

#[derive(Clone)]
struct DisplaySnapshot {
    entry: LoreEntry,
    shown_at: Instant,
    age: Duration,
    closing_age: Option<Duration>,
}

static DISPLAY: OnceLock<Mutex<DisplayState>> = OnceLock::new();
static BACKGROUND_HWND: AtomicIsize = AtomicIsize::new(0);
static CONTENT_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn enqueue(entry: LoreEntry) {
    let display = DISPLAY.get_or_init(|| Mutex::new(DisplayState::new()));
    if let Ok(mut state) = display.lock() {
        crate::runtime::log_line(&format!(
            "LorePickup: queued {} ({:#x}, param {}, qty {}).",
            entry.name, entry.raw_id, entry.param_id, entry.quantity
        ));
        state.pending.push_back((entry, Instant::now()));
    }

    let background = BACKGROUND_HWND.load(Ordering::Relaxed) as HWND;
    let content = CONTENT_HWND.load(Ordering::Relaxed) as HWND;
    if !background.is_null() {
        unsafe {
            InvalidateRect(background, null(), 0);
        }
    }
    if !content.is_null() {
        unsafe { InvalidateRect(content, null(), 0) };
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

        let background = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
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

        let content = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
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

        if background.is_null() || content.is_null() {
            return Err("CreateWindowExW failed for a lore-card layer".to_string());
        }

        BACKGROUND_HWND.store(background as isize, Ordering::Relaxed);
        CONTENT_HWND.store(content as isize, Ordering::Relaxed);
        SetLayeredWindowAttributes(background, rgb(0, 0, 0), 0, LWA_COLORKEY | LWA_ALPHA);
        SetLayeredWindowAttributes(content, FOREGROUND_KEY, 0, LWA_COLORKEY | LWA_ALPHA);
        ShowWindow(background, SW_HIDE);
        ShowWindow(content, SW_HIDE);

        // GDI+ is used only for the supplied PNG inventory thumbnails.
        let mut gdiplus_token = 0usize;
        let startup = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..std::mem::zeroed()
        };
        if GdiplusStartup(&mut gdiplus_token, &startup, null_mut()) != 0 {
            crate::runtime::log_line("LorePickup: GDI+ startup failed; using icon fallbacks.");
        }

        let mut msg: MSG = std::mem::zeroed();
        let mut placement: Option<OverlayPlacement> = None;
        let mut shown = false;
        let mut painted_entry: Option<Instant> = None;
        let mut applied_alpha = 0u8;
        let mut last_probe = Instant::now()
            .checked_sub(PROBE_INTERVAL)
            .unwrap_or_else(Instant::now);
        let mut dialog_hits = 0u8;

        loop {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let now = Instant::now();
            let has_work = DISPLAY
                .get_or_init(|| Mutex::new(DisplayState::new()))
                .lock()
                .map(|state| state.has_work())
                .unwrap_or(false);

            if has_work && now.saturating_duration_since(last_probe) >= PROBE_INTERVAL {
                last_probe = now;
                let detected = find_game_window(background, content)
                    .filter(|game| GetForegroundWindow() == *game)
                    .map(|game| detect_vanilla_pickup_dialog(game))
                    .unwrap_or(false);
                dialog_hits = if detected {
                    dialog_hits.saturating_add(1).min(3)
                } else {
                    0
                };
            } else if !has_work {
                dialog_hits = 0;
            }

            let snapshot = if let Ok(mut state) = DISPLAY
                .get_or_init(|| Mutex::new(DisplayState::new()))
                .lock()
            {
                state.sync_dialog(dialog_hits >= 2, now);
                state.snapshot(now)
            } else {
                None
            };

            if let Some(snapshot) = snapshot {
                let alignment_changed =
                    update_alignment(background, content, &mut placement, &mut shown)
                        .unwrap_or(false);
                let fade = overlay_fade(&snapshot);
                let foreground_alpha = (CONTENT_ALPHA as f32 * fade).round() as u8;
                if foreground_alpha != applied_alpha {
                    let panel_alpha = (PANEL_ALPHA as f32 * fade).round() as u8;
                    SetLayeredWindowAttributes(
                        background,
                        rgb(0, 0, 0),
                        panel_alpha,
                        LWA_COLORKEY | LWA_ALPHA,
                    );
                    SetLayeredWindowAttributes(
                        content,
                        FOREGROUND_KEY,
                        foreground_alpha,
                        LWA_COLORKEY | LWA_ALPHA,
                    );
                    applied_alpha = foreground_alpha;
                }

                // The pixels do not change during the hold. Repainting the full-screen layered
                // window every 16 ms caused GDI to expose its transparent clear between drawing
                // passes, which looked like the game was flickering through the card.
                let closing = snapshot.closing_age.is_some();
                if painted_entry != Some(snapshot.shown_at) || alignment_changed || closing {
                    InvalidateRect(background, null(), 0);
                    InvalidateRect(content, null(), 0);
                    painted_entry = Some(snapshot.shown_at);
                }
            } else {
                // The overlay is a real absence when idle, not an invisible full-screen window.
                hide_overlay(background, content, &mut shown);
                painted_entry = None;
                applied_alpha = 0;
            }

            thread::sleep(Duration::from_millis(16));
        }
    }
}

fn overlay_fade(snapshot: &DisplaySnapshot) -> f32 {
    let opening = if snapshot.age < FADE_IN {
        snapshot.age.as_secs_f32() / FADE_IN.as_secs_f32()
    } else {
        1.0
    };
    let closing = if let Some(age) = snapshot.closing_age {
        1.0 - age.as_secs_f32() / FADE_OUT.as_secs_f32()
    } else {
        1.0
    };
    opening.min(closing).clamp(0.0, 1.0)
}

fn close_progress(snapshot: &DisplaySnapshot) -> f32 {
    snapshot
        .closing_age
        .map(|age| age.as_secs_f32() / FADE_OUT.as_secs_f32())
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OverlayPlacement {
    game: isize,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

unsafe fn hide_overlay(background: HWND, content: HWND, shown: &mut bool) {
    if *shown {
        ShowWindow(background, SW_HIDE);
        ShowWindow(content, SW_HIDE);
        *shown = false;
    }
}

unsafe fn update_alignment(
    background: HWND,
    content: HWND,
    previous: &mut Option<OverlayPlacement>,
    shown: &mut bool,
) -> Option<bool> {
    let Some(game) = find_game_window(background, content) else {
        hide_overlay(background, content, shown);
        return None;
    };

    if GetForegroundWindow() != game {
        hide_overlay(background, content, shown);
        return None;
    }

    let mut rect: RECT = std::mem::zeroed();
    if GetClientRect(game, &mut rect) == 0 {
        hide_overlay(background, content, shown);
        return None;
    }

    let mut origin = POINT { x: 0, y: 0 };
    if ClientToScreen(game, &mut origin) == 0 {
        hide_overlay(background, content, shown);
        return None;
    }

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        hide_overlay(background, content, shown);
        return None;
    }

    let current = OverlayPlacement {
        game: game as isize,
        x: origin.x,
        y: origin.y,
        width,
        height,
    };
    let changed = previous.as_ref() != Some(&current);

    if changed {
        SetWindowPos(
            background,
            HWND_TOPMOST,
            current.x,
            current.y,
            current.width,
            current.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        SetWindowPos(
            content,
            HWND_TOPMOST,
            current.x,
            current.y,
            current.width,
            current.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        *previous = Some(current);
        *shown = true;
    } else if !*shown {
        ShowWindow(background, SW_SHOWNA);
        ShowWindow(content, SW_SHOWNA);
        *shown = true;
    }

    Some(changed)
}

struct FindContext {
    pid: u32,
    background: HWND,
    content: HWND,
    best: HWND,
    best_area: i64,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    let ctx = &mut *(lparam as *mut FindContext);

    if hwnd == ctx.background || hwnd == ctx.content || IsWindowVisible(hwnd) == 0 {
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

unsafe fn find_game_window(background: HWND, content: HWND) -> Option<HWND> {
    let mut ctx = FindContext {
        pid: GetCurrentProcessId(),
        background,
        content,
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

/// Elden Ring owns the pickup lifecycle. We only activate a staged lore card while the
/// distinctive bottom-centre `NEW item / OK` panel is actually present on the composited game
/// image. That means the game's persistent per-character acquisition flags—not a mod-side list—
/// decide whether an item is new, and the same controller/keyboard confirmation closes both.
unsafe fn detect_vanilla_pickup_dialog(game: HWND) -> bool {
    let mut client: RECT = std::mem::zeroed();
    if GetClientRect(game, &mut client) == 0 {
        return false;
    }
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    if width < 640 || height < 360 {
        return false;
    }

    let mut origin = POINT { x: 0, y: 0 };
    if ClientToScreen(game, &mut origin) == 0 {
        return false;
    }

    // The confirmation strip is centred at the foot of the vanilla acquisition dialog.
    // Relative coordinates follow Elden Ring's safe-area layout and survive aspect-ratio changes.
    let capture_left = (width as f32 * 0.39).round() as i32;
    let capture_top = (height as f32 * 0.79).round() as i32;
    let capture_width = (width as f32 * 0.22).round() as i32;
    let capture_height = (height as f32 * 0.11).round() as i32;

    let screen_dc = GetDC(null_mut());
    if screen_dc.is_null() {
        return false;
    }
    let memory_dc = CreateCompatibleDC(screen_dc);
    if memory_dc.is_null() {
        ReleaseDC(null_mut(), screen_dc);
        return false;
    }

    let mut bits: *mut std::ffi::c_void = null_mut();
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: capture_width,
            // Negative height creates a top-down BGRA buffer.
            biHeight: -capture_height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        },
        ..std::mem::zeroed()
    };
    let bitmap = CreateDIBSection(screen_dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() || bits.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        DeleteDC(memory_dc);
        ReleaseDC(null_mut(), screen_dc);
        return false;
    }

    let previous = SelectObject(memory_dc, bitmap);
    let copied = BitBlt(
        memory_dc,
        0,
        0,
        capture_width,
        capture_height,
        screen_dc,
        origin.x + capture_left,
        origin.y + capture_top,
        SRCCOPY,
    ) != 0;

    let mut samples = 0usize;
    let mut dark = 0usize;
    let mut pale = 0usize;
    let mut warm = 0usize;
    if copied {
        let pixels = std::slice::from_raw_parts(
            bits as *const u32,
            (capture_width * capture_height) as usize,
        );
        for y in (0..capture_height as usize).step_by(2) {
            for x in (0..capture_width as usize).step_by(2) {
                let pixel = pixels[y * capture_width as usize + x];
                let b = (pixel & 0xFF) as i32;
                let g = ((pixel >> 8) & 0xFF) as i32;
                let r = ((pixel >> 16) & 0xFF) as i32;
                let highest = r.max(g).max(b);
                let lowest = r.min(g).min(b);
                samples += 1;
                dark += usize::from(highest < 58);
                pale += usize::from(lowest > 112 && highest - lowest < 56);
                warm += usize::from(r > 105 && g > 68 && b < 118 && r * 10 > b * 12);
            }
        }
    }

    SelectObject(memory_dc, previous);
    DeleteObject(bitmap);
    DeleteDC(memory_dc);
    ReleaseDC(null_mut(), screen_dc);

    copied
        && samples > 0
        && dark * 100 > samples * 18
        && pale * 1000 > samples * 4
        && warm * 1000 > samples
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum PaintLayer {
    Background,
    Content,
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.is_null() {
        return;
    }

    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let layer = if hwnd as isize == BACKGROUND_HWND.load(Ordering::Relaxed) {
        PaintLayer::Background
    } else {
        PaintLayer::Content
    };

    // Compose the entire frame away from the visible overlay, then publish it in one BitBlt.
    // This prevents the transparent clear and the card from ever appearing as separate frames.
    let buffer_dc = CreateCompatibleDC(hdc);
    let buffer_bitmap = if !buffer_dc.is_null() && width > 0 && height > 0 {
        CreateCompatibleBitmap(hdc, width, height)
    } else {
        null_mut()
    };

    if buffer_dc.is_null() || buffer_bitmap.is_null() {
        if !buffer_dc.is_null() {
            DeleteDC(buffer_dc);
        }
        draw_frame(hdc, &client, layer);
        EndPaint(hwnd, &ps);
        return;
    }

    let old_bitmap = SelectObject(buffer_dc, buffer_bitmap);
    draw_frame(buffer_dc, &client, layer);
    BitBlt(hdc, 0, 0, width, height, buffer_dc, 0, 0, SRCCOPY);
    SelectObject(buffer_dc, old_bitmap);
    DeleteObject(buffer_bitmap);
    DeleteDC(buffer_dc);

    EndPaint(hwnd, &ps);
}

unsafe fn draw_frame(hdc: HDC, client: &RECT, layer: PaintLayer) {
    let key = if layer == PaintLayer::Background {
        rgb(0, 0, 0)
    } else {
        FOREGROUND_KEY
    };
    let transparent_brush = CreateSolidBrush(key);
    FillRect(hdc, client, transparent_brush);
    DeleteObject(transparent_brush);

    let snapshot = DISPLAY
        .get_or_init(|| Mutex::new(DisplayState::new()))
        .lock()
        .ok()
        .and_then(|state| state.snapshot(Instant::now()));

    if let Some(snapshot) = snapshot {
        draw_card(hdc, client, &snapshot, layer);
    }
}

unsafe fn draw_card(hdc: HDC, client: &RECT, snapshot: &DisplaySnapshot, layer: PaintLayer) {
    let entry = &snapshot.entry;
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let scale = (height as f32 / 2160.0).clamp(0.58, 1.35);
    let px = |at_4k: f32| (at_4k * scale).round() as i32;

    let panel_width = ((width as f32 * 0.215).round() as i32)
        .clamp(px(650.0), px(920.0))
        .min((height as f32 * 0.46).round() as i32)
        .min(width - px(100.0));
    let margin_right = px(66.0);
    let pad_x = px(62.0);
    let pad_top = px(44.0);
    let pad_bottom = px(62.0);
    let icon_size = px(142.0).max(70);
    let icon_gap = px(30.0);
    let title_size = px(56.0).max(27);
    let body_size = px(43.0).max(21);
    let detail_size = px(33.0).max(16);
    let title_gap = px(25.0);
    let rule_gap = px(25.0);
    let text_width = panel_width - pad_x * 2;
    let title_width = (text_width - icon_size - icon_gap).max(px(280.0));

    let title_font = CreateFontW(
        -title_size,
        0,
        0,
        0,
        FW_SEMIBOLD as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        OUT_TT_PRECIS.into(),
        CLIP_DEFAULT_PRECIS.into(),
        CLEARTYPE_QUALITY.into(),
        (DEFAULT_PITCH | FF_ROMAN).into(),
        FONT_FACE.as_ptr(),
    );
    let body_font = CreateFontW(
        -body_size,
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        OUT_TT_PRECIS.into(),
        CLIP_DEFAULT_PRECIS.into(),
        CLEARTYPE_QUALITY.into(),
        (DEFAULT_PITCH | FF_ROMAN).into(),
        FONT_FACE.as_ptr(),
    );
    let detail_font = CreateFontW(
        -detail_size,
        0,
        0,
        0,
        FW_SEMIBOLD as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        OUT_TT_PRECIS.into(),
        CLIP_DEFAULT_PRECIS.into(),
        CLEARTYPE_QUALITY.into(),
        (DEFAULT_PITCH | FF_ROMAN).into(),
        FONT_FACE.as_ptr(),
    );

    SetBkMode(hdc, TRANSPARENT as i32);

    let stock_font = GetStockObject(17); // DEFAULT_GUI_FONT; used only if CreateFontW fails.
    let selected_title = if title_font.is_null() {
        stock_font
    } else {
        title_font
    };
    let old_font = SelectObject(hdc, selected_title);
    let title_height = measure_text(hdc, &entry.name, title_width)
        .max(title_size + px(8.0))
        .max(icon_size);

    let selected_body = if body_font.is_null() {
        stock_font
    } else {
        body_font
    };
    SelectObject(hdc, selected_body);
    let paragraphs = lore_paragraphs(&entry.description);
    let paragraph_gap = px(24.0).max(10);
    let body_height =
        measure_paragraphs(hdc, &paragraphs, text_width, paragraph_gap).max(body_size + px(8.0));

    let selected_detail = if detail_font.is_null() {
        stock_font
    } else {
        detail_font
    };
    SelectObject(hdc, selected_detail);
    let detail_gap = px(12.0).max(5);
    let details_height = if entry.details.is_empty() {
        0
    } else {
        px(36.0)
            + entry
                .details
                .iter()
                .map(|line| measure_text(hdc, line, text_width) + detail_gap)
                .sum::<i32>()
    };
    let natural_height = pad_top
        + title_height
        + title_gap
        + px(1.0)
        + rule_gap
        + body_height
        + details_height
        + pad_bottom;
    let panel_height = natural_height
        .max((panel_width as f32 * 1.50) as i32)
        .min(height - px(90.0));
    let right = width - margin_right;
    // Elden Ring's pickup strip sits at the lower-right. End the lore card just above it.
    let bottom_anchor = (height as f32 * 0.755).round() as i32;
    let bottom = bottom_anchor.max(panel_height + px(40.0));
    let top = bottom - panel_height;
    let left = right - panel_width;
    let panel = RECT {
        left,
        top,
        right,
        bottom,
    };

    if layer == PaintLayer::Background {
        // The painted vellum lives on the translucent layer; icons and lettering remain on the
        // fully opaque content layer. The supplied art has genuinely ragged transparent edges.
        let drawn = crate::icons::card_path()
            .map(|path| draw_png_exact(hdc, &panel, &path))
            .unwrap_or(false);
        if !drawn {
            let panel_brush = CreateSolidBrush(rgb(28, 24, 18));
            FillRect(hdc, &panel, panel_brush);
            DeleteObject(panel_brush);
        }

        SelectObject(hdc, old_font);
        if !title_font.is_null() {
            DeleteObject(title_font);
        }
        if !body_font.is_null() {
            DeleteObject(body_font);
        }
        if !detail_font.is_null() {
            DeleteObject(detail_font);
        }
        return;
    }

    let ink = 1.0;

    let text_left = left + pad_x;
    let text_right = right - pad_x;
    let mut y = top + pad_top;

    let icon_rect = RECT {
        left: text_left,
        top: y,
        right: text_left + icon_size,
        bottom: y + icon_size,
    };
    if ink > 0.12 {
        draw_icon_panel(hdc, &icon_rect, entry, px);
    }

    SelectObject(hdc, selected_title);
    draw_shadowed_text(
        hdc,
        &entry.name,
        icon_rect.right + icon_gap,
        y + ((title_height - measure_text(hdc, &entry.name, title_width)) / 2).max(0),
        text_right,
        y + title_height,
        fade_colour(230, 220, 194, ink),
        px(2.0).max(1),
    );
    y += title_height + title_gap;

    let old_pen = SelectObject(
        hdc,
        CreatePen(PS_SOLID, px(2.0).max(1), fade_colour(133, 105, 59, ink)),
    );
    MoveToEx(hdc, text_left, y, null_mut());
    LineTo(hdc, text_right, y);
    let ornament = px(9.0).max(4);
    let ornament_brush = CreateSolidBrush(fade_colour(133, 105, 59, ink));
    let old_brush = SelectObject(hdc, ornament_brush);
    Ellipse(
        hdc,
        text_left - ornament / 2,
        y - ornament / 2,
        text_left + ornament / 2,
        y + ornament / 2,
    );
    Ellipse(
        hdc,
        text_right - ornament / 2,
        y - ornament / 2,
        text_right + ornament / 2,
        y + ornament / 2,
    );
    SelectObject(hdc, old_brush);
    DeleteObject(ornament_brush);
    let rule_pen = SelectObject(hdc, old_pen);
    DeleteObject(rule_pen);
    y += rule_gap;

    SelectObject(hdc, selected_body);
    y = draw_paragraphs(
        hdc,
        &paragraphs,
        text_left,
        y,
        text_right,
        paragraph_gap,
        fade_colour(224, 221, 211, ink),
        px(2.0).max(1),
    );

    if !entry.details.is_empty() {
        y += px(23.0);
        let separator = CreatePen(PS_SOLID, px(1.0).max(1), fade_colour(92, 75, 47, ink));
        let previous = SelectObject(hdc, separator);
        MoveToEx(hdc, text_left, y, null_mut());
        LineTo(hdc, text_right, y);
        SelectObject(hdc, previous);
        DeleteObject(separator);
        y += px(20.0);
        SelectObject(hdc, selected_detail);
        for line in &entry.details {
            let line_height = measure_text(hdc, line, text_width).max(detail_size + px(3.0));
            draw_shadowed_text(
                hdc,
                line,
                text_left,
                y,
                text_right,
                y + line_height,
                fade_colour(198, 178, 129, ink),
                1,
            );
            y += line_height + detail_gap;
        }
    }

    let closing = close_progress(snapshot);
    if closing > 0.0 {
        draw_rune_dust(hdc, &panel, closing, entry.raw_id, px);
    }

    SelectObject(hdc, old_font);
    if !title_font.is_null() {
        DeleteObject(title_font);
    }
    if !body_font.is_null() {
        DeleteObject(body_font);
    }
    if !detail_font.is_null() {
        DeleteObject(detail_font);
    }
}

unsafe fn draw_rune_dust(
    hdc: HDC,
    panel: &RECT,
    progress: f32,
    seed: u32,
    px: impl Fn(f32) -> i32 + Copy,
) {
    let colour = fade_colour(231, 190, 100, (1.0 - progress * 0.55).max(0.25));
    let pen = CreatePen(PS_SOLID, px(2.0).max(1), colour);
    let brush = CreateSolidBrush(colour);
    let old_pen = SelectObject(hdc, pen);
    let old_brush = SelectObject(hdc, brush);
    let width = (panel.right - panel.left).max(1) as f32;
    let height = (panel.bottom - panel.top).max(1) as f32;

    for index in 0..64u32 {
        let born = hash_unit(seed ^ index.wrapping_mul(0xA511_E9B3));
        if born > progress || progress - born > 0.42 {
            continue;
        }
        let life = ((progress - born) / 0.42).clamp(0.0, 1.0);
        let x0 = panel.left as f32 + hash_unit(seed ^ index.wrapping_mul(0x63D8_3595)) * width;
        let y0 = panel.top as f32 + hash_unit(seed ^ index.wrapping_mul(0xC2B2_AE35)) * height;
        let x = (x0 + life * px(92.0) as f32).round() as i32;
        let y = (y0 - life * px(54.0) as f32).round() as i32;
        let size = px(4.0 + hash_unit(index ^ seed) * 8.0).max(2);
        Ellipse(hdc, x - size, y - size, x + size, y + size);
        if index % 5 == 0 {
            MoveToEx(hdc, x - size * 2, y, null_mut());
            LineTo(hdc, x + size * 2, y);
            MoveToEx(hdc, x, y - size * 2, null_mut());
            LineTo(hdc, x, y + size * 2);
        }
    }

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(brush);
    DeleteObject(pen);
}

fn hash_unit(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    (value as f32) / (u32::MAX as f32)
}

fn fade_colour(r: u8, g: u8, b: u8, opacity: f32) -> u32 {
    let opacity = opacity.clamp(0.0, 1.0);
    rgb(
        (r as f32 * opacity).round() as u8,
        (g as f32 * opacity).round() as u8,
        (b as f32 * opacity).round() as u8,
    )
}

unsafe fn draw_icon_panel(
    hdc: HDC,
    rect: &RECT,
    entry: &LoreEntry,
    px: impl Fn(f32) -> i32 + Copy,
) {
    // The icon tile is intentionally opaque so PNG transparency and true black survive the
    // colour-keyed content layer cleanly.
    let backing = CreateSolidBrush(rgb(8, 8, 7));
    FillRect(hdc, rect, backing);
    DeleteObject(backing);

    let old_brush = SelectObject(hdc, GetStockObject(5)); // NULL_BRUSH
    let pen = CreatePen(PS_SOLID, px(2.0).max(1), rgb(152, 121, 68));
    let old_pen = SelectObject(hdc, pen);
    Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);

    let inset = px(8.0).max(3);
    let inner = CreatePen(PS_SOLID, px(1.0).max(1), rgb(73, 61, 39));
    SelectObject(hdc, inner);
    Rectangle(
        hdc,
        rect.left + inset,
        rect.top + inset,
        rect.right - inset,
        rect.bottom - inset,
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen);
    DeleteObject(inner);

    let drawn = entry
        .icon_id
        .and_then(crate::icons::icon_path)
        .map(|path| draw_png(hdc, rect, &path))
        .unwrap_or(false);

    if !drawn {
        draw_icon_fallback(hdc, rect, entry.raw_id & 0xF000_0000, px);
    }
}

unsafe fn draw_png(hdc: HDC, rect: &RECT, path: &std::path::Path) -> bool {
    let inset = ((rect.right - rect.left) / 18).max(3);
    let target = RECT {
        left: rect.left + inset,
        top: rect.top + inset,
        right: rect.right - inset,
        bottom: rect.bottom - inset,
    };
    draw_png_exact(hdc, &target, path)
}

unsafe fn draw_png_exact(hdc: HDC, rect: &RECT, path: &std::path::Path) -> bool {
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut image = null_mut();
    if GdipLoadImageFromFile(wide.as_ptr(), &mut image) != 0 || image.is_null() {
        return false;
    }

    let mut graphics = null_mut();
    let ok = if GdipCreateFromHDC(hdc, &mut graphics) == 0 && !graphics.is_null() {
        GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        let status = GdipDrawImageRectI(
            graphics,
            image,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
        );
        GdipDeleteGraphics(graphics);
        status == 0
    } else {
        false
    };
    GdipDisposeImage(image);
    ok
}

unsafe fn draw_icon_fallback(hdc: HDC, rect: &RECT, category: u32, px: impl Fn(f32) -> i32 + Copy) {
    let pen = CreatePen(PS_SOLID, px(4.0).max(2), rgb(193, 163, 96));
    let old_pen = SelectObject(hdc, pen);
    let old_brush = SelectObject(hdc, GetStockObject(5));
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    let radius = (rect.right - rect.left) * 3 / 10;
    Ellipse(hdc, cx - radius, cy - radius, cx + radius, cy + radius);

    match category {
        0x0000_0000 => {
            MoveToEx(hdc, cx - radius / 2, cy + radius / 2, null_mut());
            LineTo(hdc, cx + radius / 2, cy - radius / 2);
            MoveToEx(hdc, cx - radius / 3, cy + radius / 3, null_mut());
            LineTo(hdc, cx + radius / 3, cy + radius);
        }
        0x1000_0000 => {
            Rectangle(
                hdc,
                cx - radius / 2,
                cy - radius / 2,
                cx + radius / 2,
                cy + radius / 2,
            );
        }
        _ => {
            MoveToEx(hdc, cx, cy - radius, null_mut());
            LineTo(hdc, cx + radius, cy);
            LineTo(hdc, cx, cy + radius);
            LineTo(hdc, cx - radius, cy);
            LineTo(hdc, cx, cy - radius);
        }
    }

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(pen);
}

unsafe fn measure_text(hdc: HDC, text: &str, width: i32) -> i32 {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return 0;
    }

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: 0,
    };
    DrawTextW(
        hdc,
        wide.as_ptr(),
        wide.len() as i32,
        &mut rect,
        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
    );
    rect.bottom - rect.top
}

fn lore_paragraphs(text: &str) -> Vec<String> {
    let clean = text.replace('\r', "");
    let paragraphs = clean
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if paragraphs.is_empty() {
        vec![text.trim().to_string()]
    } else {
        paragraphs
    }
}

unsafe fn measure_paragraphs(hdc: HDC, paragraphs: &[String], width: i32, gap: i32) -> i32 {
    paragraphs
        .iter()
        .enumerate()
        .map(|(index, paragraph)| {
            measure_text(hdc, paragraph, width) + if index + 1 < paragraphs.len() { gap } else { 0 }
        })
        .sum()
}

unsafe fn draw_paragraphs(
    hdc: HDC,
    paragraphs: &[String],
    left: i32,
    mut top: i32,
    right: i32,
    gap: i32,
    colour: u32,
    shadow_offset: i32,
) -> i32 {
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let height = measure_text(hdc, paragraph, right - left);
        draw_shadowed_text(
            hdc,
            paragraph,
            left,
            top,
            right,
            top + height,
            colour,
            shadow_offset,
        );
        top += height;
        if index + 1 < paragraphs.len() {
            top += gap;
        }
    }
    top
}

unsafe fn draw_shadowed_text(
    hdc: HDC,
    text: &str,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    colour: u32,
    shadow_offset: i32,
) {
    SetTextColor(hdc, rgb(4, 4, 4));
    draw_text(
        hdc,
        text,
        left + shadow_offset,
        top + shadow_offset,
        right + shadow_offset,
        bottom + shadow_offset,
    );
    SetTextColor(hdc, colour);
    draw_text(hdc, text, left, top, right, bottom);
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
