//! Bastion Anticheat: validate intentions; the authoritative simulation owns
//! movement, damage, upgrades, cooldowns and time. No ping-based accusations,
//! statistical suspicion scores, client fingerprint trust, or automatic bans.
use std::time::Duration;
use tankyard_core::PlayerInput;
use tankyard_protocol::{InputBundle, sequence_newer};

pub const NAME: &str = "Bastion Anticheat";
pub const TAGLINE: &str = "The Bastion keeps watch over fair play.";

/// Contradictions in decoded messages from a peer that has completed the
/// version handshake. Each reason is safe to include in the staff audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    ControlValues,
    InputValues,
    ConflictingFrame,
}

impl Violation {
    pub fn reason(self) -> &'static str {
        match self {
            Self::ControlValues => "Control request outside the negotiated protocol limits.",
            Self::InputValues => "Input history outside the negotiated protocol limits.",
            Self::ConflictingFrame => "Two different inputs for the same frame.",
        }
    }
}

/// Retain one unmodified wire history per player. Simulation inputs have
/// already lost information to normalization and merged button edges.
#[derive(Default, Debug)]
pub struct InputHistory {
    recent: Option<(InputBundle, Duration)>,
}

impl InputHistory {
    pub fn check(&mut self, bundle: &InputBundle, now: Duration) -> Result<(), Violation> {
        if !bundle.valid() {
            return Err(Violation::InputValues);
        }
        // A u32 sequence wraps. Only compare between packets received close
        // together, and forget the window on every new player or round.
        let recent = self.recent.as_ref().filter(|(_, at)| {
            now.checked_sub(*at)
                .is_some_and(|age| age < Duration::from_secs(2))
        });
        let frames = &bundle.inputs[..usize::from(bundle.count)];
        for (i, frame) in frames.iter().enumerate() {
            let conflicts =
                |other: &PlayerInput| other.sequence == frame.sequence && other != frame;
            if frames[..i].iter().any(conflicts)
                || recent.is_some_and(|(old, _)| {
                    old.inputs[..usize::from(old.count)].iter().any(conflicts)
                })
            {
                return Err(Violation::ConflictingFrame);
            }
        }
        // Reordered or duplicated packets do not evict or prolong the newest
        // history. Forward gaps, including across a wrap, are ordinary.
        let last = frames[frames.len() - 1].sequence;
        if recent.is_none_or(|(old, _)| {
            sequence_newer(last, old.inputs[usize::from(old.count) - 1].sequence)
        }) {
            self.recent = Some((*bundle, now));
        }
        Ok(())
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Counters {
    pub accepted_inputs: u64,
    pub stale_inputs: u64,
    pub invalid_inputs: u64,
    /// Requests the server refused before simulation. In enforce mode a
    /// joined player is disconnected for them; observe mode only discards.
    pub rejected_requests: u64,
}
#[derive(Default)]
pub struct Bastion {
    counters: Counters,
}
impl Bastion {
    pub fn counters(&self) -> Counters {
        self.counters
    }
    pub fn note_rejected_request(&mut self) {
        self.counters.rejected_requests = self.counters.rejected_requests.saturating_add(1);
    }
    /// Sequence numbers establish ordering, never elapsed simulation time. A
    /// legitimate resumed client can advance by a forward jump strictly inside
    /// the half-range; the exact midpoint stays ambiguous and is treated as
    /// stale. Delayed/reordered inputs are simply ignored; none of these
    /// outcomes bans.
    pub fn input(&mut self, mut input: PlayerInput, previous: Option<u32>) -> Option<PlayerInput> {
        if !input.valid() {
            self.counters.invalid_inputs = self.counters.invalid_inputs.saturating_add(1);
            return None;
        }
        if previous.is_some_and(|last| !tankyard_protocol::sequence_newer(input.sequence, last)) {
            self.counters.stale_inputs = self.counters.stale_inputs.saturating_add(1);
            return None;
        }
        input.movement = input.movement.clamp_length(1.);
        self.counters.accepted_inputs = self.counters.accepted_inputs.saturating_add(1);
        Some(input)
    }
}

pub struct CreatorCommand<'a> {
    pub client: u64,
    pub nonce: &'a [u8; 32],
    pub server: &'a str,
    pub round: u32,
    pub current_round: u32,
    pub sequence: u64,
    pub previous_sequence: Option<u64>,
    pub payload: &'a [u8],
    pub signature: &'a [u8],
}
pub fn creator_proof(client: u64, nonce: &[u8; 32], server: &str, signature: &[u8]) -> bool {
    tankyard_protocol::verify_creator(client, nonce, server, signature)
}
/// Host consent, creator identity, fresh session authorization, rate limits,
/// and request-before-execution auditing remain mandatory at the call site.
/// Every request independently proves possession of the pinned signing key.
pub fn creator_command(request: CreatorCommand<'_>) -> bool {
    request.round == request.current_round
        && !request.payload.is_empty()
        && request.payload.len() <= 1024
        && request
            .previous_sequence
            .is_none_or(|last| request.sequence > last)
        && tankyard_protocol::verify_creator_command(
            &tankyard_protocol::creator_command(
                request.client,
                request.nonce,
                request.server,
                request.round,
                request.sequence,
                request.payload,
            ),
            request.signature,
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_core::V2;

    #[test]
    fn repeated_history_survives_loss_wrap_reordering_and_resume() {
        let mut checks = InputHistory::default();
        let mut sent = InputBundle::default();
        let now = Duration::from_secs(3);
        let frame = |sequence| PlayerInput {
            sequence,
            fire: sequence % 2 == 0,
            ..Default::default()
        };
        for sequence in [u32::MAX - 1, u32::MAX] {
            sent.push(frame(sequence));
        }
        let delayed = sent;
        sent.push(frame(0));
        checks.check(&sent, now).unwrap();
        checks.check(&delayed, now).unwrap();
        checks.check(&sent, now).unwrap();
        for sequence in [1, 94_003, 940_000] {
            sent.push(frame(sequence));
            checks.check(&sent, now).unwrap();
        }
        // A new observation after a long pause has no live comparison window.
        let mut resumed = InputBundle::default();
        resumed.push(PlayerInput {
            sequence: 940_000,
            ..Default::default()
        });
        checks
            .check(&resumed, now + Duration::from_secs(3))
            .unwrap();
    }

    #[test]
    fn refuses_a_contradiction_without_committing_any_part_of_it() {
        let frame = PlayerInput {
            sequence: 17,
            movement: V2::new(1., 1.),
            ..Default::default()
        };
        let now = Duration::ZERO;
        let mut checks = InputHistory::default();
        let mut original = InputBundle::default();
        original.push(frame);
        checks.check(&original, now).unwrap();
        let mut changed = original;
        changed.inputs[0].fire = true;
        changed.push(PlayerInput {
            sequence: 18,
            ability: true,
            ..Default::default()
        });
        assert_eq!(
            checks.check(&changed, now),
            Err(Violation::ConflictingFrame)
        );
        original.push(PlayerInput {
            sequence: 18,
            ..Default::default()
        });
        checks.check(&original, now).unwrap();
        let mut both = changed;
        both.push(frame);
        assert_eq!(
            InputHistory::default().check(&both, now),
            Err(Violation::ConflictingFrame)
        );
        both.count = 4;
        assert_eq!(checks.check(&both, now), Err(Violation::InputValues));
    }
    #[test]
    fn lag_reordering_and_sequence_gaps_never_become_cheating_accusations() {
        let mut bastion = Bastion::default();
        let mut previous = None;
        for sequence in [
            u32::MAX - 2,
            u32::MAX - 1,
            u32::MAX,
            0,
            1,
            1,
            0,
            90_000,
            89_999,
            90_001,
        ] {
            if let Some(input) = bastion.input(
                PlayerInput {
                    sequence,
                    ..Default::default()
                },
                previous,
            ) {
                previous = Some(input.sequence);
            }
        }
        assert_eq!(previous, Some(90_001));
        assert_eq!(bastion.counters().invalid_inputs, 0);
        assert_eq!(bastion.counters().stale_inputs, 3);
        assert_eq!(bastion.counters().accepted_inputs, 7);
    }
    #[test]
    fn diagonal_motion_is_bounded_and_nonfinite_or_impossible_inputs_are_ignored() {
        let mut bastion = Bastion::default();
        let input = bastion
            .input(
                PlayerInput {
                    movement: V2::new(1., 1.),
                    ..Default::default()
                },
                None,
            )
            .unwrap();
        assert!((input.movement.length() - 1.).abs() < 0.0001);
        for invalid in [
            PlayerInput {
                movement: V2::new(f32::NAN, 0.),
                ..Default::default()
            },
            PlayerInput {
                movement: V2::new(100., 0.),
                ..Default::default()
            },
            PlayerInput {
                aim: f32::INFINITY,
                ..Default::default()
            },
            PlayerInput {
                aim_distance: 5001.,
                ..Default::default()
            },
        ] {
            assert!(bastion.input(invalid, None).is_none());
        }
        assert_eq!(bastion.counters().invalid_inputs, 4);
        assert!(bastion.input(PlayerInput::default(), None).is_some());
    }
    #[test]
    fn a_custom_client_with_its_own_real_key_cannot_impersonate_the_creator() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let impostor = Ed25519KeyPair::from_seed_unchecked(&[42; 32]).unwrap();
        assert_ne!(
            impostor.public_key().as_ref(),
            tankyard_protocol::CREATOR_PUBLIC_KEY
        );
        let nonce = [11; 32];
        let signature = impostor.sign(&tankyard_protocol::creator_challenge(
            7,
            &nonce,
            "tankyard-central",
        ));
        assert!(!creator_proof(
            7,
            &nonce,
            "tankyard-central",
            signature.as_ref()
        ));
        let payload = br#"{"action":"god","enabled":true}"#;
        let signature = impostor.sign(&tankyard_protocol::creator_command(
            7,
            &nonce,
            "tankyard-central",
            1,
            1,
            payload,
        ));
        assert!(!creator_command(CreatorCommand {
            client: 7,
            nonce: &nonce,
            server: "tankyard-central",
            round: 1,
            current_round: 1,
            sequence: 1,
            previous_sequence: None,
            payload,
            signature: signature.as_ref()
        }));
    }
}
