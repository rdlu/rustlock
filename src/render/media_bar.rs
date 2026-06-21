use crate::render::Renderer;
use cairo::{Format, ImageSurface};

impl Renderer {
    pub(crate) fn draw_media(&mut self) {
        if let Some(ref title) = self.system_status.media_title {
            let center_x = self.width as f64 / 2.0;
            let start_y = self.height as f64 - 120.0;
            let art_size = 56.0;

            if self.config.show_album_art && self.system_status.media_art_url != self.last_art_url {
                self.last_art_url = self.system_status.media_art_url.clone();
                self.media_art_surface = None;
                if let Some(ref data) = self.system_status.media_art_data {
                    if let Ok(img) = image::load_from_memory(data) {
                        let img = img.to_rgba8();
                        let (w, h) = img.dimensions();
                        if let Ok(mut surface) = ImageSurface::create(Format::ARgb32, w as i32, h as i32) {
                            if let Ok(mut surface_data) = surface.data() {
                                for y in 0..h {
                                    for x in 0..w {
                                        let pixel = img.get_pixel(x, y);
                                        let idx = ((y * w + x) * 4) as usize;
                                        surface_data[idx] = pixel[2];
                                        surface_data[idx + 1] = pixel[1];
                                        surface_data[idx + 2] = pixel[0];
                                        surface_data[idx + 3] = pixel[3];
                                    }
                                }
                            } else {
                                log::error!("Failed to access album art surface data");
                            }
                            self.media_art_surface = Some(surface);
                        } else {
                            log::error!("Failed to create album art surface");
                        }
                    }
                }
            }

            let has_art = self.config.show_album_art && self.media_art_surface.is_some();

            self.context.new_path();
            self.context
                .set_source_rgba(1.0, 1.0, 1.0, self.fade_alpha * 0.9);
            self.context.set_font_size(16.0);

            let display_text = if let Some(ref artist) = self.system_status.media_artist {
                format!("{} - {}", artist, title)
            } else {
                title.clone()
            };

            let te = render_try!(self.context.text_extents(&display_text));

            // Center art + text as a group with 16px gap between them
            let art_text_gap = 16.0;
            let group_width = te.width() + art_size + art_text_gap;
            let group_start_x = center_x - group_width / 2.0;
            let art_x = group_start_x;
            let text_center_x = art_x + art_size + art_text_gap + te.width() / 2.0;

            if has_art {
                if let Some(ref art) = self.media_art_surface {
                    render_try!(self.context.save());
                    let scale = art_size / art.width() as f64;
                    self.context.translate(art_x, start_y);
                    self.context.scale(scale, scale);
                    render_try!(self.context.set_source_surface(art, 0.0, 0.0));
                    render_try!(self.context.paint_with_alpha(self.fade_alpha));
                    render_try!(self.context.restore());
                }
            }

            self.context.move_to(text_center_x - te.width() / 2.0, start_y + 20.0);
            render_try!(self.context.show_text(&display_text));

            // All media buttons on one row, evenly spaced.
            // Each gets a 24×24 hit area, matching draw_icon_at's target_size.
            let btn_size = 24.0;
            let btn_gap = 48.0;
            let btn_y = start_y + 50.0;

            // Layout: prev | play_pause | next (centered as a group)
            let total_buttons: f64 =
                (self.media_prev_icon_surface.is_some() as u32
                    + 1
                    + self.media_next_icon_surface.is_some() as u32) as f64;
            let group_width = (total_buttons - 1.0) * btn_gap + btn_size;
            let group_start_x = center_x - group_width / 2.0;
            let mut btn_x = group_start_x;

            if let Some(ref icon) = self.media_prev_icon_surface {
                self.draw_icon_at(btn_x, btn_y - btn_size / 2.0, icon);
                self.media_rects.push(("prev", btn_x, btn_y - btn_size / 2.0, btn_size, btn_size));
                btn_x += btn_gap;
            }

            // Play/pause — always present (at least one of play/pause icon should load)
            if self.system_status.media_playing {
                if let Some(ref icon) = self.media_pause_icon_surface {
                    self.draw_icon_at(btn_x, btn_y - btn_size / 2.0, icon);
                    self.media_rects.push(("play_pause", btn_x, btn_y - btn_size / 2.0, btn_size, btn_size));
                }
            } else if let Some(ref icon) = self.media_play_icon_surface {
                self.draw_icon_at(btn_x, btn_y - btn_size / 2.0, icon);
                self.media_rects.push(("play_pause", btn_x, btn_y - btn_size / 2.0, btn_size, btn_size));
            }
            btn_x += btn_gap;

            if let Some(ref icon) = self.media_next_icon_surface {
                self.draw_icon_at(btn_x, btn_y - btn_size / 2.0, icon);
                self.media_rects.push(("next", btn_x, btn_y - btn_size / 2.0, btn_size, btn_size));
            }
        }
    }
}
