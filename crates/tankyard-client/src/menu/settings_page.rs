//! Display, graphics, audio and accessibility settings.
use super::*;

impl Menu {
    pub(super) fn accessibility_preview(&self, r: Rect, settings: &Settings, s: f32) {
        panel(r);
        label(
            "LIVE PREVIEW",
            r.x + 14. * s,
            r.y + 22. * s,
            12. * s,
            muted_color(),
        );
        let time = crate::accessibility::motion_time(get_time() as f32);
        for team in 1..=4 {
            let x = r.x + r.w * (team as f32 - 0.5) / 4.;
            let color = render::team_color(team, false, settings.colorblind);
            if let Some(class) = self.practice_content.class("vector") {
                render::class_model(
                    class,
                    crate::ui_layout::position(vec2(x, r.y + 65. * s)),
                    15. * s,
                    -0.4 + (time * 0.7).sin() * 0.2,
                    0.,
                    0.,
                    color,
                    time,
                );
            }
            if settings.colorblind {
                render::team_marker(
                    team,
                    false,
                    crate::ui_layout::position(vec2(x, r.y + 70. * s)),
                    3. * s,
                );
            }
            center_label(&format!("Team {team}"), x, r.y + 104. * s, 11. * s, color);
        }
        label(
            "Your tank",
            r.x + 14. * s,
            r.y + 140. * s,
            13. * s,
            render::team_color(0, true, settings.colorblind),
        );
        right_label(
            "Opponent",
            r.x + r.w - 14. * s,
            r.y + 140. * s,
            13. * s,
            render::team_color(0, false, settings.colorblind),
        );
        center_label(
            if settings.reduced_motion {
                "Motion paused"
            } else {
                "Motion preview"
            },
            r.center().x,
            r.y + 167. * s,
            11. * s,
            muted_color(),
        );
    }
    pub(super) fn settings_tabs(&mut self, s: f32) {
        let x = 40. * s;
        let y = 158. * s;
        let tab_w = (screen_width() - 2. * x - 7. * 8. * s) / 8.;
        // Indices are stable for saved state; Graphics was added last but
        // sits beside Display.
        for (slot, (index, name)) in [
            (0, "Display"),
            (7, "Graphics"),
            (1, "Style"),
            (2, "Controls"),
            (3, "Accessibility"),
            (4, "Profile"),
            (5, "Audio"),
            (6, "Servers"),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(x + slot as f32 * (tab_w + 8.0 * s), y, tab_w, 38.0 * s),
                name,
                self.settings_tab == index,
                true,
                s * 0.85,
            ) {
                self.settings_tab = index;
                self.clear_focus();
            }
        }
    }
    /// Presets, render scale, upscaling and post-processing.
    pub(super) fn graphics(
        &mut self,
        settings: &mut Settings,
        x: f32,
        by: f32,
        col: f32,
        s: f32,
    ) -> Option<Action> {
        label("QUALITY PRESET", x, by + 19.0 * s, 13.0 * s, muted_color());
        // (name, render scale, upscaler, sharpness, FXAA, shape quality)
        // Macs with MetalFX upscale with it; it needs less sharpening.
        let metalfx = crate::graphics::metalfx_available();
        let dlss_status = crate::dlss::status();
        let dlss_ready = dlss_status == crate::dlss::Status::Ready;
        let dlss_offered = dlss_ready || dlss_status == crate::dlss::Status::NeedsRuntime;
        let dlss_selected = dlss_offered && settings.upscaler == crate::upscale::DLSS;
        let (upscaler, performance, balanced) = if metalfx {
            (crate::upscale::METALFX, 0.3, 0.15)
        } else {
            (1, 0.45, 0.3)
        };
        let presets = if dlss_ready {
            // GeForce RTX: the arena is light to draw, so DLSS's fixed cost
            // buys image quality rather than speed. The fast presets stay on
            // FSR 1; Quality is DLSS Quality and Ultra is DLAA.
            let dlss = crate::upscale::DLSS;
            [
                ("Performance", 0.65, upscaler, performance, false, 0),
                ("Balanced", 0.85, upscaler, balanced, false, 1),
                ("Quality", 0.67, dlss, 0.0, false, 2),
                ("Ultra", 1.0, dlss, 0.0, false, 2),
            ]
        } else {
            [
                ("Performance", 0.65, upscaler, performance, false, 0),
                ("Balanced", 0.85, upscaler, balanced, false, 1),
                ("Quality", 1.0, upscaler, 0.0, true, 2),
                ("Ultra", 1.5, 0, 0.15, false, 2),
            ]
        };
        let width = col / presets.len() as f32;
        for (i, (name, scale, upscaler, sharpness, fxaa, shape)) in presets.into_iter().enumerate()
        {
            let current = (settings.render_scale - scale).abs() < 0.01
                && settings.upscaler == upscaler
                && (settings.sharpness - sharpness).abs() < 0.01
                && settings.fxaa == fxaa
                && settings.shape_quality == shape;
            if button(
                Rect::new(
                    x + i as f32 * width,
                    by + 30.0 * s,
                    width - 6.0 * s,
                    33.0 * s,
                ),
                name,
                current,
                true,
                s * 0.72,
            ) {
                settings.render_scale = scale;
                settings.upscaler = upscaler;
                settings.sharpness = sharpness;
                settings.fxaa = fxaa;
                settings.shape_quality = shape;
                return Some(Action::Save);
            }
        }
        let (w, h) = (screen_width(), screen_height());
        let physical = miniquad::window::screen_size();
        let ratio = (physical.0 / w.max(1.0)).max(1.0);
        slider_value(
            Rect::new(x, by + 82.0 * s, col, 48.0 * s),
            "Render scale",
            &mut settings.render_scale,
            [0.5, 2.0],
            0.05,
            s,
            |value| {
                format!(
                    "{:.0}%  ·  {} × {}",
                    value * 100.0,
                    (w * ratio * value).round(),
                    (h * ratio * value).round()
                )
            },
        );
        let scale_note = if dlss_selected {
            match crate::dlss::Mode::for_scale(settings.render_scale) {
                crate::dlss::Mode::Dlaa => {
                    "DLAA: full resolution, anti-aliased by DLSS.".to_owned()
                }
                mode => format!(
                    "DLSS {}: fewer pixels, rebuilt to full detail.",
                    mode.name()
                ),
            }
        } else if settings.render_scale > 1.0 {
            "Above 100% supersamples: smoother edges, more GPU work.".to_owned()
        } else if settings.render_scale < 1.0 {
            "Below 100% renders fewer pixels, then upscales. Menus stay sharp.".to_owned()
        } else {
            "The arena renders at full resolution.".to_owned()
        };
        label(&scale_note, x, by + 143.0 * s, 11.0 * s, muted_color());
        label("UPSCALER", x, by + 172.0 * s, 13.0 * s, muted_color());
        let mut names = vec![(0, "Bilinear"), (1, "AMD FSR 1")];
        if metalfx {
            names.push((crate::upscale::METALFX, "MetalFX"));
        }
        if dlss_offered {
            names.push((crate::upscale::DLSS, "NVIDIA DLSS"));
        }
        // An upscaler this system lacks (a copied settings file) runs as
        // FSR 1 here.
        let current = match settings.upscaler {
            crate::upscale::METALFX if !metalfx => 1,
            crate::upscale::DLSS if !dlss_offered => 1,
            upscaler => upscaler,
        };
        let width = col / names.len() as f32;
        for (i, &(index, name)) in names.iter().enumerate() {
            if button(
                Rect::new(
                    x + i as f32 * width,
                    by + 183.0 * s,
                    width - 6.0 * s,
                    33.0 * s,
                ),
                name,
                current == index,
                true,
                s * 0.72,
            ) {
                settings.upscaler = index;
                if index == crate::upscale::DLSS && !dlss_ready {
                    crate::dlss::start_runtime_download();
                }
            }
        }
        slider(
            Rect::new(x, by + 228.0 * s, col, 48.0 * s),
            "Sharpening",
            &mut settings.sharpness,
            0.0,
            1.0,
            s,
        );
        if dlss_selected {
            self.dlss_status(&dlss_status, Rect::new(x, by + 290.0 * s, col, 33.0 * s), s);
        } else {
            toggle(
                Rect::new(x, by + 290.0 * s, col, 33.0 * s),
                "FXAA edge smoothing",
                &mut settings.fxaa,
                s,
            );
        }
        let device = crate::hardware::device();
        if let crate::dlss::Status::Unavailable(reason) = &dlss_status
            && self.post_error.is_none()
            && device.as_ref().is_some_and(crate::hardware::Device::nvidia)
        {
            // Say why an NVIDIA GPU can't offer DLSS (a GTX card, an old
            // driver, a build without it).
            wrap(
                &format!("NVIDIA DLSS: {reason}"),
                x,
                by + 348.0 * s,
                col,
                12.0 * s,
                muted_color(),
            );
        }
        if let Some(error) = &self.post_error {
            wrap(
                &format!(
                    "Post-processing is unavailable on this device, so the arena uses bilinear filtering. {error}"
                ),
                x,
                by + 348.0 * s,
                col,
                14.0 * s,
                RED,
            );
        }
        None
    }
    /// DLSS's state where the FXAA toggle would be (DLSS anti-aliases).
    pub(super) fn dlss_status(&mut self, status: &crate::dlss::Status, rect: Rect, s: f32) {
        use crate::dlss::download::Progress;
        if *status == crate::dlss::Status::Ready {
            label(
                "NVIDIA DLSS is on. It also anti-aliases, so FXAA is off.",
                rect.x,
                rect.y + 21.0 * s,
                12.0 * s,
                muted_color(),
            );
            return;
        }
        match crate::dlss::runtime_download() {
            Some(Progress::Running { done, total }) => {
                let title = if total > 0 {
                    format!(
                        "Downloading DLSS from NVIDIA…  {:.0}%",
                        done as f32 * 100. / total as f32
                    )
                } else {
                    "Downloading DLSS from NVIDIA…".to_owned()
                };
                button(rect, &title, false, false, s * 0.72);
            }
            Some(Progress::Failed(error)) => {
                if button(rect, "Retry the DLSS download", false, true, s * 0.72) {
                    crate::dlss::start_runtime_download();
                }
                label(&error, rect.x, rect.bottom() + 16.0 * s, 11.0 * s, RED);
            }
            _ => {
                if button(
                    rect,
                    "Download DLSS  (60 MB, from NVIDIA)",
                    false,
                    true,
                    s * 0.72,
                ) {
                    crate::dlss::start_runtime_download();
                }
                label(
                    "Until then the arena upscales with FSR 1.",
                    rect.x,
                    rect.bottom() + 16.0 * s,
                    11.0 * s,
                    muted_color(),
                );
            }
        }
    }
    pub(super) fn interface_scale(
        &mut self,
        rect: Rect,
        settings: &mut Settings,
        s: f32,
        anchor: f32,
    ) {
        let mut preview = self.scale_edit.preview(settings.ui_scale);
        if slider_value(
            rect,
            "Interface scale",
            &mut preview,
            [0.8, 1.4],
            0.01,
            s,
            |value| format!("{:.0}%", value * 100.),
        ) {
            self.scale_edit.stage(preview);
        }
        if self
            .scale_edit
            .commit(&mut settings.ui_scale, active_slider("Interface scale"))
        {
            self.scale_anchor = Some(anchor);
        }
        label(
            "Release to apply",
            rect.x,
            rect.y + 67. * s,
            11. * s,
            muted_color(),
        );
    }
    pub(super) fn settings(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 158.0 * s;
        let col = (screen_width() - 110.0 * s) * 0.5;
        let rx = x + col + 30.0 * s;
        let by = y + 64.0 * s;
        // The animated background looks nice until a shape wanders under
        // a damn caption. Quiet cards keep controls readable on every tab.
        let card_top = by - 11.0 * s;
        let card_height = (screen_height() - 79.0 * s - card_top).max(0.0);
        for px in [x - 12.0 * s, rx - 12.0 * s] {
            let card = Rect::new(px, card_top, col + 24.0 * s, card_height);
            draw_rectangle(card.x, card.y, card.w, card.h, alpha(surface_color(), 0.55));
            draw_rectangle_lines(card.x, card.y, card.w, card.h, s, alpha(line_color(), 0.55));
        }
        match self.settings_tab {
            0 => {
                if toggle(
                    Rect::new(x, by, col, 33.0 * s),
                    "Fullscreen [F11]",
                    &mut settings.fullscreen,
                    s,
                ) {
                    return Action::Save;
                }
                toggle(
                    Rect::new(x, by + 47.0 * s, col, 33.0 * s),
                    "VSync  (restart required)",
                    &mut settings.vsync,
                    s,
                );
                toggle(
                    Rect::new(x, by + 94.0 * s, col, 33.0 * s),
                    "FPS and network diagnostics",
                    &mut settings.show_fps,
                    s,
                );
                self.interface_scale(
                    Rect::new(x, by + 154.0 * s, col, 48.0 * s),
                    settings,
                    s,
                    408.,
                );
                wrap(
                    "Rendering quality, upscaling and antialiasing are on the Graphics tab.",
                    x,
                    by + 252.0 * s,
                    col,
                    15.0 * s,
                    muted_color(),
                );
                // Macs pace frames with the display link, which has no
                // adaptive mode.
                if settings.vsync && !cfg!(target_os = "macos") {
                    toggle(
                        Rect::new(x, by + 300.0 * s, col, 33.0 * s),
                        "Adaptive VSync  (restart required)",
                        &mut settings.adaptive_vsync,
                        s,
                    );
                }
                label(
                    "RESOLUTION · render size in fullscreen",
                    rx,
                    by + DISPLAY_RESOLUTION_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                let native_title = crate::native_resolution::current().map_or_else(
                    || "Native (monitor resolution)".to_owned(),
                    |(width, height)| format!("Native · {width} × {height}"),
                );
                if button(
                    Rect::new(rx, by + DISPLAY_RESOLUTION_ROW * s, col, 28. * s),
                    &native_title,
                    settings.native_resolution,
                    true,
                    s * 0.85,
                ) {
                    settings.native_resolution = true;
                    return Action::Save;
                }
                for (index, (width, height)) in
                    [(1024, 768), (1280, 800), (1600, 900), (1920, 1080)]
                        .into_iter()
                        .enumerate()
                {
                    if button(
                        Rect::new(
                            rx,
                            by + (DISPLAY_RESOLUTION_ROW
                                + (index + 1) as f32 * DISPLAY_RESOLUTION_STEP)
                                * s,
                            col,
                            28.0 * s,
                        ),
                        &format!("{width} × {height}"),
                        !settings.native_resolution
                            && settings.width == width
                            && settings.height == height,
                        true,
                        s * 0.85,
                    ) {
                        settings.native_resolution = false;
                        settings.width = width;
                        settings.height = height;
                        return Action::Save;
                    }
                }
                label(
                    "FRAME LIMIT",
                    rx,
                    by + DISPLAY_FRAME_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (index, cap) in [0, 60, 120, 144, 240].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + index as f32 * (col / 5.0),
                            by + DISPLAY_FRAME_ROW * s,
                            col / 5.0 - 7.0 * s,
                            34.0 * s,
                        ),
                        &if cap == 0 {
                            "∞".into()
                        } else {
                            cap.to_string()
                        },
                        settings.frame_cap == cap,
                        true,
                        s * 0.73,
                    ) {
                        settings.frame_cap = cap;
                    }
                }
                if toggle(
                    Rect::new(rx, by + DISPLAY_BATTERY_ROW * s, col, 33.0 * s),
                    "Battery saver",
                    &mut settings.battery_saver,
                    s,
                ) {
                    return Action::Save;
                }
                wrap(
                    if cfg!(target_os = "macos") {
                        "Games run at up to 60 fps and menus at 30. Macs with two GPUs use the integrated one after a restart."
                    } else {
                        "Games run at up to 60 fps and menus at 30."
                    },
                    rx,
                    by + DISPLAY_BATTERY_NOTE * s,
                    col,
                    11.0 * s,
                    muted_color(),
                );
            }
            7 => {
                if let Some(action) = self.graphics(settings, x, by, col, s) {
                    return action;
                }
                label(
                    "MSAA  (restart to apply)",
                    rx,
                    by + GRAPHICS_AA_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (index, (samples, title)) in [(1, "Off"), (4, "4x MSAA"), (8, "8x MSAA")]
                    .into_iter()
                    .enumerate()
                {
                    if button(
                        Rect::new(
                            rx + index as f32 * (col / 3.0),
                            by + GRAPHICS_AA_ROW * s,
                            col / 3.0 - 6.0 * s,
                            31.0 * s,
                        ),
                        title,
                        crate::graphics::samples(settings.anti_aliasing) == samples,
                        crate::graphics::supports_samples(samples),
                        s * 0.73,
                    ) {
                        settings.anti_aliasing = samples;
                        let _ = settings.save();
                    }
                }
                label(
                    "SHAPE QUALITY",
                    rx,
                    by + GRAPHICS_SHAPE_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (i, title) in ["Low", "Normal", "High"].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + i as f32 * col / 3.0,
                            by + GRAPHICS_SHAPE_ROW * s,
                            col / 3.0 - 6.0 * s,
                            31.0 * s,
                        ),
                        title,
                        usize::from(settings.shape_quality) == i,
                        true,
                        s * 0.72,
                    ) {
                        settings.shape_quality = i as u8;
                    }
                }
                label(
                    "TEXTURE FILTERING",
                    rx,
                    by + GRAPHICS_TEXTURE_LABEL * s,
                    13.0 * s,
                    muted_color(),
                );
                for (i, level) in [1, 2, 4, 8, 16].into_iter().enumerate() {
                    if button(
                        Rect::new(
                            rx + i as f32 * col / 5.0,
                            by + GRAPHICS_TEXTURE_ROW * s,
                            col / 5.0 - 6.0 * s,
                            31.0 * s,
                        ),
                        &format!("{level}x"),
                        settings.anisotropic_filtering == level,
                        level <= crate::graphics::max_anisotropy(),
                        s * 0.72,
                    ) {
                        settings.anisotropic_filtering = level;
                    }
                }
                label(
                    if crate::graphics::texture_count() == 0 {
                        "Sharpens textured tanks from mods."
                    } else if crate::graphics::max_anisotropy() <= 1 {
                        "This graphics device supports standard filtering."
                    } else {
                        "Filtering sharpens textured tanks viewed at an angle."
                    },
                    rx,
                    by + GRAPHICS_TEXTURE_NOTE * s,
                    11. * s,
                    muted_color(),
                );
                if !cfg!(target_os = "macos")
                    && let Some(device) = crate::hardware::device()
                {
                    label(
                        "GRAPHICS DEVICE",
                        rx,
                        by + GRAPHICS_DEVICE_LABEL * s,
                        13.0 * s,
                        muted_color(),
                    );
                    let below = wrap(
                        &device.summary(),
                        rx,
                        by + GRAPHICS_DEVICE_TEXT * s,
                        col,
                        12.0 * s,
                        text_color(),
                    );
                    let (note, color) = if device.software {
                        (
                            "No hardware acceleration: this is a software renderer. Install the graphics driver for your GPU.",
                            RED,
                        )
                    } else if device.offloaded {
                        ("Running on the discrete NVIDIA GPU.", muted_color())
                    } else {
                        ("Hardware accelerated.", muted_color())
                    };
                    wrap(note, rx, below + 6.0 * s, col, 11.0 * s, color);
                }
                if cfg!(target_os = "macos") {
                    if toggle(
                        Rect::new(rx, by + GRAPHICS_RENDERER_ROW * s, col, 33.0 * s),
                        "Metal renderer  (restart to apply)",
                        &mut settings.metal,
                        s,
                    ) {
                        let _ = settings.save();
                    }
                    let gpu = crate::hardware::device()
                        .map(|device| device.name())
                        .filter(|name| !name.is_empty());
                    let note = match (crate::graphics::metal(), settings.metal) {
                        (true, true) => match gpu {
                            Some(gpu) => format!("Rendering with Metal on {gpu}."),
                            None => "Rendering with Metal.".into(),
                        },
                        (false, false) => {
                            "Rendering with OpenGL, which macOS has deprecated. Metal is faster."
                                .into()
                        }
                        (_, true) => "Metal starts after a restart.".into(),
                        (_, false) => "OpenGL starts after a restart.".into(),
                    };
                    wrap(
                        &note,
                        rx,
                        by + GRAPHICS_RENDERER_NOTE * s,
                        col,
                        11.0 * s,
                        muted_color(),
                    );
                }
            }
            1 => {
                label("Appearance", x, by + 20.0 * s, 25.0 * s, text_color());
                wrap(
                    "Adjust motion, effects, and colors.",
                    x,
                    by + 57.0 * s,
                    col,
                    20.0 * s,
                    muted_color(),
                );
                if toggle(
                    Rect::new(x, by + 165.0 * s, col, 33.0 * s),
                    "Dark theme [F9]",
                    &mut settings.dark_arena,
                    s,
                ) {
                    let _ = settings.save();
                }
                slider(
                    Rect::new(x, by + 225.0 * s, col, 48.0 * s),
                    "Visual effects",
                    &mut settings.effects,
                    0.0,
                    1.0,
                    s,
                );
                slider(
                    Rect::new(x, by + 305.0 * s, col, 48.0 * s),
                    "Camera shake",
                    &mut settings.shake,
                    0.0,
                    1.0,
                    s,
                );
                slider(
                    Rect::new(rx, by + 50.0 * s, col, 48.0 * s),
                    "Camera smoothing",
                    &mut settings.camera_smoothing,
                    0.0,
                    1.0,
                    s,
                );
            }
            2 => {
                toggle(
                    Rect::new(rx, by, col, 33.0 * s),
                    "Auto fire",
                    &mut settings.auto_fire,
                    s,
                );
                let names = [
                    "Move forward",
                    "Move backward",
                    "Move left",
                    "Move right",
                    "Active ability",
                    "Toggle auto fire",
                ];
                for (index, name) in names.iter().enumerate() {
                    let r = Rect::new(x, by + index as f32 * 53.0 * s, col, 42.0 * s);
                    if button(r, name, self.binding == Some(index), true, s * 0.9) {
                        self.binding = Some(index);
                    }
                    right_label(
                        if self.binding == Some(index) {
                            "PRESS A KEY"
                        } else {
                            &settings.bindings[index]
                        },
                        x + col - 16.0 * s,
                        r.y + 28.0 * s,
                        16.0 * s,
                        accent(),
                    );
                }
                if let Some(index) = self.binding
                    && let Some(binding) = bindable_pressed()
                {
                    settings.bindings[index] = binding;
                    self.binding = None;
                }
                wrap(
                    "Aim with the mouse. Left click fires and directs drones to the cursor. Right click recalls drones. Keys 1–8 buy stats; hold Shift to fill a stat. Tab opens tank upgrades. Escape pauses.",
                    rx,
                    by + 86.0 * s,
                    col,
                    21.0 * s,
                    text_color(),
                );
                wrap(
                    "Click a control, then press a letter, arrow key, Space, LeftShift, or LeftControl.",
                    rx,
                    by + 256.0 * s,
                    col,
                    18.0 * s,
                    muted_color(),
                );
                if small_button(
                    Rect::new(rx, by + 350.0 * s, col, 36.0 * s),
                    "Restore default bindings",
                    s * 0.9,
                ) {
                    settings.bindings = Settings::default().bindings;
                    self.binding = None;
                }
            }
            3 => {
                toggle(
                    Rect::new(x, by, col, 33.0 * s),
                    "Colorblind palette",
                    &mut settings.colorblind,
                    s,
                );
                toggle(
                    Rect::new(x, by + 58.0 * s, col, 33.0 * s),
                    "Reduced motion",
                    &mut settings.reduced_motion,
                    s,
                );
                toggle(
                    Rect::new(x, by + 116.0 * s, col, 33.0 * s),
                    "High-contrast labels",
                    &mut settings.high_contrast,
                    s,
                );
                self.interface_scale(
                    Rect::new(x, by + 188.0 * s, col, 48.0 * s),
                    settings,
                    s,
                    442.,
                );
                wrap(
                    "Reduced motion stops decorative animation, particles, camera easing and shake. Combat movement and attack warnings stay visible.",
                    rx,
                    by + 20.0 * s,
                    col,
                    17.0 * s,
                    text_color(),
                );
                crate::accessibility::configure(
                    settings.colorblind,
                    settings.high_contrast,
                    settings.reduced_motion,
                );
                self.accessibility_preview(
                    Rect::new(rx, by + 112. * s, col, 180. * s),
                    settings,
                    s,
                );
                label("LOCAL PROFILE", rx, by + 310.0 * s, 13.0 * s, accent());
                label(
                    &format!(
                        "{} matches  ·  {} kills  ·  Best {}",
                        settings.games_played, settings.total_kills, settings.highest_score
                    ),
                    rx,
                    by + 343.0 * s,
                    18.0 * s,
                    text_color(),
                );
            }
            5 => {
                label("Proximity voice", x, by + 20. * s, 25. * s, text_color());
                wrap(
                    "Nearby players become quieter as they move away. Your microphone starts muted. Use M or the microphone button to talk, and headphones to avoid feedback.",
                    x,
                    by + 60. * s,
                    col,
                    18. * s,
                    muted_color(),
                );
                slider(
                    Rect::new(rx, by + 30. * s, col, 48. * s),
                    "Master volume",
                    &mut settings.master_volume,
                    0.,
                    1.,
                    s,
                );
                slider(
                    Rect::new(rx, by + 120. * s, col, 48. * s),
                    "Nearby voices",
                    &mut settings.voice_volume,
                    0.,
                    1.,
                    s,
                );
                slider(
                    Rect::new(rx, by + 210. * s, col, 48. * s),
                    "Boss alarm",
                    &mut settings.effects_volume,
                    0.,
                    1.,
                    s,
                );
                if button(
                    Rect::new(rx, by + 294. * s, col, 34. * s),
                    "Preview boss alarm",
                    false,
                    true,
                    s * 0.8,
                ) {
                    return Action::PreviewAlarm;
                }
                wrap(
                    "Voice is available online while alive. Microphone and speaker devices follow your operating system's defaults. Server mutes also apply to voice.",
                    rx,
                    by + 352. * s,
                    col,
                    18. * s,
                    muted_color(),
                );
            }
            _ => {
                label("PLAYER NAME", x, by + 15.0 * s, 13.0 * s, muted_color());
                text_field(
                    Rect::new(x, by + 29.0 * s, col, 44.0 * s),
                    &mut settings.player_name,
                    &mut self.name_active,
                    24,
                    s,
                    false,
                );
                label(
                    "DEFAULT SERVER ADDRESS",
                    x,
                    by + 116.0 * s,
                    13.0 * s,
                    muted_color(),
                );
                text_field(
                    Rect::new(x, by + 130.0 * s, col, 44.0 * s),
                    &mut settings.server_address,
                    &mut self.address_active,
                    128,
                    s,
                    false,
                );
                wrap(
                    "Choose Favorites or Recent. Sort by address, move favorites with the arrows, or remove individual entries with ×.",
                    x,
                    by + 220. * s,
                    col,
                    18. * s,
                    muted_color(),
                );
                if let Err(error) = self.server_browser.draw(
                    settings,
                    &mut self.servers,
                    Rect::new(rx, by, col, (card_height - 22. * s).max(300. * s)),
                    s,
                ) {
                    self.error = error;
                }
                wrap(
                    "Settings file",
                    x,
                    by + 304.0 * s,
                    col,
                    19.0 * s,
                    muted_color(),
                );
                wrap(
                    &Settings::path().display().to_string(),
                    x,
                    by + 337.0 * s,
                    col,
                    13.0 * s,
                    muted_color(),
                );
            }
        }
        Action::None
    }
}
