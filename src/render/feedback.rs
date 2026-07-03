use crate::render::ring_shape;
use crate::render::Renderer;
use std::time::Instant;

impl Renderer {
    pub(crate) fn draw_verifying_feedback(&self) {
        let center_x = self.width as f64 / 2.0;
        let center_y = self.height as f64 / 2.0;
        let radius = self.config.indicator_radius as f64;
        let thickness = self.config.indicator_thickness as f64;
        let (r, g, b, a) = self.config.verifying_color;

        if a > 0.0 {
            self.context.new_path();
            self.context.set_source_rgba(r, g, b, a * self.fade_alpha);
            self.context.set_line_width(thickness + 2.0);
            self.context.set_line_join(cairo::LineJoin::Round);
            ring_shape::build_ring_path(
                &self.context,
                center_x,
                center_y,
                radius,
                self.config.ring_shape,
            );
            render_try!(self.context.stroke());
        }
    }

    pub(crate) fn draw_wrong_password_feedback(&self) {
        let center_x = self.width as f64 / 2.0;
        let center_y = self.height as f64 / 2.0;
        let radius = self.config.indicator_radius as f64;
        let thickness = self.config.indicator_thickness as f64;
        let intensity = if let Some(start) = self.wrong_password_start {
            let elapsed = start.elapsed();
            let duration = std::time::Duration::from_millis(self.config.wrong_password_duration);
            if elapsed < duration {
                1.0 - (elapsed.as_secs_f64() / duration.as_secs_f64())
            } else {
                0.0
            }
        } else {
            0.0
        };

        if intensity > 0.0 {
            self.context.new_path();
            self.context
                .set_source_rgba(1.0, 0.0, 0.0, intensity * self.fade_alpha);
            self.context.set_line_width(thickness + 2.0);
            self.context.set_line_join(cairo::LineJoin::Round);
            ring_shape::build_ring_path(
                &self.context,
                center_x,
                center_y,
                radius,
                self.config.ring_shape,
            );
            render_try!(self.context.stroke());
        }
    }

    pub(crate) fn draw_key_highlight_feedback(&self) {
        let center_x = self.width as f64 / 2.0;
        let center_y = self.height as f64 / 2.0;
        let radius = self.config.indicator_radius as f64;
        let thickness = self.config.indicator_thickness as f64;
        let intensity = if let Some(start) = self.key_highlight_start {
            let elapsed = start.elapsed();
            let duration = std::time::Duration::from_millis(self.config.key_highlight_duration);
            if elapsed < duration {
                1.0 - (elapsed.as_secs_f64() / duration.as_secs_f64())
            } else {
                0.0
            }
        } else {
            0.0
        };

        if intensity > 0.0 {
            let (r, g, b, a) = if self.caps_lock {
                self.config.caps_lock_key_hl_color
            } else {
                self.config.key_hl_color
            };
            self.context
                .set_source_rgba(r, g, b, a * intensity * self.fade_alpha);
            self.context.set_line_width(thickness + 1.5);

            self.context.new_path();
            self.context.set_line_cap(cairo::LineCap::Round);
            // Convert angle range to normalized perimeter t (for circle: t = angle / 2π)
            let max_dots = self.config.max_dots as f64;
            let t_offset = ring_shape::top_centre_offset(self.config.ring_shape);
            let global_t = ((self.password_display.len() as f64) / max_dots) + t_offset;
            let random_t = self.key_highlight_angle / (2.0 * std::f64::consts::PI);
            let t_start = global_t + random_t;
            let sector_t = 40.0 / 360.0;
            let t_end = t_start + sector_t;
            ring_shape::build_sector_path(
                &self.context,
                center_x,
                center_y,
                radius,
                self.config.ring_shape,
                t_start,
                t_end,
            );
            render_try!(self.context.stroke());
        }
    }

    pub(crate) fn draw_cleared_feedback(&self) {
        let center_x = self.width as f64 / 2.0;
        let center_y = self.height as f64 / 2.0;
        let radius = self.config.indicator_radius as f64;
        let thickness = self.config.indicator_thickness as f64;
        let intensity = if let Some(start) = self.cleared_feedback_start {
            let elapsed = start.elapsed();
            let duration = std::time::Duration::from_millis(self.config.cleared_feedback_duration);
            if elapsed < duration {
                1.0 - (elapsed.as_secs_f64() / duration.as_secs_f64())
            } else {
                0.0
            }
        } else {
            0.0
        };

        if intensity > 0.0 {
            self.context.new_path();
            self.context
                .set_source_rgba(1.0, 0.0, 0.0, intensity * self.fade_alpha * 0.5);
            ring_shape::build_fill_path(
                &self.context,
                center_x,
                center_y,
                radius - thickness / 2.0,
                thickness,
                self.config.ring_shape,
            );
            render_try!(self.context.fill());

            self.context.new_path();
            self.context
                .set_source_rgba(1.0, 0.0, 0.0, intensity * self.fade_alpha);
            self.context.set_line_width(thickness + 4.0);
            self.context.set_line_join(cairo::LineJoin::Round);
            ring_shape::build_ring_path(
                &self.context,
                center_x,
                center_y,
                radius,
                self.config.ring_shape,
            );
            render_try!(self.context.stroke());

            self.context.new_path();
            self.context.set_font_size(24.0);
            self.context
                .set_source_rgba(1.0, 1.0, 1.0, intensity * self.fade_alpha);
            let text = "CLEARED";
            let te = render_try!(self.context.text_extents(text));
            self.context
                .move_to(center_x - te.width() / 2.0, center_y - radius - 20.0);
            render_try!(self.context.show_text(text));
        }
    }

