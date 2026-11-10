//! Staff packs ask for their password before simulation, networking, mods or
//! microphone setup, and again from Mods for a pack the player skipped.
use crate::ui::RED;
use crate::{ambient::Ambient, credentials, settings::Settings, ui::*};
use macroquad::prelude::*;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    mpsc,
};
use tankyard_client::{
    credential_vault,
    staff_pack::{Identity, PUBLISHER, Verified},
};
use zeroize::{Zeroize, Zeroizing};

/// Failed attempts this run, across every prompt, so reopening the prompt
/// never resets the wait between guesses.
static FAILURES: AtomicU32 = AtomicU32::new(0);

/// Seconds to wait after the `failures`th wrong password: 2, 4, 8 … 60.
fn backoff(failures: u32) -> f64 {
    if failures == 0 {
        0.
    } else {
        f64::from(1_u32 << failures.min(6)).min(60.)
    }
}

pub enum Outcome {
    /// Every pack was unlocked or skipped.
    Done,
    Quit,
}

/// Ask for each locked pack's password in turn. Skipping leaves its tools
/// locked for this run; Mods can unlock it later.
/// `backdrop` is the live arena to show behind the prompt, if any.
pub async fn unlock_packs(
    packs: &[(String, Verified)],
    mut backdrop: Option<&mut Ambient>,
) -> Outcome {
    for (name, pack) in packs {
        if credentials::unlocked(pack.identity) {
            continue;
        }
        if !prompt(name, pack, backdrop.as_deref_mut()).await {
            return Outcome::Quit;
        }
    }
    Outcome::Done
}

type Pending = mpsc::Receiver<Result<Zeroizing<[u8; 32]>, credential_vault::Error>>;

/// One pack's prompt. False when the player quit the game.
async fn prompt(name: &str, pack: &Verified, mut backdrop: Option<&mut Ambient>) -> bool {
    let settings = Settings::load();
    set_theme(settings.dark_arena);
    let mut password = Zeroizing::new(String::new());
    let mut focused = true;
    let mut notice = String::new();
    let mut pending: Option<Pending> = None;
    let mut unlocked_at = None;
    let mut retry_at = get_time() + backoff(FAILURES.load(Ordering::Relaxed));
    loop {
        if is_quit_requested() {
            return false;
        }
        // Hold the result on screen briefly, and wait for the click or key
        // that unlocked it to end so the menu does not receive it.
        if let Some(at) = unlocked_at
            && get_time() - at > 0.6
            && !is_mouse_button_down(MouseButton::Left)
            && !is_key_down(KeyCode::Enter)
        {
            while get_char_pressed().is_some() {}
            return true;
        }
        if let Some(receiver) = &pending {
            match receiver.try_recv() {
                Ok(Ok(seed)) => {
                    pending = None;
                    if credentials::unlock(pack.identity, seed) {
                        FAILURES.store(0, Ordering::Relaxed);
                        notice =
                            format!("{} unlocked. F8 opens them in game.", pack.identity.title());
                        unlocked_at = Some(get_time());
                    } else {
                        notice = "This pack can't be unlocked.".into();
                    }
                }
                Ok(Err(_)) | Err(mpsc::TryRecvError::Disconnected) => {
                    pending = None;
                    let failures = FAILURES.fetch_add(1, Ordering::Relaxed) + 1;
                    let wait = backoff(failures);
                    notice = format!("Incorrect password. Try again in {wait:.0} s.");
                    retry_at = get_time() + wait;
                    focused = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        match backdrop.as_deref_mut() {
            Some(arena) => arena.draw(&settings),
            None => clear_background(bg_color()),
        }
        let s = (screen_width() / 760.)
            .min(screen_height() / 540.)
            .clamp(0.1, 1.4);
        let r = Rect::new(
            (screen_width() - 500. * s) / 2.,
            (screen_height() - 360. * s) / 2.,
            500. * s,
            360. * s,
        );
        panel(r);
        center_label(
            match pack.identity {
                Identity::Creator => "CREATOR TOOLS",
                Identity::CentralOwner => "CENTRAL OWNER TOOLS",
            },
            r.center().x,
            r.y + 46. * s,
            25. * s,
            text_color(),
        );
        // The heading already names the tools; a renamed pack says so.
        let origin = if name.eq_ignore_ascii_case(pack.identity.title()) {
            format!("Signed by {PUBLISHER}")
        } else {
            format!("{name}  ·  signed by {PUBLISHER}")
        };
        center_label(
            &fitted_label(&origin, 13. * s, r.w - 60. * s),
            r.center().x,
            r.y + 72. * s,
            13. * s,
            muted_color(),
        );
        center_label(
            "Enter the pack's unlock password",
            r.center().x,
            r.y + 100. * s,
            16. * s,
            muted_color(),
        );
        let waiting = get_time() < retry_at;
        let available = pending.is_none() && unlocked_at.is_none() && !waiting;
        set_interactive(available);
        text_field(
            Rect::new(r.x + 36. * s, r.y + 120. * s, r.w - 72. * s, 46. * s),
            &mut password,
            &mut focused,
            256,
            s,
            true,
        );
        let submit = button(
            Rect::new(r.x + 36. * s, r.y + 186. * s, r.w - 72. * s, 42. * s),
            if pending.is_some() {
                "Unlocking…"
            } else {
                "Unlock"
            },
            true,
            available && !password.is_empty(),
            s,
        );
        set_interactive(pending.is_none() && unlocked_at.is_none());
        let skip =
            button(
                Rect::new(r.x + 36. * s, r.y + 238. * s, r.w - 72. * s, 34. * s),
                "Not now",
                false,
                pending.is_none() && unlocked_at.is_none(),
                s * 0.85,
            ) || (pending.is_none() && unlocked_at.is_none() && is_key_pressed(KeyCode::Escape));
        if skip {
            password.zeroize();
            while get_char_pressed().is_some() {}
            return true;
        }
        if available && !password.is_empty() && (submit || is_key_pressed(KeyCode::Enter)) {
            let (reply, receiver) = mpsc::channel();
            let secret = Zeroizing::new(std::mem::take(&mut *password));
            let pack = pack.clone();
            match std::thread::Builder::new()
                .name("credential-unlock".into())
                .spawn(move || {
                    let _ = reply.send(pack.open(&secret));
                }) {
                Ok(_) => {
                    pending = Some(receiver);
                    notice.clear();
                }
                Err(_) => notice = "Unlock is unavailable. Please try again.".into(),
            }
        }
        set_interactive(true);
        let notice = if waiting && pending.is_none() && notice.is_empty() {
            format!(
                "Too many attempts. Try again in {:.0} s.",
                (retry_at - get_time()).ceil()
            )
        } else {
            notice.clone()
        };
        center_label(
            &fitted_label(&notice, 13. * s, r.w - 60. * s),
            r.center().x,
            r.y + 300. * s,
            13. * s,
            if unlocked_at.is_some() { accent() } else { RED },
        );
        center_label(
            "Keep the password separate from the pack.",
            r.center().x,
            r.y + 336. * s,
            12. * s,
            muted_color(),
        );
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::backoff;

    #[test]
    fn wrong_passwords_wait_longer_each_time_up_to_a_minute() {
        assert_eq!(backoff(0), 0.);
        assert_eq!(backoff(1), 2.);
        assert_eq!(backoff(3), 8.);
        assert_eq!(backoff(6), 60.);
        assert_eq!(backoff(40), 60.);
    }
}
