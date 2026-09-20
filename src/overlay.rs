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
    GetStockObject, InvalidateRect, LineTo, MoveToEx, Rectangle, RoundRect, SelectObject,
    SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DEFAULT_PITCH, DT_CALCRECT, DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, DT_WORDBREAK, FF_SWISS, FW_NORMAL, FW_SEMIBOLD, HDC, OUT_TT_PRECIS,
    PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::Graphics::GdiPlus::{
    GdipCreateFromHDC, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectI,
    GdipLoadImageFromFile, GdipSetInterpolationMode, GdiplusStartup, GdiplusStartupInput,
    InterpolationModeHighQualityBicubic,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::Input::XboxController::{
    XInputGetState, XINPUT_GAMEPAD_Y, XINPUT_STATE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetClientRect,
    GetForegroundWindow, GetWindowThreadProcessId, IsWindowVisible, PeekMessageW, RegisterClassW,
    SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
    HTTRANSPARENT, HWND_TOPMOST, LWA_ALPHA, LWA_COLORKEY, MSG, PM_REMOVE, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_HIDE, SW_SHOWNA, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT, WNDCLASSW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const FADE_IN: Duration = Duration::from_millis(120);
const FADE_OUT: Duration = Duration::from_millis(160);
const DISMISS_ARM_DELAY: Duration = Duration::from_millis(100);
const PANEL_ALPHA: u8 = 232;
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
    b'S' as u16,
    b'e' as u16,
    b'g' as u16,
    b'o' as u16,
    b'e' as u16,
    b' ' as u16,
    b'U' as u16,
    b'I' as u16,
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
    pending: VecDeque<LoreEntry>,
}

struct CurrentEntry {
    entry: LoreEntry,
    shown_at: Instant,
    closing_at: Option<Instant>,
    dismiss_ready: bool,
}

impl DisplayState {
    fn new() -> Self {
        Self {
            current: None,
            pending: VecDeque::new(),
        }
    }

    fn enqueue(&mut self, entry: LoreEntry) {
        self.pending.push_back(entry);
    }

    fn tick(&mut self, y_down: bool, now: Instant) {
        if self.current.is_none() {
            self.advance(now);
        }

        if let Some(current) = self.current.as_mut() {
            let age = now.saturating_duration_since(current.shown_at);
            if !current.dismiss_ready && !y_down && age >= DISMISS_ARM_DELAY {
                current.dismiss_ready = true;
            }

            if current.closing_at.is_none() && current.dismiss_ready && y_down
            {
                current.closing_at = Some(now);
            }

            if current
                .closing_at
                .map(|closing| now.saturating_duration_since(closing) >= FADE_OUT)
                .unwrap_or(false)
            {
                self.current = None;
                self.advance(now);
            }
        }
    }

    fn advance(&mut self, now: Instant) {
        if let Some(entry) = self.pending.pop_front() {
            self.current = Some(CurrentEntry {
                entry,
                shown_at: now,
                closing_at: None,
                dismiss_ready: false,
            });
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
            queued: self.pending.len(),
        })
    }
}

#[derive(Clone)]
struct DisplaySnapshot {
    entry: LoreEntry,
    shown_at: Instant,
    age: Duration,
    closing_age: Option<Duration>,
    queued: usize,
}

static DISPLAY: OnceLock<Mutex<DisplayState>> = OnceLock::new();
static BACKGROUND_HWND: AtomicIsize = AtomicIsize::new(0);
static CONTENT_HWND: AtomicIsize = AtomicIsize::new(0);

pub fn enqueue(entry: LoreEntry) {
    let display = DISPLAY.get_or_init(|| Mutex::new(DisplayState::new()));
    if let Ok(mut state) = display.lock() {
        crate::runtime::log_line(&format!(
            "LorePickup: queued flagged item panel {} ({:#x}, param {}, qty {}).",
            entry.name, entry.raw_id, entry.param_id, entry.quantity
        ));
        state.enqueue(entry);
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
        let mut painted_queue: Option<usize> = None;
        let mut applied_alpha = 0u8;
        loop {
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            let now = Instant::now();
            let y_down = controller_y_down();

            let snapshot = if let Ok(mut state) = DISPLAY
                .get_or_init(|| Mutex::new(DisplayState::new()))
                .lock()
            {
                state.tick(y_down, now);
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
                let content_changed = painted_entry != Some(snapshot.shown_at)
                    || painted_queue != Some(snapshot.queued);
                if content_changed || alignment_changed {
                    InvalidateRect(background, null(), 0);
                    InvalidateRect(content, null(), 0);
                    painted_entry = Some(snapshot.shown_at);
                    painted_queue = Some(snapshot.queued);
                }
            } else {
                // The overlay is a real absence when idle, not an invisible full-screen window.
                hide_overlay(background, content, &mut shown);
                painted_entry = None;
                painted_queue = None;
                applied_alpha = 0;
            }

            thread::sleep(Duration::from_millis(16));
        }
    }
}

