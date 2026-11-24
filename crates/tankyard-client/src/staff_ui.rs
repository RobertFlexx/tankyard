//! The small controls both staff panels share, and how they word things.
use crate::ui::*;
use macroquad::prelude::*;

/// Common reasons, in the words the player will read.
const REASONS: [(&str, &str); 4] = [
    ("Spam", "Spamming chat."),
    ("Harassment", "Harassing other players."),
    ("Cheating", "Cheating."),
    ("Griefing", "Griefing teammates."),
];
const DURATIONS: [(&str, f32); 4] = [
    ("10 min", 10.),
    ("1 hour", 60.),
    ("1 day", 1440.),
    ("7 days", MOST_MINUTES),
];
/// The longest ban the duration controls offer. The server allows a year.
pub const MOST_MINUTES: f32 = 10_080.;
/// The longest mute a server accepts.
pub const MOST_MUTE_MINUTES: u32 = 1440;

fn chips<const N: usize>(r: Rect, s: f32) -> [Rect; N] {
    let gap = 6. * s;
    let width = (r.w - gap * (N - 1) as f32) / N as f32;
    std::array::from_fn(|i| Rect::new(r.x + i as f32 * (width + gap), r.y, width, r.h))
}

/// One click fills in a usual reason, which can still be edited.
pub fn reason_chips(r: Rect, reason: &mut String, s: f32) {
    for (rect, (title, text)) in chips::<4>(r, s).into_iter().zip(REASONS) {
        if button(rect, title, reason.as_str() == text, true, s * 0.62) {
            text.clone_into(reason);
        }
    }
}

pub fn duration_chips(r: Rect, minutes: &mut f32, s: f32) {
    for (rect, (title, value)) in chips::<4>(r, s).into_iter().zip(DURATIONS) {
        if button(rect, title, *minutes == value, true, s * 0.62) {
            *minutes = value;
        }
    }
}

/// A mute is capped at a day; a longer duration still means "as long as it can".
pub fn mute_minutes(minutes: f32) -> u32 {
    (minutes as u32).clamp(1, MOST_MUTE_MINUTES)
}

pub fn unix_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

pub use tankyard_protocol::span;

#[derive(serde::Deserialize)]
struct Record {
    time: u64,
    actor: String,
    action: String,
    target: String,
    reason: String,
    // Servers before 2.0.5 don't write names.
    #[serde(default)]
    actor_name: String,
    #[serde(default)]
    target_name: String,
}

/// The audit log as staff read it, oldest first. A creator request already
/// says what happened, so an outcome only gets its own row when it failed.
pub fn audit_rows(lines: &[String], now: u64) -> Vec<String> {
    lines
        .iter()
        .filter(|line| {
            serde_json::from_str::<Record>(line).map_or(true, |record| {
                record.action != "creator_result"
                    || record.reason.starts_with("Refused")
                    || record.reason.starts_with("Err(")
            })
        })
        .map(|line| audit_line(line, now))
        .collect()
}

/// One line of the audit log, readable. Anything that isn't a record we
/// wrote is shown as it came.
pub fn audit_line(line: &str, now: u64) -> String {
    let Ok(record) = serde_json::from_str::<Record>(line) else {
        return line.to_owned();
    };
    let ago = now.saturating_sub(record.time) / 60;
    let when = if ago == 0 {
        "just now".to_owned()
    } else {
        format!("{} ago", span(ago))
    };
    let target = if record.target_name.is_empty() {
        record.target
    } else {
        record.target_name
    };
    let what = match record.action.as_str() {
        "server_moderation" => record.reason,
        "central_owner_identity" => "Central owner verified".to_owned(),
        "central_owner_policy" => format!("Central owner access {}", record.reason),
        "creator_policy" => format!("Creator tools {}", record.reason),
        "server_role" => format!("Role set to {} for {target}", record.reason),
        "creator_request" => format!("Creator tools: {}", creator_request(&record.reason)),
        "creator_result" => record
            .reason
            .strip_prefix("Err(NetError(\"")
            .and_then(|rest| rest.strip_suffix("\"))"))
            .map_or(record.reason.clone(), |error| format!("Refused: {error}")),
        other => format!("{other}: {target} {}", record.reason),
    };
    let who = if record.actor_name.is_empty() {
        record.actor
    } else {
        record.actor_name
    };
    format!("{when} · {who} · {what}")
}

