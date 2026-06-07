use std::time::{Duration, Instant};

use imgui::{MouseButton, StyleVar, Ui, WindowHoveredFlags};

const TOUCH_SCROLL_DRAG_THRESHOLD: f32 = 18.0;
const TOUCH_SCROLL_HOLD_THRESHOLD: Duration = Duration::from_millis(250);
const TOUCH_HOVER_DISABLE_THRESHOLD: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollableListFrame {
    pub touch_started: bool,
    pub suppress_click: bool,
    pub disable_hover: bool,
}

#[derive(Clone, Debug)]
pub struct ScrollableList {
    touch_scroll_active: bool,
    touch_scroll_gesture: bool,
    touch_scroll_ignore_delta: bool,
    touch_scroll_distance: f32,
    touch_scroll_started_at: Option<Instant>,
}

impl ScrollableList {
    pub fn new() -> Self {
        Self {
            touch_scroll_active: false,
            touch_scroll_gesture: false,
            touch_scroll_ignore_delta: false,
            touch_scroll_distance: 0.0,
            touch_scroll_started_at: None,
        }
    }

    pub fn draw<R>(
        &mut self,
        ui: &Ui,
        id: &str,
        size: [f32; 2],
        border: bool,
        content: impl FnOnce(&Ui, ScrollableListFrame) -> R,
    ) -> Option<R> {
        ui.child_window(id).size(size).border(border).build(|| {
            let item_padding = ui.push_style_var(StyleVar::FramePadding([8.0, 10.0]));
            let item_spacing = ui.push_style_var(StyleVar::ItemSpacing([4.0, 6.0]));

            let frame = self.update_touch_scroll(ui);
            let result = content(ui, frame);

            item_spacing.pop();
            item_padding.pop();

            result
        })
    }

    fn update_touch_scroll(&mut self, ui: &Ui) -> ScrollableListFrame {
        let mut touch_started = false;

        if ui.is_mouse_clicked(MouseButton::Left)
            && ui
                .is_window_hovered_with_flags(WindowHoveredFlags::ALLOW_WHEN_BLOCKED_BY_ACTIVE_ITEM)
        {
            touch_started = true;
            self.touch_scroll_active = true;
            self.touch_scroll_gesture = false;
            self.touch_scroll_ignore_delta = true;
            self.touch_scroll_distance = 0.0;
            self.touch_scroll_started_at = Some(Instant::now());
        }

        let mut disable_hover = false;

        if self.touch_scroll_active && ui.io().mouse_down[MouseButton::Left as usize] {
            let delta_y = if self.touch_scroll_ignore_delta {
                self.touch_scroll_ignore_delta = false;
                0.0
            } else {
                ui.io().mouse_delta[1]
            };
            let delta_y_abs = delta_y.abs();
            self.touch_scroll_distance += delta_y_abs;
            disable_hover =
                delta_y_abs >= TOUCH_HOVER_DISABLE_THRESHOLD || self.touch_scroll_gesture;

            if self.touch_scroll_distance >= TOUCH_SCROLL_DRAG_THRESHOLD
                || self
                    .touch_scroll_started_at
                    .is_some_and(|started_at| started_at.elapsed() >= TOUCH_SCROLL_HOLD_THRESHOLD)
            {
                self.touch_scroll_gesture = true;
            }

            if self.touch_scroll_gesture && delta_y_abs > 0.0 {
                let scroll_y = (ui.scroll_y() - delta_y).clamp(0.0, ui.scroll_max_y());
                ui.set_scroll_y(scroll_y);
            }
        }

        let suppress_click = self.touch_scroll_gesture;
        disable_hover |= suppress_click;

        if self.touch_scroll_active && ui.is_mouse_released(MouseButton::Left) {
            self.touch_scroll_active = false;
            self.touch_scroll_gesture = false;
            self.touch_scroll_ignore_delta = false;
            self.touch_scroll_distance = 0.0;
            self.touch_scroll_started_at = None;
        }

        ScrollableListFrame {
            touch_started,
            suppress_click,
            disable_hover,
        }
    }
}

impl Default for ScrollableList {
    fn default() -> Self {
        Self::new()
    }
}