fn controller_y_down() -> bool {
    // Poll all four XInput slots. The overlay is click-through and never consumes the press; this
    // simply observes the same Y/OK press the game receives. A card is armed only after Y has been
    // released, so a held press cannot dismiss two cards in a queued stack.
    (0..4).any(|index| unsafe {
        let mut state: XINPUT_STATE = std::mem::zeroed();
        XInputGetState(index, &mut state) == 0 && state.Gamepad.wButtons & XINPUT_GAMEPAD_Y != 0
    })
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
        draw_card(hdc, client, &snapshot, layer);
    }
}

unsafe fn draw_card(hdc: HDC, client: &RECT, snapshot: &DisplaySnapshot, layer: PaintLayer) {
    let entry = &snapshot.entry;
    let width = client.right - client.left;
    let height = client.bottom - client.top;
    let scale = (height as f32 / 1080.0).clamp(0.78, 1.55);
    let px = |at_1080: f32| (at_1080 * scale).round() as i32;

    let panel_width = ((width as f32 * 0.27).round() as i32)
        .clamp(px(550.0), px(650.0))
        .min(width - px(88.0));
    let margin_right = px(34.0);
    let pad_x = px(28.0);
    let pad_top = px(26.0);
    let pad_bottom = px(18.0);
    let icon_size = px(92.0).max(68);
    let icon_gap = px(20.0);
    let title_size = px(31.0).max(24);
    let body_size = px(20.0).max(16);
    let detail_size = px(16.0).max(13);
    let label_size = px(12.0).max(10);
    let footer_size = px(14.0).max(11);
    let title_gap = px(20.0);
    let text_width = panel_width - pad_x * 2;
    let title_width = (text_width - icon_size - icon_gap).max(px(280.0));

    let title_font = create_ui_font(title_size, FW_SEMIBOLD as i32);
    let body_font = create_ui_font(body_size, FW_NORMAL as i32);
    let detail_font = create_ui_font(detail_size, FW_NORMAL as i32);
    let label_font = create_ui_font(label_size, FW_SEMIBOLD as i32);
    let footer_font = create_ui_font(footer_size, FW_SEMIBOLD as i32);

    SetBkMode(hdc, TRANSPARENT as i32);

    let stock_font = GetStockObject(17); // DEFAULT_GUI_FONT; used only if CreateFontW fails.
    let selected_title = if title_font.is_null() {
        stock_font
    } else {
        title_font
    };
    let old_font = SelectObject(hdc, selected_title);
    let title_text_height = measure_text(hdc, &entry.name, title_width).max(title_size + px(4.0));
    let header_height = icon_size.max(label_size + px(8.0) + title_text_height);

    let selected_body = if body_font.is_null() {
        stock_font
    } else {
        body_font
    };
    SelectObject(hdc, selected_body);
    let paragraphs = lore_paragraphs(&entry.description);
    let paragraph_gap = px(12.0).max(8);
    let body_height =
        measure_paragraphs(hdc, &paragraphs, text_width, paragraph_gap).max(body_size + px(4.0));

    let selected_detail = if detail_font.is_null() {
        stock_font
    } else {
        detail_font
    };
    SelectObject(hdc, selected_detail);
    let detail_label_width = px(122.0).max(92);
    let detail_value_width = (text_width - detail_label_width - px(12.0)).max(px(230.0));
    let detail_gap = px(9.0).max(6);
    let details_height = if entry.details.is_empty() {
        0
    } else {
        label_size
            + px(16.0)
            + entry
                .details
                .iter()
                .map(|line| {
                    let (_, value) = split_detail(line);
                    measure_text(hdc, value, detail_value_width)
                        .max(detail_size + px(3.0))
                        + detail_gap
                })
                .sum::<i32>()
    };
    let footer_height = px(30.0).max(24);
    let natural_height = pad_top
        + header_height
        + title_gap
        + label_size
        + px(25.0)
        + body_height
        + if entry.details.is_empty() { 0 } else { px(31.0) }
        + details_height
        + px(22.0)
        + footer_height
        + pad_bottom;
    let panel_height = natural_height
        .max(px(310.0))
        .min(height - px(76.0));
    let right = width - margin_right;
    let top = px(38.0);
    let bottom = top + panel_height;
    let left = right - panel_width;
    let panel = RECT {
        left,
        top,
        right,
        bottom,
    };

    if layer == PaintLayer::Background {
        let radius = px(18.0).max(12);
        let shadow = RECT {
            left: panel.left + px(5.0),
            top: panel.top + px(7.0),
            right: panel.right + px(5.0),
            bottom: panel.bottom + px(7.0),
        };
        draw_rounded_panel(hdc, &shadow, rgb(7, 9, 12), rgb(7, 9, 12), radius, 1);

        // Always render a visible physical stack when another card is queued. Two rear cards are
        // enough to communicate the deck without consuming more of the game view.
        for depth in (1..=snapshot.queued.min(2)).rev() {
            let depth = depth as i32;
            let shift_x = px(11.0) * depth;
            let shift_y = px(10.0) * depth;
            let rear = RECT {
                left: panel.left - shift_x,
                top: panel.top + shift_y,
                right: panel.right - shift_x,
                bottom: panel.bottom + shift_y,
            };
            draw_rounded_panel(
                hdc,
                &rear,
                rgb(31, 36, 44),
                rgb(92, 103, 117),
                radius,
                px(1.0).max(1),
            );
        }

        draw_rounded_panel(
            hdc,
            &panel,
            rgb(20, 24, 30),
            rgb(86, 98, 113),
            radius,
            px(1.0).max(1),
        );
        let accent = RECT {
            left: panel.left,
            top: panel.top + radius,
            right: panel.left + px(4.0).max(3),
            bottom: panel.bottom - radius,
        };
        let accent_brush = CreateSolidBrush(rgb(201, 161, 78));
        FillRect(hdc, &accent, accent_brush);
        DeleteObject(accent_brush);

        SelectObject(hdc, old_font);
        delete_fonts(&[title_font, body_font, detail_font, label_font, footer_font]);
        return;
    }

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

    let header_left = icon_rect.right + icon_gap;
    let item_lore_label = if entry.quantity > 1 {
        format!("ITEM LORE  •  ×{}", entry.quantity)
    } else {
        "ITEM LORE".to_string()
    };
    SelectObject(hdc, if label_font.is_null() { stock_font } else { label_font });
    draw_text_coloured(
        hdc,
        &item_lore_label,
        header_left,
        y,
        text_right,
        y + label_size + px(5.0),
        rgb(198, 160, 81),
        DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, selected_title);
    draw_text_coloured(
        hdc,
        &entry.name,
        header_left,
        y + label_size + px(8.0),
        text_right,
        y + header_height,
        rgb(246, 247, 249),
        DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
    );
    if snapshot.queued > 0 {
        let counter = format!("{} CARDS", snapshot.queued + 1);
        SelectObject(hdc, if footer_font.is_null() { stock_font } else { footer_font });
        draw_text_coloured(
            hdc,
            &counter,
            header_left,
            y,
            text_right,
            y + label_size + px(5.0),
            rgb(158, 169, 182),
            DT_RIGHT | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    y += header_height + title_gap;

    let rule = CreatePen(PS_SOLID, px(1.0).max(1), rgb(66, 76, 88));
    let old_pen = SelectObject(hdc, rule);
    MoveToEx(hdc, text_left, y, null_mut());
    LineTo(hdc, text_right, y);
    SelectObject(hdc, old_pen);
    DeleteObject(rule);
    y += px(15.0);

    SelectObject(hdc, if label_font.is_null() { stock_font } else { label_font });
    draw_text_coloured(
        hdc,
        "LORE",
        text_left,
        y,
        text_right,
        y + label_size + px(4.0),
        rgb(150, 163, 178),
        DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
    );
    y += label_size + px(10.0);

    SelectObject(hdc, selected_body);
    y = draw_paragraphs(
        hdc,
        &paragraphs,
        text_left,
        y,
        text_right,
        paragraph_gap,
        rgb(224, 229, 234),
    );

    if !entry.details.is_empty() {
        y += px(21.0);
        let separator = CreatePen(PS_SOLID, px(1.0).max(1), rgb(58, 68, 80));
        let previous = SelectObject(hdc, separator);
        MoveToEx(hdc, text_left, y, null_mut());
        LineTo(hdc, text_right, y);
        SelectObject(hdc, previous);
        DeleteObject(separator);
        y += px(14.0);
        SelectObject(hdc, if label_font.is_null() { stock_font } else { label_font });
        draw_text_coloured(
            hdc,
            "ITEM DETAILS",
            text_left,
            y,
            text_right,
            y + label_size + px(4.0),
            rgb(150, 163, 178),
            DT_LEFT | DT_SINGLELINE | DT_NOPREFIX,
        );
        y += label_size + px(12.0);

        for line in &entry.details {
            let (label, value) = split_detail(line);
            SelectObject(hdc, selected_detail);
            let line_height = measure_text(hdc, value, detail_value_width)
                .max(detail_size + px(3.0));
            SelectObject(hdc, if label_font.is_null() { stock_font } else { label_font });
            draw_text_coloured(
                hdc,
                label,
                text_left,
                y,
                text_left + detail_label_width,
                y + line_height,
                rgb(201, 161, 78),
                DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
            );
            SelectObject(hdc, selected_detail);
            draw_text_coloured(
                hdc,
                value,
                text_left + detail_label_width + px(12.0),
                y,
                text_right,
                y + line_height,
                rgb(235, 238, 241),
                DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
            );
            y += line_height + detail_gap;
        }
    }

    let footer_top = panel.bottom - pad_bottom - footer_height;
    draw_footer(
        hdc,
        text_left,
        footer_top,
        text_right,
        footer_height,
        snapshot.queued,
        if footer_font.is_null() { stock_font } else { footer_font },
        px,
    );

    SelectObject(hdc, old_font);
    delete_fonts(&[title_font, body_font, detail_font, label_font, footer_font]);
}

unsafe fn create_ui_font(size: i32, weight: i32) -> *mut std::ffi::c_void {
    CreateFontW(
        -size,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET.into(),
        OUT_TT_PRECIS.into(),
        CLIP_DEFAULT_PRECIS.into(),
        CLEARTYPE_QUALITY.into(),
        (DEFAULT_PITCH | FF_SWISS).into(),
        FONT_FACE.as_ptr(),
    )
}

unsafe fn delete_fonts(fonts: &[*mut std::ffi::c_void]) {
    for &font in fonts {
        if !font.is_null() {
            DeleteObject(font);
        }
    }
}

fn split_detail(line: &str) -> (&str, &str) {
    line.split_once("  ")
        .map(|(label, value)| (label.trim(), value.trim()))
        .unwrap_or(("INFO", line.trim()))
}

unsafe fn draw_rounded_panel(
    hdc: HDC,
    rect: &RECT,
    fill: u32,
    border: u32,
    radius: i32,
    border_width: i32,
) {
    let brush = CreateSolidBrush(fill);
    let pen = CreatePen(PS_SOLID, border_width.max(1), border);
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen);
    DeleteObject(brush);
}

unsafe fn draw_text_coloured(
    hdc: HDC,
    text: &str,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    colour: u32,
    flags: u32,
) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return;
    }
    SetTextColor(hdc, colour);
    let mut rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    DrawTextW(hdc, wide.as_ptr(), wide.len() as i32, &mut rect, flags);
}

