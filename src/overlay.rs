use std::collections::VecDeque;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW,
    CreatePen, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect,
    GetStockObject, InvalidateRect, LineTo, MoveToEx, Rectangle, SelectObject, SetBkMode,
    SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH,
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

const MIN_DISPLAY_TIME: Duration = Duration::from_secs(20);
const MAX_DISPLAY_TIME: Duration = Duration::from_secs(32);
const FADE_IN: Duration = Duration::from_millis(240);
const FADE_OUT: Duration = Duration::from_millis(900);
const PANEL_ALPHA: u8 = 176;
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
}

struct DisplayState {
    current: Option<(LoreEntry, Instant, Duration)>,
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
            .map(|(_, shown_at, duration)| now.duration_since(*shown_at) >= *duration)
            .unwrap_or(true);

        if expired {
            self.current = self.pending.pop_front().map(|entry| {
                let duration = display_time(&entry.description);
                (entry, now, duration)
            });
        }
    }

    fn snapshot(&self, now: Instant) -> Option<DisplaySnapshot> {
        self.current
            .as_ref()
            .map(|(entry, shown_at, duration)| DisplaySnapshot {
                entry: entry.clone(),
                shown_at: *shown_at,
                age: now.saturating_duration_since(*shown_at),
                duration: *duration,
            })
    }
}

#[derive(Clone)]
struct DisplaySnapshot {
    entry: LoreEntry,
    shown_at: Instant,
    age: Duration,
    duration: Duration,
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
        state.pending.push_back(entry);
        if state.current.is_none() {
            state.advance(Instant::now());
        }
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

