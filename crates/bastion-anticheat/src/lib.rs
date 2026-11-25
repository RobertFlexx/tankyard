//! Bastion Anticheat: validate intentions; the authoritative simulation owns
//! movement, damage, upgrades, cooldowns and time. No ping-based accusations,
//! statistical suspicion scores, client fingerprint trust, or automatic bans.
//!
//! A finding is a proof, never a suspicion. The game's client cannot send any
//! of these, in any version of the current protocol, whatever the network
//! does to its packets:
//!
//! - an input frame or bundle outside the limits the protocol fixes;
//! - a control request outside those limits, once the player has joined;
//! - two different inputs for the same frame;
//! - a bundle whose frames are not in order.
//!
//! Loss, delay, duplication and reordering change which packets arrive and
//! when, never what one says, and the transport authenticates every byte
//! before it gets here. Undecodable bytes are not proofs: a newer client of
//! the same protocol may send a request this build does not know.
//!
//! Each proof is counted once, however many packets repeat it, and a
//! connection is removed only after [`PROOFS_TO_REMOVE`] distinct proofs
//! inside [`PROOF_WINDOW`]. One damaged message can't end a session; a
//! modified client sends many within a second. Every offending message is
//! refused either way. State per connection is fixed in size.
#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]
use std::time::Duration;
use tankyard_core::PlayerInput;
use tankyard_protocol::{InputBundle, sequence_newer};

pub const NAME: &str = "Bastion Anticheat";
pub const TAGLINE: &str = "The Bastion keeps watch over fair play.";

/// Distinct proofs inside [`PROOF_WINDOW`] that remove a connection.
pub const PROOFS_TO_REMOVE: usize = 3;
/// How long a proof counts toward removal.
pub const PROOF_WINDOW: Duration = Duration::from_secs(60);
/// Frames are compared with others received this recently. Sequences never
/// repeat within a connection, so this only bounds what is remembered.
const FRAME_WINDOW: Duration = Duration::from_secs(2);
const REMEMBERED_FRAMES: usize = 32;
const REMEMBERED_PROOFS: usize = 8;

/// Contradictions in decoded messages from a peer that has completed the
/// version handshake. Each reason is safe to include in the staff audit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Violation {
    ControlValues,
    InputValues,
    ConflictingFrame,
    ReorderedHistory,
}

impl Violation {
    pub fn reason(self) -> &'static str {
        match self {
            Self::ControlValues => "Control request outside the negotiated protocol limits.",
            Self::InputValues => "Input history outside the negotiated protocol limits.",
            Self::ConflictingFrame => "Two different inputs for the same frame.",
            Self::ReorderedHistory => "Input history out of order within one packet.",
        }
    }
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// One contradiction and what it is about. A frame repeated in later
/// packets is the same proof; each message is its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proof {
    pub violation: Violation,
    subject: Subject,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subject {
    Frame(u32),
    Message(u64),
}

