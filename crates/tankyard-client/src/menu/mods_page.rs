//! Pack library, safety review and staff-pack controls.
use super::*;

impl Menu {
    pub(super) fn mods(&mut self, settings: &mut Settings, s: f32) -> Action {
        let x = 40.0 * s;
        let y = 169.0 * s;
        let w = screen_width() - x * 2.0;
        label("Mods", x, y, 30.0 * s, text_color());
        label(
            "Lua script packs and data packs for offline games and servers you host.",
            x,
            y + 31.0 * s,
            16.0 * s,
            muted_color(),
        );
        if small_button(
            Rect::new(
                screen_width() - 165.0 * s,
                // Below the page's scroll clip, level with the title.
                y - 20.0 * s,
                125.0 * s,
                30.0 * s,
            ),
            "Refresh",
            s,
        ) {
            self.mods_scanned = false;
            self.catalog_stale = true;
        }
        if small_button(
            Rect::new(
                screen_width() - 375.0 * s,
                y - 20.0 * s,
                200.0 * s,
                30.0 * s,
            ),
            "Open mods folder",
            s,
        ) {
            let folder = crate::mod_library::library_dir();
            match crate::file_dialog::reveal(&folder) {
                Ok(()) => {
                    self.info = format!(
                        "Opened {}. Drop pack folders or .zip files there, then press Refresh.",
                        folder.display()
                    )
                }
                Err(error) => self.error = error,
            }
        }
        if let Some(result) = self.mod_chooser.as_ref().and_then(|chooser| chooser.poll()) {
            self.mod_chooser = None;
            match result {
                Ok(Some(path)) => self.import_result(settings, crate::mod_library::import(&path)),
                Ok(None) => {}
                Err(error) => self.error = error,
            }
        }
        if !self.mods_scanned {
            self.mods_scanned = true;
            self.mod_review = None;
            self.mod_viewer = None;
            self.mod_entries = crate::mod_library::discover();
            self.mod_icons.clear();
            self.mod_selected = self
                .mod_selected
                .min(self.mod_entries.len().saturating_sub(1));
            if let Some(focus) = self.mod_focus.take()
                && let Some(index) = self
                    .mod_entries
                    .iter()
                    .position(|entry| entry.id() == Some(focus.as_str()))
            {
                self.mod_selected = index;
                let visible_from = index.saturating_sub(3);
                self.mod_scroll = self.mod_scroll.min(index).max(visible_from);
            }
            if self.preview_review {
                self.mod_review = self
                    .mod_entries
                    .iter()
                    .position(|entry| entry.verdict() != tankyard_mods::scan::Verdict::Clean);
            }
            if self.preview_files
                && let Some(index) = self.mod_entries.iter().position(|entry| entry.scripted())
            {
                self.mod_selected = index;
                self.open_files(index, None);
            }
        }
        // Import row.
        let import_y = y + 52.0 * s;
        label(
            "ADD A MOD PACK",
            x,
            import_y + 12.0 * s,
            11.0 * s,
            muted_color(),
        );
        let choosing = self.mod_chooser.is_some();
        if button(
            Rect::new(x, import_y + 20.0 * s, 190.0 * s, 38.0 * s),
            if choosing {
                "Choosing…"
            } else {
                "Browse for a pack…"
            },
            true,
            !choosing,
            s * 0.82,
        ) {
            self.mod_chooser = Some(crate::file_dialog::choose_mod_pack());
            self.error.clear();
            self.info = "Pick a .zip pack, or the mod.json inside a pack folder.".into();
        }
        let field_x = x + 200.0 * s;
        let field_w = (w - 336.0 * s).min(560.0 * s);
        text_field(
            Rect::new(field_x, import_y + 20.0 * s, field_w, 38.0 * s),
            &mut self.mod_path,
            &mut self.mod_path_active,
            512,
            s * 0.8,
            false,
        );
        if button(
            Rect::new(
                field_x + field_w + 10.0 * s,
                import_y + 20.0 * s,
                120.0 * s,
                38.0 * s,
            ),
            "Import",
            !self.mod_path.trim().is_empty(),
            !self.mod_path.trim().is_empty(),
            s * 0.85,
        ) {
            let path = self.mod_path.trim().trim_matches('"').to_owned();
            self.import_result(
                settings,
                crate::mod_library::import(std::path::Path::new(&path)),
            );
            self.mod_path.clear();
        }
        label(
            "Or paste a .zip or pack folder path, or drop a .zip onto this window. Every pack is reviewed before it can run.",
            x,
            import_y + 76.0 * s,
            12.0 * s,
            muted_color(),
        );
        // Library list on the left, the selected pack's details on the right.
        let area_y = import_y + 92.0 * s;
        let area_h = (screen_height() - area_y - 112.0 * s).max(120.0 * s);
        let list_w = (w * 0.42).clamp(260.0 * s, 470.0 * s);
        let list = Rect::new(x, area_y, list_w, area_h);
        let detail = Rect::new(x + list_w + 14.0 * s, area_y, w - list_w - 14.0 * s, area_h);
        let row_h = 56.0 * s;
        let visible = ((list.h + 6.0 * s) / (row_h + 6.0 * s)).floor().max(1.0) as usize;
        let list_scrolls = self.mod_entries.len() > visible;
        if list_scrolls {
            crate::ui_layout::claim_wheel(list);
        }
        if list_scrolls && contains(list) && self.mod_review.is_none() {
            let steps = crate::ui_layout::scroll_steps(
                &mut self.mod_scroll_fraction,
                crate::ui_layout::wheel_lines().y,
            );
            if steps < 0 {
                self.mod_scroll += steps.unsigned_abs() as usize;
            } else if steps > 0 {
                self.mod_scroll = self.mod_scroll.saturating_sub(steps as usize);
            }
        }
        self.mod_scroll = self
            .mod_scroll
            .min(self.mod_entries.len().saturating_sub(visible));
        if self.mod_entries.is_empty() {
            panel(list);
            wrap(
                "No packs yet. Browse for a pack, drop a .zip onto this window, or put pack folders in the mods folder.",
                list.x + 16.0 * s,
                list.y + 30.0 * s,
                list.w - 32.0 * s,
                15.0 * s,
                muted_color(),
            );
            panel(detail);
            label(
                "Make your own",
                detail.x + 22.0 * s,
                detail.y + 36.0 * s,
                20.0 * s,
                text_color(),
            );
            wrap(
                "A pack is a folder with mod.json, optional Lua scripts and art, and a tankyard.toml with its name, creator, version, icon and links. See docs/modding.md.",
                detail.x + 22.0 * s,
                detail.y + 66.0 * s,
                detail.w - 44.0 * s,
                15.0 * s,
                muted_color(),
            );
        }
        let mut toggle = None;
        for (slot, index) in (self.mod_scroll..self.mod_entries.len())
            .take(visible)
            .enumerate()
        {
            let entry = &self.mod_entries[index];
            let r = Rect::new(
                list.x,
                list.y + slot as f32 * (row_h + 6.0 * s),
                list.w,
                row_h,
            );
            let enabled = entry.enabled(settings);
            let selected = self.mod_selected == index;
            // An outline marks the selection, keeping status colors readable.
            if button(r, "", false, true, s) {
                self.mod_selected = index;
            }
            if selected {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.5 * s, accent());
            }
            if enabled {
                draw_rectangle(r.x, r.y, 4.0 * s, r.h, accent());
            }
            let icon = Rect::new(r.x + 12.0 * s, r.y + 8.0 * s, 40.0 * s, 40.0 * s);
            self.mod_icon(index, icon, s);
            let entry = &self.mod_entries[index];
            let text_x = icon.right() + 12.0 * s;
            let switch = Rect::new(r.right() - 58.0 * s, r.y + 15.0 * s, 46.0 * s, 26.0 * s);
            let text_w = switch.x - text_x - 10.0 * s;
            let version = entry
                .version()
                .map(|v| format!("  v{v}"))
                .unwrap_or_default();
            label(
                &fitted_label(&format!("{}{version}", entry.name()), 16.0 * s, text_w),
                text_x,
                r.y + 24.0 * s,
                16.0 * s,
                text_color(),
            );
            let (status, color) = mod_status(entry, settings);
            let byline = match (&entry.error, entry.creator()) {
                (Some(_), _) => "Cannot load".to_owned(),
                (None, Some(creator)) => format!("{status} · by {creator}"),
                (None, None) => status.to_owned(),
            };
            label(
                &fitted_label(&byline, 12.0 * s, text_w),
                text_x,
                r.y + 43.0 * s,
                12.0 * s,
                if entry.error.is_some() { RED } else { color },
            );
            let mut on = enabled;
            if entry.error.is_none() && toggle_switch(switch, &mut on, s) {
                self.mod_selected = index;
                if !enabled && !entry.approved(settings) {
                    self.mod_review = Some(index);
                    self.mod_trust = false;
                } else {
                    toggle = Some(index);
                }
            }
        }
        if let Some(index) = toggle {
            self.toggle_mod(settings, index);
        }
        if !self.mod_entries.is_empty() {
            self.mod_details(settings, detail, s);
        }
        label(
            &fitted_label(
                &format!(
                    "Library: {}  ·  Scripts run in a Luau sandbox with no file, network or system access.",
                    crate::mod_library::library_dir().display()
                ),
                13.0 * s,
                w,
            ),
            x,
            screen_height() - 93.0 * s,
            13.0 * s,
            muted_color(),
        );
        Action::None
    }
    /// A pack's icon, or a tile with its initial when it has none.
    pub(super) fn mod_icon(&mut self, index: usize, rect: Rect, s: f32) {
        let entry = &self.mod_entries[index];
        let key = format!("{}#{}", entry.path, entry.digest);
        if let Some(data) = &entry.icon
            && !self.mod_icons.contains_key(&key)
        {
            let texture = Texture2D::from_rgba8(data.width, data.height, &data.rgba);
            texture.set_filter(FilterMode::Linear);
            self.mod_icons.insert(key.clone(), texture);
        }
        if let Some(texture) = self.mod_icons.get(&key) {
            // Fit inside the tile, keeping the icon's shape.
            let (tw, th) = (texture.width().max(1.0), texture.height().max(1.0));
            let scale = (rect.w / tw).min(rect.h / th);
            let size = vec2(tw * scale, th * scale);
            // Through the page offset, so icons scroll with their rows.
            crate::ui::ui_texture(
                texture,
                rect.center().x - size.x * 0.5,
                rect.center().y - size.y * 0.5,
                DrawTextureParams {
                    dest_size: Some(size),
                    ..Default::default()
                },
            );
            return;
        }
        let name = entry.name();
        // A stable color per pack from its name.
        let hue = name
            .bytes()
            .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
        let palette = [
            CYAN,
            RED,
            crate::ui::VIOLET,
            AMBER,
            Color::new(0.33, 0.78, 0.45, 1.0),
        ];
        let color = palette[hue as usize % palette.len()];
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, color);
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0 * s, alpha(BLACK, 0.25));
        let initial: String = name
            .chars()
            .next()
            .map_or('?', |c| c.to_ascii_uppercase())
            .to_string();
        center_label(
            &initial,
            rect.center().x,
            rect.center().y + rect.h * 0.2,
            rect.h * 0.55,
            WHITE,
        );
    }

    /// The selected pack: who made it, what it contains, where it lives, and
    /// whether it is safe to run.
    pub(super) fn mod_details(&mut self, settings: &mut Settings, r: Rect, s: f32) {
        let index = self.mod_selected.min(self.mod_entries.len() - 1);
        panel(r);
        let px = r.x + 22.0 * s;
        let pw = r.w - 44.0 * s;
        let icon = Rect::new(px, r.y + 20.0 * s, 84.0 * s, 84.0 * s);
        self.mod_icon(index, icon, s);
        let entry = &self.mod_entries[index];
        let tx = icon.right() + 18.0 * s;
        let tw = r.right() - 22.0 * s - tx;
        label(
            &fitted_label(entry.name(), 24.0 * s, tw),
            tx,
            r.y + 44.0 * s,
            24.0 * s,
            text_color(),
        );
        let mut facts = Vec::new();
        if let Some(version) = entry.version() {
            facts.push(format!("Version {version}"));
        }
        if let Some(creator) = entry.creator() {
            facts.push(format!("by {creator}"));
        }
        if let Some(license) = entry
            .project
            .as_ref()
            .and_then(|p| p.details.license.as_deref())
        {
            facts.push(format!("{license} license"));
        }
        if let Some(source) = entry.project.as_ref().map(|p| &p.source)
            && source.restricted()
        {
            facts.push(
                if source.viewable {
                    "some files private"
                } else {
                    "private source"
                }
                .to_owned(),
            );
        }
        label(
            &fitted_label(&facts.join("  ·  "), 13.0 * s, tw),
            tx,
            r.y + 66.0 * s,
            13.0 * s,
            muted_color(),
        );
        let (status, color) = mod_status(entry, settings);
        let badge_w = label_width(status, 11.0 * s) + 20.0 * s;
        let badge = Rect::new(tx, r.y + 78.0 * s, badge_w, 22.0 * s);
        capsule(badge, mix(surface_color(), color, 0.22));
        center_label(
            status,
            badge.center().x,
            badge.bottom() - 6.5 * s,
            11.0 * s,
            color,
        );
        let mut tag_x = badge.right() + 8.0 * s;
        let tags = entry
            .project
            .as_ref()
            .map(|p| p.details.tags.clone())
            .unwrap_or_default();
        for tag in tags {
            let tag_w = label_width(&tag, 11.0 * s) + 18.0 * s;
            if tag_x + tag_w > r.right() - 22.0 * s {
                break;
            }
            // Capsules overlap their parts, so they need an opaque color.
            capsule(
                Rect::new(tag_x, badge.y, tag_w, badge.h),
                mix(surface_color(), line_color(), 0.5),
            );
            center_label(
                &tag,
                tag_x + tag_w * 0.5,
                badge.bottom() - 6.5 * s,
                11.0 * s,
                text_color(),
            );
            tag_x += tag_w + 6.0 * s;
        }
        let mut y = r.y + 136.0 * s;
        let description = match (&entry.error, entry.description()) {
            (Some(error), _) => format!("This pack cannot load: {error}"),
            (None, Some(text)) => text.to_owned(),
            (None, None) => "No description.".to_owned(),
        };
        let lines = wrapped_lines(&description, pw, 14.0 * s).len().min(6);
        wrap(
            &description,
            px,
            y,
            pw,
            14.0 * s,
            if entry.error.is_some() {
                RED
            } else {
                text_color()
            },
        );
        y += lines as f32 * 19.0 * s + 14.0 * s;
        if let Some(manifest) = &entry.manifest {
            let count = |n: usize, one: &str, many: &str| match n {
                0 => None,
                1 => Some(format!("1 {one}")),
                n => Some(format!("{n} {many}")),
            };
            let contents: Vec<String> = [
                count(manifest.classes.len(), "tank", "tanks"),
                count(manifest.maps.len(), "map", "maps"),
                count(manifest.scripts.len(), "script", "scripts"),
                count(manifest.textures.len(), "texture", "textures"),
                count(manifest.hooks.len(), "hook", "hooks"),
                manifest.rules.as_ref().map(|_| "custom rules".to_owned()),
                (!manifest.dependencies.is_empty())
                    .then(|| format!("needs {}", manifest.dependencies.join(", "))),
            ]
            .into_iter()
            .flatten()
            .collect();
            label("CONTENTS", px, y, 11.0 * s, muted_color());
            y += 18.0 * s;
            let summary = if let Some(staff) = &entry.staff {
                format!(
                    "{}  ·  signed by {}  ·  {}",
                    staff.identity.title(),
                    tankyard_client::staff_pack::PUBLISHER,
                    if crate::credentials::unlocked(staff.identity) {
                        "unlocked"
                    } else {
                        "locked"
                    }
                )
            } else if contents.is_empty() {
                "Settings only".to_owned()
            } else {
                contents.join("  ·  ")
            };
            label(
                &fitted_label(&summary, 13.0 * s, pw),
                px,
                y,
                13.0 * s,
                text_color(),
            );
            y += 18.0 * s;
            let kb = entry.size as f32 / 1024.0;
            label(
                &fitted_label(
                    &format!(
                        "{} · {} · {} files · {} · id {}",
                        if entry.staff.is_some() {
                            "Staff pack"
                        } else if entry.scripted() {
                            "Lua script pack"
                        } else {
                            "Data pack"
                        },
                        if entry.archive { "zip" } else { "folder" },
                        entry.files,
                        if kb >= 1024.0 {
                            format!("{:.1} MB", kb / 1024.0)
                        } else if entry.size < 1024 {
                            format!("{} bytes", entry.size)
                        } else {
                            format!("{kb:.0} KB")
                        },
                        manifest.id
                    ),
                    12.0 * s,
                    pw,
                ),
                px,
                y,
                12.0 * s,
                muted_color(),
            );
            y += 22.0 * s;
        }
        if let Some(error) = &entry.project_error {
            label(&fitted_label(error, 12.0 * s, pw), px, y, 12.0 * s, RED);
            y += 18.0 * s;
        }
        let links: Vec<(&str, String)> = entry
            .project
            .as_ref()
            .map(|p| {
                p.links
                    .all()
                    .into_iter()
                    .map(|(l, u)| (l, u.to_owned()))
                    .collect()
            })
            .unwrap_or_default();
        let creator_url = entry.project.as_ref().and_then(|p| p.creator.url.clone());
        let mut link_x = px;
        let mut opened = None;
        for (title, url) in links
            .iter()
            .map(|(t, u)| (*t, u.as_str()))
            .chain(creator_url.as_deref().map(|u| ("Creator", u)))
        {
            let bw = label_width(title, 12.0 * s) + 36.0 * s;
            if link_x + bw > r.right() - 22.0 * s {
                break;
            }
            if small_button(
                Rect::new(link_x, y + 4.0 * s, bw, 30.0 * s),
                &format!("{title} ↗"),
                s * 0.72,
            ) {
                opened = Some(url.to_owned());
            }
            link_x += bw + 8.0 * s;
        }
        if let Some(url) = opened {
            match crate::links::open(&url) {
                Ok(()) => self.info = format!("Opened {url}"),
                Err(error) => self.error = error,
            }
        }
        let enabled = entry.enabled(settings);
        let footer = r.bottom() - 52.0 * s;
        let removable = entry.in_library();
        let mut view_files = false;
        let path_room = pw - if removable { 530.0 } else { 420.0 } * s;
        label(
            &fitted_label(&entry.path, 11.0 * s, path_room),
            px,
            footer + 22.0 * s,
            11.0 * s,
            muted_color(),
        );
        if removable {
            let confirming = self.mod_remove.as_deref() == Some(entry.path.as_str());
            if small_button(
                Rect::new(r.right() - 520.0 * s, footer, 102.0 * s, 34.0 * s),
                if confirming { "Confirm" } else { "Remove" },
                s * 0.78,
            ) {
                if confirming {
                    self.mod_remove = None;
                    let entry = entry.clone();
                    match crate::mod_library::remove(&entry) {
                        Ok(()) => {
                            if let Some(id) = entry.id() {
                                settings.enabled_mods.retain(|e| e != id);
                                let _ = settings.save();
                            }
                            self.info = format!("Removed {}.", entry.name());
                            self.error.clear();
                            self.mods_scanned = false;
                            self.catalog_stale = true;
                        }
                        Err(error) => self.error = error,
                    }
                    return;
                }
                self.mod_remove = Some(entry.path.clone());
                self.info = format!(
                    "Press Confirm to delete {} from the mods folder.",
                    entry.name()
                );
            }
        }
        if small_button(
            Rect::new(r.right() - 410.0 * s, footer, 110.0 * s, 34.0 * s),
            "View files",
            s * 0.78,
        ) {
            view_files = true;
        }
        let locked_staff = entry
            .staff
            .as_ref()
            .is_some_and(|staff| enabled && !crate::credentials::unlocked(staff.identity));
        if locked_staff {
            if small_button(
                Rect::new(r.right() - 290.0 * s, footer, 120.0 * s, 34.0 * s),
                "Unlock…",
                s * 0.78,
            ) {
                self.staff_unlock = true;
            }
        } else if small_button(
            Rect::new(r.right() - 290.0 * s, footer, 120.0 * s, 34.0 * s),
            "Safety review",
            s * 0.78,
        ) {
            self.mod_review = Some(index);
            self.mod_trust = false;
        }
        // Packs that still need confirmation say so instead of looking safe.
        let needs_review = !enabled && !entry.approved(settings);
        if button(
            Rect::new(r.right() - 162.0 * s, footer, 140.0 * s, 34.0 * s),
            if enabled {
                "Disable"
            } else if needs_review {
                "Review & enable"
            } else {
                "Enable"
            },
            !enabled && !needs_review,
            entry.error.is_none(),
            s * 0.78,
        ) {
            if !enabled && !entry.approved(settings) {
                self.mod_review = Some(index);
                self.mod_trust = false;
            } else {
                self.toggle_mod(settings, index);
            }
        }
        if view_files {
            self.open_files(index, None);
        }
    }

    /// The review sheet for one pack, drawn above the page and its canvas.
    pub(super) fn mod_review_overlay(&mut self, settings: &mut Settings, s: f32) {
        use tankyard_mods::scan::{Severity, Verdict};
        if let Some(index) = self.mod_review
            && let Some(entry) = self.mod_entries.get(index)
        {
            let panel_w = (720.0 * s).min(screen_width() - 48.0 * s);
            let panel_h = (470.0 * s).min(screen_height() - 160.0 * s);
            let r = Rect::new(
                (screen_width() - panel_w) * 0.5,
                (screen_height() - panel_h) * 0.5,
                panel_w,
                panel_h,
            );
            draw_rectangle(
                0.,
                0.,
                screen_width(),
                screen_height(),
                alpha(bg_color(), 0.55),
            );
            panel(r);
            let verdict = entry.verdict();
            let (title, color) = match verdict {
                Verdict::Clean => ("No problems found", Color::new(0.30, 0.64, 0.30, 1.0)),
                Verdict::Review => ("Review before enabling", warm_text_color()),
                Verdict::Dangerous => ("This pack tries to reach your computer", RED),
            };
            draw_rectangle(r.x, r.y, r.w, 5.0 * s, color);
            label(
                &fitted_label(entry.name(), 22.0 * s, r.w - 40.0 * s),
                r.x + 20.0 * s,
                r.y + 38.0 * s,
                22.0 * s,
                text_color(),
            );
            label(title, r.x + 20.0 * s, r.y + 62.0 * s, 14.0 * s, color);
            let summary = match verdict {
                Verdict::Clean => {
                    "Nothing in this pack tries to use system commands, files, native code or hidden data."
                }
                Verdict::Review => {
                    "Some contents are unusual for a game mod. The sandbox still blocks system access, but check that you expected them."
                }
                Verdict::Dangerous => {
                    "The sandbox blocks these attempts, but a pack that makes them was probably written to cause harm. Only continue if you fully trust its author."
                }
            };
            let mut ly = wrap(
                summary,
                r.x + 20.0 * s,
                r.y + 88.0 * s,
                r.w - 40.0 * s,
                13.0 * s,
                muted_color(),
            ) + 8.0 * s;
            let footer = r.bottom() - 62.0 * s;
            let findings = &entry.report.findings;
            let mut open_finding = None;
            if findings.is_empty() {
                label(
                    "No findings.",
                    r.x + 20.0 * s,
                    ly + 12.0 * s,
                    13.0 * s,
                    muted_color(),
                );
            }
            for finding in findings {
                if ly + 20.0 * s
                    > footer
                        - (if verdict == Verdict::Dangerous {
                            30.0
                        } else {
                            0.0
                        }) * s
                {
                    label("…", r.x + 20.0 * s, ly + 10.0 * s, 13.0 * s, muted_color());
                    break;
                }
                let (tag, tint) = match finding.severity {
                    Severity::Danger => ("DANGER", RED),
                    Severity::Warning => ("REVIEW", warm_text_color()),
                    Severity::Info => ("NOTE", muted_color()),
                };
                // Each finding opens the file browser at its line.
                let row = Rect::new(r.x + 14.0 * s, ly - 2.0 * s, r.w - 28.0 * s, 20.0 * s);
                if contains(row) {
                    draw_rectangle(row.x, row.y, row.w, row.h, alpha(hover_color(), 0.6));
                    if is_mouse_button_pressed(MouseButton::Left) {
                        open_finding = Some((finding.file.clone(), finding.line));
                    }
                }
                label(tag, r.x + 20.0 * s, ly + 12.0 * s, 10.0 * s, tint);
                let place = if finding.line > 0 {
                    format!("{}:{}", finding.file, finding.line)
                } else {
                    finding.file.clone()
                };
                label(
                    &fitted_label(
                        &format!("{place} — {}", finding.message),
                        12.0 * s,
                        r.w - 110.0 * s,
                    ),
                    r.x + 84.0 * s,
                    ly + 12.0 * s,
                    12.0 * s,
                    text_color(),
                );
                ly += 20.0 * s;
            }
            let enabled = entry.enabled(settings);
            let needs_trust = verdict == Verdict::Dangerous && !entry.approved(settings);
            if needs_trust {
                crate::ui::toggle(
                    Rect::new(r.x + 20.0 * s, footer - 34.0 * s, r.w - 40.0 * s, 30.0 * s),
                    "I trust this pack's author and accept the risk",
                    &mut self.mod_trust,
                    s * 0.8,
                );
            }
            let by = r.bottom() - 52.0 * s;
            if small_button(
                Rect::new(r.x + 20.0 * s, by, 110.0 * s, 36.0 * s),
                "Close",
                s * 0.85,
            ) {
                self.mod_review = None;
            }
            if !enabled
                && entry.error.is_none()
                && button(
                    Rect::new(r.right() - 220.0 * s, by, 200.0 * s, 36.0 * s),
                    if verdict == Verdict::Clean {
                        "Enable"
                    } else {
                        "Enable anyway"
                    },
                    verdict == Verdict::Clean,
                    !needs_trust || self.mod_trust,
                    s * 0.85,
                )
            {
                if verdict != Verdict::Clean && !settings.approved_mods.contains(&entry.digest) {
                    settings.approved_mods.push(entry.digest.clone());
                }
                self.mod_review = None;
                self.toggle_mod(settings, index);
            }
            if is_key_pressed(KeyCode::Escape) && self.mod_viewer.is_none() {
                self.mod_review = None;
            }
            if let Some((file, line)) = open_finding {
                self.open_files(index, Some((&file, line as usize)));
            }
        }
    }
    /// Open the file browser on a pack, optionally at a file and line.
    pub(super) fn open_files(&mut self, index: usize, focus: Option<(&str, usize)>) {
        let Some(entry) = self.mod_entries.get(index) else {
            return;
        };
        match crate::mod_viewer::Viewer::open(entry.name(), &entry.path, focus) {
            Ok(viewer) => self.mod_viewer = Some(viewer),
            Err(error) => self.error = format!("Could not open {}: {error}", entry.name()),
        }
    }
    /// Report enabled packs that were held back from loading.
    pub fn held_mods(&mut self, held: &[String], missing: Vec<String>) {
        self.error = held.join(" ");
        self.mod_missing = (!missing.is_empty()).then(|| (self.error.clone(), missing));
    }
    /// Enabled packs that are gone, while the message about them is shown.
    pub(super) fn missing_offer(&self) -> Option<&[String]> {
        self.mod_missing
            .as_ref()
            .filter(|(message, _)| !self.error.is_empty() && *message == self.error)
            .map(|(_, missing)| missing.as_slice())
    }
    /// The width the bottom message wraps to, leaving room for its switch.
    pub(super) fn message_room(&self, w: f32, margin: f32, s: f32) -> f32 {
        let switch = if self.missing_offer().is_some() {
            136.
        } else {
            0.
        };
        (w - margin * 2. - (24. + switch) * s).max(1.)
    }
    pub(super) fn toggle_mod(&mut self, settings: &mut Settings, index: usize) {
        let Some(entry) = self.mod_entries.get(index) else {
            return;
        };
        let Some(id) = entry.id().map(str::to_owned) else {
            return;
        };
        let previous = settings.enabled_mods.clone();
        if let Some(staff) = &entry.staff {
            // Staff packs never load into a match; they only lock or unlock tools.
            if settings.enabled_mods.contains(&id) {
                settings.enabled_mods.retain(|e| e != &id);
                crate::credentials::lock(staff.identity);
                self.staff_changed = true;
                self.info = format!("{} locked and disabled.", staff.identity.title());
            } else {
                settings.enabled_mods.push(id);
                self.staff_unlock = true;
                self.info.clear();
            }
            self.error.clear();
            let _ = settings.save();
            return;
        }
        if settings.enabled_mods.contains(&id) {
            settings.enabled_mods.retain(|e| e != &id);
        } else if settings.enabled_mods.len() >= tankyard_mods::MAX_MODS {
            self.error = "At most 16 packs can be enabled at once.".into();
            return;
        } else {
            settings.enabled_mods.push(id);
        }
        let ready = crate::mod_library::ready_paths(settings);
        match crate::mod_library::load(&ready.paths) {
            Ok(loaded) => {
                self.maps = loaded.bundle.maps;
                self.map = self.map.min(self.maps.len().saturating_sub(1));
                self.held_mods(&ready.held, ready.missing);
                self.info.clear();
                let _ = settings.save();
            }
            Err(error) => {
                settings.enabled_mods = previous;
                self.error = error;
            }
        }
    }
    /// Report an import. A pack that passed review is enabled right away;
    /// a flagged one waits for the player's review.
    pub fn import_result(
        &mut self,
        settings: &mut Settings,
        result: Result<crate::mod_library::Entry, String>,
    ) {
        match result {
            Ok(entry) => {
                self.error.clear();
                if let Some(error) = &entry.error {
                    // A staff pack whose signature failed is kept out of use.
                    self.info.clear();
                    self.error = format!("Added {}, but it cannot load: {error}", entry.name());
                    self.mods_scanned = false;
                    self.catalog_stale = true;
                    return;
                }
                let clean = entry.verdict() == tankyard_mods::scan::Verdict::Clean;
                let id = entry.id().unwrap_or_default().to_owned();
                let room = settings.enabled_mods.len() < tankyard_mods::MAX_MODS;
                self.info = if clean && (entry.enabled(settings) || room) {
                    if !entry.enabled(settings) {
                        settings.enabled_mods.push(id.clone());
                        let _ = settings.save();
                    }
                    if entry.staff.is_some() {
                        self.staff_unlock = true;
                        format!("Added {}. Enter its password to unlock it.", entry.name())
                    } else {
                        format!("Added and enabled {}. It passed review.", entry.name())
                    }
                } else if clean {
                    format!(
                        "Added {}. Disable another pack to enable it (16 at most).",
                        entry.name()
                    )
                } else {
                    format!(
                        "Added {}. Review its findings before enabling it.",
                        entry.name()
                    )
                };
                self.mod_focus = Some(id);
                self.mods_scanned = false;
                self.catalog_stale = true;
            }
            Err(error) => {
                self.info.clear();
                self.error = format!("Import failed: {error}");
            }
        }
    }
}