unsafe fn draw_footer(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    height: i32,
    queued: usize,
    font: *mut std::ffi::c_void,
    px: impl Fn(f32) -> i32 + Copy,
) {
    let separator = CreatePen(PS_SOLID, px(1.0).max(1), rgb(58, 68, 80));
    let old_pen = SelectObject(hdc, separator);
    MoveToEx(hdc, left, top - px(10.0), null_mut());
    LineTo(hdc, right, top - px(10.0));
    SelectObject(hdc, old_pen);
    DeleteObject(separator);

    SelectObject(hdc, font);
    let button = height.min(px(24.0)).max(20);
    let cy = top + height / 2;
    let button_rect = RECT {
        left,
        top: cy - button / 2,
        right: left + button,
        bottom: cy + button / 2,
    };
    let brush = CreateSolidBrush(rgb(201, 161, 78));
    let pen = CreatePen(PS_SOLID, 1, rgb(225, 190, 117));
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, pen);
    Ellipse(
        hdc,
        button_rect.left,
        button_rect.top,
        button_rect.right,
        button_rect.bottom,
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen);
    DeleteObject(brush);
    draw_text_coloured(
        hdc,
        "Y",
        button_rect.left,
        button_rect.top,
        button_rect.right,
        button_rect.bottom,
        rgb(18, 22, 27),
        DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    draw_text_coloured(
        hdc,
        "Dismiss",
        button_rect.right + px(9.0),
        top,
        right,
        top + height,
        rgb(174, 184, 195),
        DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
    );
    if queued > 0 {
        let remaining = format!("{} more", queued);
        draw_text_coloured(
            hdc,
            &remaining,
            left,
            top,
            right,
            top + height,
            rgb(174, 184, 195),
            DT_RIGHT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX,
        );
    }
}