        loop {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let now = Instant::now();
            let snapshot = if let Ok(mut state) = DISPLAY
                .get_or_init(|| Mutex::new(DisplayState::new()))
                .lock()
            {
                state.advance(now);
                state.snapshot(now)
            } else {
                None
            };

            if let Some(snapshot) = snapshot {
                let alignment_changed =
                    update_alignment(background, content, &mut placement, &mut shown)
                        .unwrap_or(false);
                let fade = overlay_fade(snapshot.age, snapshot.duration);
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
                if painted_entry != Some(snapshot.shown_at) || alignment_changed {
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

fn display_time(description: &str) -> Duration {
    // About 180 words/minute after allowing for the title and the interruption of play.
    let words = description.split_whitespace().count() as u64;
    let seconds = 20 + words.saturating_sub(45).div_ceil(3);
    Duration::from_secs(seconds.clamp(MIN_DISPLAY_TIME.as_secs(), MAX_DISPLAY_TIME.as_secs()))
}

fn overlay_fade(age: Duration, duration: Duration) -> f32 {
    if age < FADE_IN {
        age.as_secs_f32() / FADE_IN.as_secs_f32()
    } else if duration.saturating_sub(age) < FADE_OUT {
        duration.saturating_sub(age).as_secs_f32() / FADE_OUT.as_secs_f32()
    } else {
        1.0
    }
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
        draw_card(hdc, client, &snapshot.entry, layer);
    }
}

unsafe fn draw_card(hdc: HDC, client: &RECT, entry: &LoreEntry, layer: PaintLayer) {
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let scale = (height as f32 / 2160.0).clamp(0.58, 1.35);
    let px = |at_4k: f32| (at_4k * scale).round() as i32;

    let panel_width = ((width as f32 * 0.285).round() as i32)
        .clamp(px(900.0), px(1460.0))
        .min(width - px(100.0));
    let margin_right = px(66.0);
    let pad_x = px(62.0);
    let pad_top = px(44.0);
    let pad_bottom = px(52.0);
    let icon_size = px(154.0).max(76);
    let icon_gap = px(38.0);
    let title_size = px(64.0).max(30);
    let body_size = px(50.0).max(24);
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
    let body_height = measure_text(hdc, &entry.description, text_width).max(body_size + px(8.0));

    let panel_height =
        pad_top + title_height + title_gap + px(1.0) + rule_gap + body_height + pad_bottom;
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
        // Only this window is alpha-reduced. The separate content window remains fully opaque.
        let panel_brush = CreateSolidBrush(rgb(19, 17, 14));
        FillRect(hdc, &panel, panel_brush);
        DeleteObject(panel_brush);

        SelectObject(hdc, old_font);
        if !title_font.is_null() {
            DeleteObject(title_font);
        }
        if !body_font.is_null() {
            DeleteObject(body_font);
        }
        return;
    }

    draw_gilded_frame(hdc, &panel, px);

    let text_left = left + pad_x;
    let text_right = right - pad_x;
    let mut y = top + pad_top;

    let icon_rect = RECT {
        left: text_left,
        top: y,
        right: text_left + icon_size,
        bottom: y + icon_size,
    };
    draw_icon_panel(hdc, &icon_rect, entry, px);

    SelectObject(hdc, selected_title);
    draw_shadowed_text(
        hdc,
        &entry.name,
        icon_rect.right + icon_gap,
        y + ((title_height - measure_text(hdc, &entry.name, title_width)) / 2).max(0),
        text_right,
        y + title_height,
        rgb(230, 220, 194),
        px(2.0).max(1),
    );
    y += title_height + title_gap;

    let old_pen = SelectObject(hdc, CreatePen(PS_SOLID, px(2.0).max(1), rgb(133, 105, 59)));
    MoveToEx(hdc, text_left, y, null_mut());
    LineTo(hdc, text_right, y);
    let ornament = px(9.0).max(4);
    let ornament_brush = CreateSolidBrush(rgb(133, 105, 59));
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
    draw_shadowed_text(
        hdc,
        &entry.description,
        text_left,
        y,
        text_right,
        y + body_height,
        rgb(224, 221, 211),
        px(2.0).max(1),
    );

    SelectObject(hdc, old_font);
    if !title_font.is_null() {
        DeleteObject(title_font);
    }
    if !body_font.is_null() {
        DeleteObject(body_font);
    }
}

unsafe fn draw_gilded_frame(hdc: HDC, panel: &RECT, px: impl Fn(f32) -> i32 + Copy) {
    let outer = CreatePen(PS_SOLID, px(3.0).max(1), rgb(139, 108, 57));
    let old_pen = SelectObject(hdc, outer);
    let old_brush = SelectObject(hdc, GetStockObject(5)); // NULL_BRUSH
    Rectangle(hdc, panel.left, panel.top, panel.right, panel.bottom);

    let inset = px(9.0).max(4);
    let inner = CreatePen(PS_SOLID, px(1.0).max(1), rgb(201, 170, 102));
    SelectObject(hdc, inner);
    Rectangle(
        hdc,
        panel.left + inset,
        panel.top + inset,
        panel.right - inset,
        panel.bottom - inset,
    );

    // Bright corner brackets, short enough to feel like the game's restrained filigree.
    let corner = px(62.0).max(28);
    let bright = CreatePen(PS_SOLID, px(3.0).max(1), rgb(221, 190, 116));
    SelectObject(hdc, bright);
    for &(x, y, dx, dy) in &[
        (panel.left, panel.top, 1, 1),
        (panel.right, panel.top, -1, 1),
        (panel.left, panel.bottom, 1, -1),
        (panel.right, panel.bottom, -1, -1),
    ] {
        MoveToEx(hdc, x, y + dy * corner, null_mut());
        LineTo(hdc, x, y);
        LineTo(hdc, x + dx * corner, y);
    }

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(outer);
    DeleteObject(inner);
    DeleteObject(bright);
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
        .map(crate::icons::icon_path)
        .filter(|path| path.is_file())
        .map(|path| draw_png(hdc, rect, &path))
        .unwrap_or(false);

    if !drawn {
        draw_icon_fallback(hdc, rect, entry.raw_id & 0xF000_0000, px);
    }
}

unsafe fn draw_png(hdc: HDC, rect: &RECT, path: &std::path::Path) -> bool {
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
        let inset = ((rect.right - rect.left) / 18).max(3);
        let status = GdipDrawImageRectI(
            graphics,
            image,
            rect.left + inset,
            rect.top + inset,
            rect.right - rect.left - inset * 2,
            rect.bottom - rect.top - inset * 2,
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