    /// Draw the current PAM message (e.g. the faillock lockout notice) centered
    /// below the indicator. Fades out over the last 500ms of its lifetime.
    pub(crate) fn draw_pam_message(&self) {
        let msg = match &self.pam_message {
            Some(m) if !m.is_empty() => m,
            _ => return,
        };
        let intensity = match self.pam_message_start {
            Some(start) => {
                let elapsed = start.elapsed();
                let duration = std::time::Duration::from_millis(self.config.message_duration);
                if elapsed >= duration {
                    0.0
                } else if duration - elapsed < std::time::Duration::from_millis(500) {
                    (duration - elapsed).as_secs_f64() / 0.5
                } else {
                    1.0
                }
            }
            None => 0.0,
        };
        if intensity <= 0.0 {
            return;
        }

        let center_x = self.width as f64 / 2.0;
        let center_y = self.height as f64 / 2.0;
        let radius = self.config.indicator_radius as f64;
        let (r, g, b, a) = self.config.message_color;

        self.context.new_path();
        self.context.set_font_size(20.0);
        self.context
            .set_source_rgba(r, g, b, a * intensity * self.fade_alpha);
        let te = render_try!(self.context.text_extents(msg));
        self.context
            .move_to(center_x - te.width() / 2.0, center_y + radius + 40.0);
        render_try!(self.context.show_text(msg));
    }

    /// Whether any feedback animation is currently in flight and therefore
    /// requires continued per-frame redraws until it finishes.
    pub(crate) fn is_animating(&self) -> bool {
        self.wrong_password_start.is_some()
            || self.key_highlight_start.is_some()
            || self.cleared_feedback_start.is_some()
            || self.verifying_start.is_some()
            || self.pam_message_start.is_some()
    }

    pub(crate) fn update_feedback_timers(&mut self) {
        self.update_uptime();
        if let Some(start) = self.wrong_password_start {
            if start.elapsed()
                > std::time::Duration::from_millis(self.config.wrong_password_duration)
            {
                self.wrong_password_shown = false;
                self.wrong_password_start = None;
            }
        }
        if let Some(start) = self.key_highlight_start {
            if start.elapsed()
                > std::time::Duration::from_millis(self.config.key_highlight_duration)
            {
                self.key_highlight_shown = false;
                self.key_highlight_start = None;
            }
        }
        if let Some(start) = self.cleared_feedback_start {
            if start.elapsed()
                > std::time::Duration::from_millis(self.config.cleared_feedback_duration)
            {
                self.cleared_feedback_shown = false;
                self.cleared_feedback_start = None;
            }
        }
        if let Some(start) = self.verifying_start {
            if start.elapsed() > std::time::Duration::from_millis(self.config.auth_timeout) {
                self.verifying_shown = false;
                self.verifying_start = None;
            }
        }
        if let Some(start) = self.pam_message_start {
            if start.elapsed() > std::time::Duration::from_millis(self.config.message_duration) {
                self.pam_message = None;
                self.pam_message_start = None;
            }
        }
    }

    pub fn show_wrong_password(&mut self) {
        self.wrong_password_shown = true;
        self.wrong_password_start = Some(Instant::now());
        // Clear verifying state — wrong password replaces it
        self.verifying_shown = false;
        self.verifying_start = None;
    }

    pub fn show_key_highlight(&mut self) {
        self.key_highlight_shown = true;
        self.key_highlight_start = Some(Instant::now());
        use std::time::SystemTime;
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let random_val = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.key_highlight_angle = ((random_val % 360) as f64).to_radians();
    }

    pub fn show_cleared_feedback(&mut self) {
        self.cleared_feedback_shown = true;
        self.cleared_feedback_start = Some(Instant::now());
    }

    pub fn show_verifying(&mut self) {
        self.verifying_shown = true;
        self.verifying_start = Some(Instant::now());
    }

    pub fn clear_verifying(&mut self) {
        self.verifying_shown = false;
        self.verifying_start = None;
    }

    pub fn show_pam_message(&mut self, msg: String) {
        self.pam_message = Some(msg);
        self.pam_message_start = Some(Instant::now());
    }
}