unsafe fn draw_icon_panel(
    hdc: HDC,
    rect: &RECT,
    entry: &LoreEntry,
    px: impl Fn(f32) -> i32 + Copy,
) {
    // The icon tile is intentionally opaque so PNG transparency and true black survive the
    // colour-keyed content layer cleanly.
    let backing = CreateSolidBrush(rgb(27, 32, 39));
    FillRect(hdc, rect, backing);
    DeleteObject(backing);

    let old_brush = SelectObject(hdc, GetStockObject(5)); // NULL_BRUSH
    let pen = CreatePen(PS_SOLID, px(1.0).max(1), rgb(75, 86, 99));
    let old_pen = SelectObject(hdc, pen);
    Rectangle(hdc, rect.left, rect.top, rect.right, rect.bottom);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
    DeleteObject(pen);

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
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    for line in clean.split('\n').map(str::trim) {
        if line.is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
        } else {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
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
) -> i32 {
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let height = measure_text(hdc, paragraph, right - left);
        draw_text_coloured(
            hdc,
            paragraph,
            left,
            top,
            right,
            top + height,
            colour,
            DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
        );
        top += height;
        if index + 1 < paragraphs.len() {
            top += gap;
        }
    }
    top
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}


#[cfg(test)]
mod pickup_tests {
    use super::*;

