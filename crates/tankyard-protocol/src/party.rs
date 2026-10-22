use serde::{Deserialize, Serialize};
use tankyard_core::EntityId;

/// Membership is issued by the invite-authenticated server. Client IDs remain
/// stable across rounds; entity IDs and tank classes may change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartyMember {
    pub client: u64,
    pub player: EntityId,
    pub name: String,
    pub class: String,
    pub leader: bool,
    pub in_menu: bool,
}

pub fn valid_party_roster(members: &[PartyMember]) -> bool {
    !members.is_empty()
        && members.len() <= 8
        && members.iter().all(|member| {
            member.player > 0
                && !member.name.is_empty()
                && member.name.len() <= 96
                && !member.name.chars().any(char::is_control)
                && tankyard_mods::identifier(&member.class)
        })
        && members.iter().filter(|member| member.leader).count() <= 1
        && members.iter().enumerate().all(|(index, member)| {
            members[..index].iter().all(|previous| {
                previous.client != member.client && previous.player != member.player
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roster_rejects_duplicates_and_unbounded_or_forged_identity_fields() {
        let member = PartyMember {
            client: 0,
            player: 1,
            name: "Pilot".into(),
            class: "vector".into(),
            leader: true,
            in_menu: false,
        };
        assert!(valid_party_roster(std::slice::from_ref(&member)));
        assert!(!valid_party_roster(&[member.clone(), member.clone()]));
        assert!(!valid_party_roster(&[]));
        let mut bad = member.clone();
        bad.name = "a\nb".into();
        assert!(!valid_party_roster(&[bad]));
        let mut bad = member;
        bad.class = "../vector".into();
        assert!(!valid_party_roster(&[bad]));
    }

    #[test]
    fn party_configuration_is_bounded_before_dispatch() {
        use crate::{MatchSettings, PartyMatchOptions, ServerCommand};
        let mut settings = MatchSettings {
            mode: tankyard_core::Mode::Sandbox,
            map: "open".into(),
            difficulty: "Normal".into(),
            bots: 4,
            party: Some(PartyMatchOptions {
                width: 3600,
                height: 2600,
                dynamic_arena: true,
                starting_level: 45,
            }),
        };
        assert!(
            ServerCommand::ConfigureParty {
                settings: settings.clone()
            }
            .valid()
        );
        settings.party.as_mut().unwrap().starting_level = 255;
        assert!(!settings.valid());
        settings.party.as_mut().unwrap().starting_level = 1;
        settings.party.as_mut().unwrap().width = 1;
        assert!(!settings.valid());
        settings.party.as_mut().unwrap().width = 3600;
        settings.bots = 9;
        assert!(!settings.valid());
        settings.bots = 4;
        settings.party = None;
        assert!(!ServerCommand::ConfigureParty { settings }.valid());
    }
}
