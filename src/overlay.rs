use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use hudhook::ImguiRenderLoop;
use imgui::{Condition, StyleVar, WindowFlags};

const DISPLAY_TIME: Duration = Duration::from_secs(12);
const PANEL_WIDTH: f32 = 470.0;
const PANEL_MARGIN: f32 = 28.0;
const PANEL_TOP: f32 = 86.0;

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
}

pub struct LoreOverlay;

impl LoreOverlay {
    pub fn new() -> Self {
        Self
    }
}

impl ImguiRenderLoop for LoreOverlay {
    fn render(&mut self, ui: &mut imgui::Ui) {
        let now = Instant::now();
        let display = DISPLAY.get_or_init(|| Mutex::new(DisplayState::new()));

        let entry = {
            let Ok(mut state) = display.lock() else {
                return;
            };
            state.advance(now);
            state.current.as_ref().map(|(entry, _)| entry.clone())
        };

        let Some(entry) = entry else {
            return;
        };

        let screen = ui.io().display_size;
        if screen[0] <= 0.0 || screen[1] <= 0.0 {
            return;
        }

        let x = (screen[0] - PANEL_WIDTH - PANEL_MARGIN).max(PANEL_MARGIN);
        let _rounding = ui.push_style_var(StyleVar::WindowRounding(7.0));
        let _padding = ui.push_style_var(StyleVar::WindowPadding([20.0, 18.0]));

        ui.window("##EldenRingLorePickup")
            .position([x, PANEL_TOP], Condition::Always)
            .size([PANEL_WIDTH, 0.0], Condition::Always)
            .bg_alpha(0.82)
            .flags(
                WindowFlags::NO_TITLE_BAR
                    | WindowFlags::NO_RESIZE
                    | WindowFlags::NO_MOVE
                    | WindowFlags::NO_SAVED_SETTINGS
                    | WindowFlags::NO_FOCUS_ON_APPEARING
                    | WindowFlags::NO_NAV
                    | WindowFlags::NO_INPUTS
                    | WindowFlags::ALWAYS_AUTO_RESIZE,
            )
            .build(|| {
                ui.text_disabled(entry.kind);
                ui.text_wrapped(&entry.name);

                if entry.quantity > 1 {
                    ui.same_line();
                    ui.text_disabled(format!(" x{}", entry.quantity));
                }

                if let Some(info) = entry.info.as_ref().filter(|s| {
                    let t = s.trim();
                    !t.is_empty() && t != entry.description.trim()
                }) {
                    ui.spacing();
                    ui.text_wrapped(info);
                }

                ui.separator();
                ui.spacing();
                ui.text_wrapped(&entry.description);
            });
    }
}