    fn recorded_pickup(state: &mut DisplayState, id: u32, name: &str, metadata: u32) {
        if crate::presentation::classify(metadata).shows_card() {
            state.enqueue(LoreEntry {
                raw_id: id, param_id: id & 0x0fff_ffff, quantity: 1,
                name: name.into(), description: "Game description".into(),
                icon_id: None, details: Vec::new(),
            });
        }
    }

    #[test]
    fn recorded_tears_form_a_deck_and_routine_pickups_cannot_join_it() {
        let mut state = DisplayState::new();
        let now = Instant::now();
        for (id, name) in [(0x40002b15, "Magic"), (0x40002b16, "Lightning"), (0x40002b17, "Holy")] {
            recorded_pickup(&mut state, id, name, 0x10100);
        }
        recorded_pickup(&mut state, 0x40003aca, "Budding Horn", 0xcc000000);
        state.tick(false, now);
        assert_eq!(state.snapshot(now).unwrap().queued, 2);
        for (index, expected) in ["Magic", "Lightning", "Holy"].into_iter().enumerate() {
            let time = now + Duration::from_secs(index as u64);
            assert_eq!(state.snapshot(time).unwrap().entry.name, expected);
            state.tick(false, time + Duration::from_millis(200));
            state.tick(true, time + Duration::from_millis(250));
            state.tick(true, time + Duration::from_millis(450));
            state.tick(true, time + Duration::from_millis(650));
            if let Some(current) = state.current.as_ref() {
                assert!(current.closing_at.is_none(), "held Y must not skip the next card");
            }
        }
        assert!(state.snapshot(now).is_none());
        assert!(state.pending.is_empty());
    }

    #[test]
    fn existing_kukri_and_new_exile_gauntlets_do_not_need_a_hud_transition() {
        let now = Instant::now();
        for (id, name, flags) in [(0x400006c2, "Kukri", 0x10000), (0x1002e6f8, "Exile Gauntlets", 0xcc000100)] {
            let mut state = DisplayState::new();
            for repeat in 0..2 {
                let time = now + Duration::from_secs(repeat * 3);
                recorded_pickup(&mut state, id, name, flags);
                state.tick(true, time); // Y held from collecting the item
                state.tick(true, time + Duration::from_secs(1));
                assert_eq!(state.snapshot(time).unwrap().entry.raw_id, id);
                assert!(state.current.as_ref().unwrap().closing_at.is_none());
                state.tick(false, time + Duration::from_millis(1200));
                state.tick(true, time + Duration::from_millis(1400));
                state.tick(false, time + Duration::from_millis(1600));
                assert!(state.snapshot(time).is_none());
            }
        }
    }

    #[test]
    fn cards_do_not_expire_on_the_small_log_timer() {
        let mut state = DisplayState::new();
        let now = Instant::now();
        recorded_pickup(&mut state, 0x20000488, "Stalwart Horn Charm", 0x10100);
        state.tick(false, now);
        state.tick(false, now + Duration::from_secs(120));
        assert!(state.current.as_ref().unwrap().closing_at.is_none());
        assert_eq!(state.snapshot(now).unwrap().entry.name, "Stalwart Horn Charm");
    }
}
