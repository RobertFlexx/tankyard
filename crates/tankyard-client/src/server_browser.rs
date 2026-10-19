use crate::{
    settings::{ServerList, ServerSort, Settings, same_server},
    ui::*,
};
use macroquad::prelude::*;

#[derive(Default)]
pub struct ServerBrowser {
    list: ServerList,
    page: usize,
}

impl ServerBrowser {
    pub fn draw(&mut self, settings: &mut Settings, rect: Rect, s: f32) -> Result<(), String> {
        panel(rect);
        let x = rect.x + 16. * s;
        let width = rect.w - 32. * s;
        let mut changed = false;
        for (i, (list, title)) in [
            (ServerList::Favorites, "Favorites"),
            (ServerList::Recent, "Recent"),
        ]
        .into_iter()
        .enumerate()
        {
            if button(
                Rect::new(
                    x + i as f32 * (width + 8. * s) / 2.,
                    rect.y + 16. * s,
                    (width - 8. * s) / 2.,
                    34. * s,
                ),
                &format!("{title} ({})", settings.servers(list).len()),
                self.list == list,
                true,
                s * 0.8,
            ) {
                self.list = list;
                self.page = 0;
            }
        }
        let sort = settings.server_sort(self.list);
        let order = match sort {
            ServerSort::Saved if self.list == ServerList::Recent => "Newest first",
            ServerSort::Saved => "Saved order",
            ServerSort::Address => "Address A–Z",
            ServerSort::ReverseAddress => "Address Z–A",
        };
        if small_button(
            Rect::new(x, rect.y + 61. * s, width, 30. * s),
            &format!("Sort: {order}"),
            s * 0.75,
        ) {
            settings.cycle_server_sort(self.list);
            self.page = 0;
            changed = true;
        }
        let servers = settings.sorted_servers(self.list);
        // Reserve the tabs, ordering control, page controls, and clear button.
        let rows = ((rect.h / s - 220.) / 43.).floor().max(0.) as usize + 1;
        let max_page = servers.len().saturating_sub(1) / rows;
        self.page = self.page.min(max_page);
        if servers.is_empty() {
            wrap(
                if self.list == ServerList::Favorites {
                    "Save a favorite from Join server, or star an entry in Recent."
                } else {
                    "Servers you connect to appear here."
                },
                x,
                rect.y + 130. * s,
                width,
                17. * s,
                muted_color(),
            );
        }
        for (slot, address) in servers.iter().skip(self.page * rows).take(rows).enumerate() {
            let y = rect.y + (105. + slot as f32 * 43.) * s;
            let manual = self.list == ServerList::Favorites && sort == ServerSort::Saved;
            let controls = if manual {
                3
            } else if self.list == ServerList::Recent {
                2
            } else {
                1
            };
            let control_width = 30. * s;
            let reserved = controls as f32 * (control_width + 5. * s);
            if button(
                Rect::new(x, y, width - reserved, 35. * s),
                address,
                same_server(&settings.server_address, address),
                true,
                s * 0.8,
            ) {
                settings.server_address = address.clone();
                changed = true;
            }
            let cx = x + width - reserved + 5. * s;
            if manual {
                let index = self.page * rows + slot;
                if button(
                    Rect::new(cx, y, control_width, 35. * s),
                    "↑",
                    false,
                    index > 0,
                    s * 0.8,
                ) {
                    settings.move_favorite(address, -1);
                    changed = true;
                }
                if button(
                    Rect::new(cx + 35. * s, y, control_width, 35. * s),
                    "↓",
                    false,
                    index + 1 < servers.len(),
                    s * 0.8,
                ) {
                    settings.move_favorite(address, 1);
                    changed = true;
                }
            } else if self.list == ServerList::Recent {
                let favorite = settings.favorites.iter().any(|s| same_server(s, address));
                if button(
                    Rect::new(cx, y, control_width, 35. * s),
                    if favorite { "★" } else { "☆" },
                    favorite,
                    true,
                    s * 0.8,
                ) {
                    if favorite {
                        settings.remove_server(ServerList::Favorites, address);
                    } else {
                        settings.add_favorite(address)?;
                    }
                    changed = true;
                }
            }
            if small_button(
                Rect::new(x + width - control_width, y, control_width, 35. * s),
                "×",
                s * 0.85,
            ) {
                settings.remove_server(self.list, address);
                changed = true;
            }
        }
        let footer = rect.y + rect.h - 72. * s;
        if button(
            Rect::new(x, footer, 70. * s, 28. * s),
            "‹ Prev",
            false,
            self.page > 0,
            s * 0.7,
        ) {
            self.page = self.page.saturating_sub(1);
        }
        center_label(
            &format!("{} / {}", self.page + 1, max_page + 1),
            rect.center().x,
            footer + 19. * s,
            13. * s,
            muted_color(),
        );
        if button(
            Rect::new(x + width - 70. * s, footer, 70. * s, 28. * s),
            "Next ›",
            false,
            self.page < max_page,
            s * 0.7,
        ) {
            self.page += 1;
        }
        if button(
            Rect::new(x, rect.y + rect.h - 34. * s, width, 24. * s),
            if self.list == ServerList::Favorites {
                "Remove all favorites"
            } else {
                "Clear recent history"
            },
            false,
            !servers.is_empty(),
            s * 0.65,
        ) {
            match self.list {
                ServerList::Favorites => settings.favorites.clear(),
                ServerList::Recent => settings.recent_servers.clear(),
            }
            self.page = 0;
            changed = true;
        }
        if changed {
            settings.save()?;
        }
        Ok(())
    }
}