/// A creator request as a few plain words: `{"Boss":{"kind":"Railgunner"}}`
/// reads `Boss Railgunner`. Anything that isn't JSON is shown as it came.
fn creator_request(payload: &str) -> String {
    use serde_json::Value;
    fn plain(value: &Value) -> String {
        match value {
            Value::Null => String::new(),
            Value::Bool(on) => if *on { "on" } else { "off" }.to_owned(),
            Value::Number(number) => number.to_string(),
            Value::String(text) => text.clone(),
            Value::Array(items) => {
                let items: Vec<_> = items.iter().map(plain).filter(|s| !s.is_empty()).collect();
                items.join(", ")
            }
            // Enum variants are capitalized keys and read as words; a
            // struct's field names add nothing, so only their values show.
            Value::Object(fields) => {
                let parts: Vec<_> = fields
                    .iter()
                    .filter_map(|(key, inner)| {
                        let inner = plain(inner);
                        if key.starts_with(char::is_uppercase) {
                            let key = words(key);
                            Some(if inner.is_empty() {
                                key
                            } else {
                                format!("{key} {inner}")
                            })
                        } else {
                            (!inner.is_empty()).then_some(inner)
                        }
                    })
                    .collect();
                parts.join(", ")
            }
        }
    }
    match serde_json::from_str::<Value>(payload) {
        Ok(value) => match plain(&value) {
            text if text.is_empty() => payload.to_owned(),
            text => text,
        },
        Err(_) => payload.to_owned(),
    }
}

/// `SetScale` as `Set scale`.
fn words(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_read_like_a_person_wrote_them() {
        assert_eq!(span(0), "0m");
        assert_eq!(span(59), "59m");
        assert_eq!(span(125), "2h 5m");
        assert_eq!(span(1440), "1d 0h");
        assert_eq!(span(10_080 + 180), "7d 3h");
    }

    #[test]
    fn a_mute_never_asks_for_more_than_a_server_allows() {
        assert_eq!(mute_minutes(10.), 10);
        assert_eq!(mute_minutes(MOST_MINUTES), MOST_MUTE_MINUTES);
        assert_eq!(mute_minutes(0.), 1);
        let command = tankyard_protocol::ServerCommand::Mute {
            player: 1,
            minutes: mute_minutes(MOST_MINUTES),
        };
        assert!(command.valid());
    }

    #[test]
    fn audit_lines_are_readable_and_anything_odd_passes_through() {
        let line = r#"{"time":1000,"actor":"77","action":"server_moderation","target":"5","reason":"Kick { player: 5 }"}"#;
        assert_eq!(
            audit_line(line, 1000 + 300),
            "5m ago · 77 · Kick { player: 5 }"
        );
        assert_eq!(audit_line(line, 1010), "just now · 77 · Kick { player: 5 }");
        assert_eq!(audit_line("not json", 5), "not json");
    }

    #[test]
    fn audit_rows_name_people_and_hide_outcomes_that_went_fine() {
        let lines: Vec<String> = [
            r#"{"time":1000,"actor":"77","action":"server_moderation","target":"9","reason":"Banned Guest (#5) for 10m","actor_name":"Pilot","target_name":"Guest"}"#,
            r#"{"time":1000,"actor":"77","action":"creator_request","target":"4","reason":"{\"Boss\":{\"kind\":\"Railgunner\"}}","actor_name":"Pilot"}"#,
            r#"{"time":1000,"actor":"77","action":"creator_result","target":"4","reason":"Spawned Railgunner.","actor_name":"Pilot"}"#,
            r#"{"time":1000,"actor":"77","action":"creator_result","target":"4","reason":"Ok(\"Spawned Railgunner.\")"}"#,
            r#"{"time":1000,"actor":"77","action":"creator_result","target":"4","reason":"Err(NetError(\"Select another player.\"))"}"#,
            r#"{"time":1000,"actor":"host","action":"server_role","target":"77","reason":"Owner","target_name":"Pilot"}"#,
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            audit_rows(&lines, 1000),
            [
                "just now · Pilot · Banned Guest (#5) for 10m",
                "just now · Pilot · Creator tools: Boss Railgunner",
                "just now · 77 · Refused: Select another player.",
                "just now · host · Role set to Owner for Pilot",
            ]
        );
    }

    #[test]
    fn creator_requests_read_as_words_and_odd_ones_pass_through() {
        assert_eq!(
            creator_request(r#"{"Moderate":{"command":{"Kick":{"player":376,"reason":""}}}}"#),
            "Moderate Kick 376"
        );
        assert_eq!(
            creator_request(r#"{"SetScale":{"speed":1.5}}"#),
            "Set scale 1.5"
        );
        assert_eq!(creator_request(r#""ClearProjectiles""#), "ClearProjectiles");
        assert_eq!(creator_request("{}"), "{}");
        assert_eq!(creator_request("not json {"), "not json {");
    }
}