impl Proof {
    fn frame(violation: Violation, sequence: u32) -> Self {
        Self {
            violation,
            subject: Subject::Frame(sequence),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Seen {
    frame: PlayerInput,
    at: Duration,
}

/// The unmodified wire frames one player sent recently. Simulation inputs
/// have already lost information to normalization and merged button edges.
#[derive(Debug)]
pub struct InputHistory {
    frames: [Option<Seen>; REMEMBERED_FRAMES],
    /// Where the next new frame is written, over the oldest.
    next: usize,
    messages: u64,
}

impl Default for InputHistory {
    fn default() -> Self {
        Self {
            frames: [None; REMEMBERED_FRAMES],
            next: 0,
            messages: 0,
        }
    }
}

impl InputHistory {
    /// Checks one bundle against the protocol and everything recently
    /// received, and remembers it if it holds. A bundle with any proof in it
    /// is refused whole and changes nothing here.
    pub fn check(&mut self, bundle: &InputBundle, now: Duration) -> Result<(), Proof> {
        self.messages = self.messages.wrapping_add(1);
        if !(1..=3).contains(&bundle.count) {
            return Err(Proof {
                violation: Violation::InputValues,
                subject: Subject::Message(self.messages),
            });
        }
        if let Some(frame) = bundle.iter().find(|frame| !frame.valid()) {
            return Err(Proof::frame(Violation::InputValues, frame.sequence));
        }
        for (i, frame) in bundle.iter().enumerate() {
            let earlier = bundle
                .iter()
                .take(i)
                .any(|other| other.sequence == frame.sequence && other != frame);
            if earlier
                || self
                    .recent(frame.sequence, now)
                    .is_some_and(|seen| seen != frame)
            {
                return Err(Proof::frame(Violation::ConflictingFrame, frame.sequence));
            }
        }
        // The client pushes one new sequence per tick, so each frame in a
        // bundle is newer than the one before it, across a wrap too.
        for (before, frame) in bundle.iter().zip(bundle.iter().skip(1)) {
            if !sequence_newer(frame.sequence, before.sequence) {
                return Err(Proof::frame(Violation::ReorderedHistory, frame.sequence));
            }
        }
        for frame in bundle.iter() {
            if self.recent(frame.sequence, now).is_none() {
                if let Some(slot) = self.frames.get_mut(self.next) {
                    *slot = Some(Seen { frame, at: now });
                }
                self.next = (self.next + 1) % REMEMBERED_FRAMES;
            }
        }
        Ok(())
    }

    fn recent(&self, sequence: u32, now: Duration) -> Option<PlayerInput> {
        self.frames.iter().flatten().find_map(|seen| {
            (seen.frame.sequence == sequence && now.saturating_sub(seen.at) < FRAME_WINDOW)
                .then_some(seen.frame)
        })
    }
}

/// What to do about a proof against a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Already counted: refuse the message and nothing more.
    Repeated,
    /// A new proof, `count` distinct ones inside the window including it.
    Recorded { count: usize },
}

impl Verdict {
    /// Whether this proof completes the case for removing the connection.
    pub fn removes(self) -> bool {
        matches!(self, Self::Recorded { count } if count >= PROOFS_TO_REMOVE)
    }
}

/// The distinct proofs against one connection over the last minute, and
/// which kinds have been reported, for as long as it stays connected.
#[derive(Debug, Default)]
pub struct Ledger {
    proofs: [Option<(Proof, Duration)>; REMEMBERED_PROOFS],
    controls: u64,
    reported: u8,
}

impl Ledger {
    /// A proof about one control message. Each message is its own proof:
    /// the reliable channel delivers every message once.
    pub fn control_proof(&mut self) -> Proof {
        self.controls = self.controls.wrapping_add(1);
        Proof {
            violation: Violation::ControlValues,
            subject: Subject::Message(self.controls),
        }
    }

    pub fn record(&mut self, proof: Proof, now: Duration) -> Verdict {
        for slot in &mut self.proofs {
            if slot.is_some_and(|(_, at)| now.saturating_sub(at) >= PROOF_WINDOW) {
                *slot = None;
            }
        }
        if self.proofs.iter().flatten().any(|(seen, _)| *seen == proof) {
            return Verdict::Repeated;
        }
        // A free slot, else the oldest. There are more slots than proofs
        // needed for removal, so the count never falls short of it.
        if let Some(slot) = self
            .proofs
            .iter_mut()
            .min_by_key(|slot| slot.map(|(_, at)| at))
        {
            *slot = Some((proof, now));
        }
        Verdict::Recorded {
            count: self.proofs.iter().flatten().count(),
        }
    }

    /// Whether `violation` still needs its first audit line.
    pub fn unreported(&self, violation: Violation) -> bool {
        self.reported & violation.bit() == 0
    }
    pub fn mark_reported(&mut self, violation: Violation) {
        self.reported |= violation.bit();
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Counters {
    pub accepted_inputs: u64,
    pub stale_inputs: u64,
    pub invalid_inputs: u64,
    /// Requests the server refused before simulation, whether or not they
    /// counted toward removing the connection.
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
    use std::f32::consts::{PI, TAU};
    use tankyard_core::V2;

    fn frame(sequence: u32) -> PlayerInput {
        PlayerInput {
            sequence,
            fire: sequence.is_multiple_of(2),
            ..Default::default()
        }
    }

    #[test]
    fn repeated_history_survives_loss_wrap_reordering_and_resume() {
        let mut checks = InputHistory::default();
        let mut sent = InputBundle::default();
        let now = Duration::from_secs(3);
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
        let first = PlayerInput {
            sequence: 17,
            movement: V2::new(1., 1.),
            ..Default::default()
        };
        let now = Duration::ZERO;
        let mut checks = InputHistory::default();
        let mut original = InputBundle::default();
        original.push(first);
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
            Err(Proof::frame(Violation::ConflictingFrame, 17))
        );
        original.push(PlayerInput {
            sequence: 18,
            ..Default::default()
        });
        checks.check(&original, now).unwrap();
        let mut both = changed;
        both.push(first);
        assert_eq!(
            InputHistory::default().check(&both, now),
            Err(Proof::frame(Violation::ConflictingFrame, 17))
        );
        both.count = 4;
        assert_eq!(
            checks.check(&both, now).map_err(|proof| proof.violation),
            Err(Violation::InputValues)
        );
    }

    #[test]
    fn frames_out_of_order_inside_one_packet_are_a_proof() {
        let now = Duration::ZERO;
        let bundle = |sequences: &[u32]| {
            let mut bundle = InputBundle::default();
            for &sequence in sequences {
                bundle.push(frame(sequence));
            }
            bundle
        };
        for sequences in [&[5, 4][..], &[5, 5], &[7, 9, 8], &[0, u32::MAX]] {
            let last = *sequences.last().unwrap();
            assert_eq!(
                InputHistory::default().check(&bundle(sequences), now),
                Err(Proof::frame(Violation::ReorderedHistory, last)),
                "{sequences:?}"
            );
        }
        // Gaps are ordinary: a refused send still uses up its sequence.
        for sequences in [&[4, 5, 6][..], &[4, 9, 400], &[u32::MAX, 0, 1]] {
            InputHistory::default()
                .check(&bundle(sequences), now)
                .unwrap();
        }
    }

    /// A small fixed generator, so these runs repeat exactly.
    struct Dice(u64);
    impl Dice {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn whatever_the_network_does_the_real_client_never_produces_a_proof() {
        for seed in 1..=64_u64 {
            let mut dice = Dice(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
            // Start anywhere, including just before the sequence wraps.
            let mut sequence = if seed % 4 == 0 {
                u32::MAX - 40
            } else {
                dice.next() as u32
            };
            let mut sent = InputBundle::default();
            let mut queue: Vec<(Duration, InputBundle)> = Vec::new();
            let mut checks = InputHistory::default();
            let tick = Duration::from_micros(16_667);
            for step in 0..3_000_u32 {
                let now = tick * step;
                sequence = sequence.wrapping_add(1);
                // Now and then a send is refused before it joins the history.
                if dice.below(40) != 0 {
                    let input = PlayerInput {
                        sequence,
                        movement: V2::from_angle(dice.below(1000) as f32 / 1000. * TAU)
                            * (dice.below(101) as f32 / 100.),
                        aim: (dice.below(1000) as f32 / 500. - 1.) * PI,
                        aim_distance: dice.below(5000) as f32,
                        fire: dice.below(2) == 0,
                        ability: dice.below(9) == 0,
                        ..Default::default()
                    };
                    sent.push(input);
                    // Lost, duplicated, or held back up to three seconds.
                    let copies = [0, 1, 1, 1, 1, 2][dice.below(6) as usize];
                    for _ in 0..copies {
                        let delay = match dice.below(10) {
                            0 => Duration::from_millis(dice.below(3_000)),
                            _ => Duration::from_millis(dice.below(120)),
                        };
                        queue.push((now + delay, sent));
                    }
                }
                // Deliver everything due, in whatever order it was shuffled.
                let mut due: Vec<_> = Vec::new();
                queue.retain(|&(at, bundle)| {
                    if at <= now {
                        due.push(bundle);
                    }
                    at > now
                });
                for i in (1..due.len()).rev() {
                    due.swap(i, dice.below(i as u64 + 1) as usize);
                }
                for bundle in due {
                    if let Err(proof) = checks.check(&bundle, now) {
                        panic!("seed {seed}: the real client was accused: {proof:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_repeated_proof_counts_once_and_old_ones_expire() {
        let mut ledger = Ledger::default();
        let mut checks = InputHistory::default();
        let at = |seconds| Duration::from_secs(seconds);
        let mut bad = InputBundle::default();
        bad.push(PlayerInput {
            sequence: 10,
            aim_distance: 9_000.,
            ..Default::default()
        });
        let proof = checks.check(&bad, at(1)).unwrap_err();
        assert_eq!(ledger.record(proof, at(1)), Verdict::Recorded { count: 1 });
        // The same bad frame comes back in the next two histories.
        for next in [11, 12] {
            bad.push(frame(next));
            let again = checks.check(&bad, at(1)).unwrap_err();
            assert_eq!(again, proof);
            assert_eq!(ledger.record(again, at(1)), Verdict::Repeated);
        }
        let second = ledger.control_proof();
        assert_eq!(ledger.record(second, at(2)), Verdict::Recorded { count: 2 });
        // A minute later the first two no longer count toward removal.
        let third = ledger.control_proof();
        let verdict = ledger.record(third, at(62));
        assert_eq!(verdict, Verdict::Recorded { count: 1 });
        assert!(!verdict.removes());
        for n in 2..=3 {
            let proof = ledger.control_proof();
            let verdict = ledger.record(proof, at(63));
            assert_eq!(verdict, Verdict::Recorded { count: n });
            assert_eq!(verdict.removes(), n == PROOFS_TO_REMOVE);
        }
        // Reports are remembered per kind for the whole connection.
        assert!(ledger.unreported(Violation::ReorderedHistory));
        ledger.mark_reported(Violation::ReorderedHistory);
        assert!(!ledger.unreported(Violation::ReorderedHistory));
        assert!(ledger.unreported(Violation::ControlValues));
    }

    #[test]
    fn a_full_ledger_still_counts_a_flood_correctly() {
        let mut ledger = Ledger::default();
        let now = Duration::from_secs(5);
        for n in 1..=40 {
            let proof = ledger.control_proof();
            let verdict = ledger.record(proof, now + Duration::from_millis(n));
            assert_eq!(
                verdict,
                Verdict::Recorded {
                    count: (n as usize).min(REMEMBERED_PROOFS)
                }
            );
            assert!(verdict.removes() == (n as usize >= PROOFS_TO_REMOVE));
        }
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
