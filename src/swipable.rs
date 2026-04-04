//! Code for a set of swipable pages in egui

#[derive(Clone, Copy)]
pub struct SwipablePages {
    current_page: usize,
    page_count: usize,
    drag_offset: f32,
    anim_offset: f32,
    is_dragging: bool,
    pending_delta: i32, // +1, -1, or 0 — page change queued after animation
}

impl SwipablePages {
    pub fn new(page_count: usize) -> Self {
        Self {
            current_page: 0,
            page_count,
            drag_offset: 0.0,
            anim_offset: 0.0,
            is_dragging: false,
            pending_delta: 0,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, add_contents: impl Fn(&mut egui::Ui, usize)) {
        let width = ui.available_width();
        let height = ui.available_height();
        let rect = ui.available_rect_before_wrap();

        // Allocate the full area and get a Response for drag tracking
        let response = ui.allocate_rect(rect, egui::Sense::drag());

        // --- Input ---
        if response.drag_started() {
            self.is_dragging = true;
            self.drag_offset = 0.0;
        }
        if response.dragged() {
            self.drag_offset += response.drag_delta().x;
        }
        if response.drag_stopped() {
            self.is_dragging = false;
            let threshold = width * 0.3;

            if self.drag_offset < -threshold && self.current_page + 1 < self.page_count {
                // Queue page-forward: animate offset to -width, THEN advance page
                self.anim_offset = self.drag_offset; // start from where finger left off
                self.pending_delta = 1;
            } else if self.drag_offset > threshold && self.current_page > 0 {
                self.anim_offset = self.drag_offset;
                self.pending_delta = -1;
            }
            // Either way, drag_offset resets
            self.drag_offset = 0.0;
        }

        // --- Animation: snap anim_offset toward drag_offset (or 0) ---
        let target = if self.is_dragging {
            self.drag_offset
        } else if self.pending_delta > 0 {
            -width // animate slide out to the left
        } else if self.pending_delta < 0 {
            width // animate slide out to the right
        } else {
            0.0 // snap back (swipe cancelled)
        };

        let dt = ui.ctx().input(|i| i.stable_dt);
        self.anim_offset += (target - self.anim_offset) * (1.0 - (-dt * 20.0_f32).exp());

        // Once animation is close enough to target, commit the page change
        if self.pending_delta != 0 && (self.anim_offset - target).abs() < 1.0 {
            self.current_page = (self.current_page as i32 + self.pending_delta) as usize;
            self.pending_delta = 0;
            self.anim_offset = 0.0; // new page is already centered
        }

        if self.anim_offset.abs() > 0.5 || self.pending_delta != 0 {
            ui.ctx().request_repaint();
        }

        // --- Render visible pages ---
        let painter = ui.painter_at(rect);
        // Clip so pages don't draw outside bounds
        let clip_rect = rect;

        for offset in [-1i32, 0, 1] {
            let page_idx = self.current_page as i32 + offset;
            if page_idx < 0 || page_idx >= self.page_count as i32 {
                continue;
            }

            let x_shift = offset as f32 * width + self.anim_offset;
            let page_rect = rect.translate(egui::vec2(x_shift, 0.0));
            // Only render if at least partially visible
            if page_rect.right() < rect.left() || page_rect.left() > rect.right() {
                continue;
            }

            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(page_rect)
                    .layout(egui::Layout::top_down(egui::Align::LEFT)),
            );
            child_ui.set_clip_rect(clip_rect);
            add_contents(&mut child_ui, page_idx as usize);
        }
    }
}
