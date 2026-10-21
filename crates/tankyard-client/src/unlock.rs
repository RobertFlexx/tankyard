//! The private edition gate runs before simulation, networking, mods or microphone setup.
use crate::ui::RED;
use crate::{credentials, settings::Settings, ui::*};
use macroquad::prelude::*;
use std::sync::mpsc;
use zeroize::{Zeroize, Zeroizing};

pub async fn require_password() -> bool {
    let Some(vault) = credentials::bundled_vault() else {
        return true;
    };
    let settings = Settings::load();
    set_theme(settings.dark_arena);
    let mut password = Zeroizing::new(String::new());
    let mut focused = true;
    let mut notice = String::new();
    let mut retry_at = 0.;
    let mut pending = None;
    let mut unlocked = false;
    loop {
        if is_quit_requested() {
            return false;
        }
        if unlocked && !is_mouse_button_down(MouseButton::Left) && !is_key_down(KeyCode::Enter) {
            while get_char_pressed().is_some() {}
            return true;
        }
        if let Some(receiver) = &pending {
            let receiver: &mpsc::Receiver<
                Result<Zeroizing<[u8; 32]>, tankyard_client::credential_vault::Error>,
            > = receiver;
            match receiver.try_recv() {
                Ok(Ok(seed)) => {
                    unlocked = credentials::unlock(seed);
                    password.zeroize();
                    notice = if unlocked {
                        "Unlocked."
                    } else {
                        "Unable to unlock this edition."
                    }
                    .into();
                    pending = None;
                }
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    password.zeroize();
                    notice = "Incorrect password. Try again.".into();
                    retry_at = get_time() + 2.;
                    focused = true;
                    pending = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        clear_background(bg_color());
        let s = (screen_width() / 760.)
            .min(screen_height() / 540.)
            .clamp(0.1, 1.4);
        let r = Rect::new(
            (screen_width() - 500. * s) / 2.,
            (screen_height() - 340. * s) / 2.,
            500. * s,
            340. * s,
        );
        panel(r);
        center_label(
            if vault.identity == "creator" {
                "CREATOR EDITION"
            } else {
                "CENTRAL OWNER EDITION"
            },
            r.center().x,
            r.y + 48. * s,
            25. * s,
            text_color(),
        );
        center_label(
            "Enter your unlock password",
            r.center().x,
            r.y + 88. * s,
            17. * s,
            muted_color(),
        );
        let available = pending.is_none() && !unlocked && get_time() >= retry_at;
        set_interactive(available);
        text_field(
            Rect::new(r.x + 36. * s, r.y + 116. * s, r.w - 72. * s, 46. * s),
            &mut password,
            &mut focused,
            256,
            s,
            true,
        );
        let submit = button(
            Rect::new(r.x + 36. * s, r.y + 188. * s, r.w - 72. * s, 42. * s),
            if pending.is_some() {
                "Unlocking…"
            } else {
                "Unlock"
            },
            true,
            available && !password.is_empty(),
            s,
        );
        if available && !password.is_empty() && (submit || is_key_pressed(KeyCode::Enter)) {
            let (reply, receiver) = mpsc::channel();
            let secret = Zeroizing::new(std::mem::take(&mut *password));
            let expected = vault.expected;
            let envelope = vault.envelope;
            match std::thread::Builder::new()
                .name("credential-unlock".into())
                .spawn(move || {
                    let _ = reply.send(tankyard_client::credential_vault::open(
                        envelope, &secret, &expected,
                    ));
                }) {
                Ok(_) => {
                    pending = Some(receiver);
                    notice.clear();
                }
                Err(_) => {
                    notice = "Unlock is unavailable. Please try again.".into();
                }
            }
        }
        set_interactive(true);
        center_label(
            &fitted_label(&notice, 13. * s, r.w - 60. * s),
            r.center().x,
            r.y + 263. * s,
            13. * s,
            if unlocked { CYAN } else { RED },
        );
        center_label(
            "Keep the password separate from the app.",
            r.center().x,
            r.y + 309. * s,
            12. * s,
            muted_color(),
        );
        next_frame().await;
    }
}
