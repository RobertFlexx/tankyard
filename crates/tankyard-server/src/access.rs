//! Server authority is local. Global authority comes only from an installed
//! account-service gateway and is never granted by a gameplay packet or mod.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};
use tankyard_protocol::{
    ServerCommand, ServerKind, ServerPermission as Permission, ServerRole as Role,
};

#[derive(Clone, Debug)]
pub struct VerifiedAccount {
    pub id: String,
    pub expires_at: u64,
}
pub struct AccountContext<'a> {
    pub server_id: &'a str,
    pub client_id: u64,
    pub now: u64,
    pub secure_transport: bool,
}
/// Implement using locally cached, cryptographically verified service claims.
/// Verification on the game thread must not perform blocking network requests.
pub trait AccountVerifier: Send + Sync {
    fn verify(
        &self,
        credential: &str,
        context: &AccountContext<'_>,
    ) -> Result<VerifiedAccount, String>;
    /// Consult cached revocation state here. Called once a second per session.
    fn is_current(&self, account: &VerifiedAccount, context: &AccountContext<'_>) -> bool {
        account.expires_at > context.now
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalRole {
    Moderator,
    Admin,
}
pub struct GlobalStaff {
    pub account: String,
    pub role: GlobalRole,
    pub expires_at: u64,
}
#[derive(Clone, Debug)]
pub enum GlobalAction {
    SuspendAccount {
        account: String,
        seconds: u64,
        reason: String,
    },
    KickOfficialPlayer {
        client: u64,
        reason: String,
    },
    DelistCommunityServer {
        server: String,
        reason: String,
    },
}
/// Separate from community roles and deliberately absent from the wire API.
pub trait GlobalAuthority: Send + Sync {
    fn verify(&self, credential: &str, now: u64) -> Result<GlobalStaff, String>;
    fn enforce(&self, staff: &GlobalStaff, action: &GlobalAction) -> Result<(), String>;
}
pub fn authorize_global(
    kind: ServerKind,
    gateway: Option<&dyn GlobalAuthority>,
    credential: &str,
    action: &GlobalAction,
    now: u64,
) -> Result<GlobalStaff, String> {
    if kind != ServerKind::Official {
        return Err("global moderation is restricted to the official backend".into());
    }
    let gateway = gateway.ok_or("global moderation gateway is not configured")?;
    if credential.is_empty() || credential.len() > 4096 {
        return Err("invalid staff credential".into());
    }
    let staff = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        gateway.verify(credential, now)
    }))
    .map_err(|_| "global verification failed")??;
    if !account_id(&staff.account) || staff.expires_at <= now {
        return Err("staff authorization expired or invalid".into());
    }
    let valid = match action {
        GlobalAction::SuspendAccount {
            account,
            seconds,
            reason,
        } => {
            account_id(account)
                && *seconds > 0
                && *seconds <= 31_536_000
                && text(reason, 160)
                && (staff.role == GlobalRole::Admin || *seconds <= 86400)
        }
        GlobalAction::KickOfficialPlayer { client, reason } => *client > 0 && text(reason, 160),
        GlobalAction::DelistCommunityServer { server, reason } => {
            staff.role == GlobalRole::Admin && account_id(server) && text(reason, 160)
        }
    };
    if !valid {
        return Err("global action is outside the staff permission scope".into());
    }
    Ok(staff)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub role: Role,
    pub permissions: Vec<Permission>,
}
impl Grant {
    pub fn for_role(role: Role) -> Self {
        let permissions = match role {
            Role::Player => vec![],
            Role::Moderator => vec![Permission::Kick, Permission::Mute, Permission::ViewAudit],
            Role::Admin => vec![
                Permission::Kick,
                Permission::Ban,
                Permission::Mute,
                Permission::ManageRooms,
                Permission::Configure,
                Permission::ViewAudit,
            ],
            Role::Owner => Permission::ALL.to_vec(),
        };
        Self { role, permissions }
    }
    fn validate(&self) -> bool {
        self.permissions.len() <= 8
            && self.permissions.iter().collect::<BTreeSet<_>>().len() == self.permissions.len()
            && (self.role != Role::Player || self.permissions.is_empty())
    }
}
struct Member {
    grant: Grant,
    account: Option<VerifiedAccount>,
    muted_until: u64,
    admin_tokens: (f32, u64),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedAccess {
    schema: u32,
    server_id: String,
    grants: HashMap<String, Grant>,
    #[serde(default)]
    sanctions: HashMap<String, Sanction>,
}
#[derive(Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Sanction {
    muted_until: u64,
    banned_until: u64,
}
pub struct Access {
    server_id: String,
    members: HashMap<u64, Member>,
    saved: HashMap<String, Grant>,
    sanctions: HashMap<String, Sanction>,
    path: String,
}
impl Access {
    pub fn new(server_id: String, path: String) -> Result<Self, String> {
        let mut saved = HashMap::new();
        let mut sanctions = HashMap::new();
        if !path.is_empty() && Path::new(&path).exists() {
            let bytes = tankyard_mods::read_bounded(Path::new(&path))?;
            let file: SavedAccess = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if file.schema != 1
                || file.server_id != server_id
                || file.grants.len() > 256
                || file.sanctions.len() > 1024
                || file.sanctions.keys().any(|id| !account_id(id))
                || file
                    .grants
                    .iter()
                    .any(|(id, g)| !account_id(id) || !g.validate())
            {
                return Err("invalid or differently scoped server access file".into());
            }
            saved = file.grants;
            sanctions = file.sanctions;
        }
        Ok(Self {
            server_id,
            members: HashMap::new(),
            saved,
            sanctions,
            path,
        })
    }
    pub fn add(
        &mut self,
        client: u64,
        account: Option<VerifiedAccount>,
        now: u64,
    ) -> Result<(), String> {
        if self.members.len() >= 256 {
            return Err("server member limit reached".into());
        }
        if account
            .as_ref()
            .is_some_and(|a| !account_id(&a.id) || a.expires_at <= now)
        {
            return Err("invalid or expired account verification".into());
        }
        if account.as_ref().is_some_and(|a| {
            self.members
                .values()
                .any(|m| m.account.as_ref().is_some_and(|b| a.id == b.id))
        }) {
            return Err("account is already connected".into());
        }
        if account.as_ref().is_some_and(|a| {
            self.sanctions
                .get(&a.id)
                .is_some_and(|s| s.banned_until > now)
        }) {
            return Err("Account is banned on this server.".into());
        }
        let muted_until = account
            .as_ref()
            .and_then(|a| self.sanctions.get(&a.id))
            .map_or(0, |s| s.muted_until);
        let grant = account
            .as_ref()
            .and_then(|a| self.saved.get(&a.id))
            .cloned()
            .unwrap_or_else(|| Grant::for_role(Role::Player));
        self.members.insert(
            client,
            Member {
                grant,
                account,
                muted_until,
                admin_tokens: (6.0, now),
            },
        );
        Ok(())
    }
    pub fn remove(&mut self, client: u64) {
        self.members.remove(&client);
    }
    pub fn clients_for_account(&self, account: &str) -> Vec<u64> {
        self.members
            .iter()
            .filter_map(|(id, m)| {
                m.account
                    .as_ref()
                    .is_some_and(|a| a.id == account)
                    .then_some(*id)
            })
            .collect()
    }
    pub fn accounts(&self) -> Vec<(u64, VerifiedAccount)> {
        self.members
            .iter()
            .filter_map(|(client, m)| m.account.clone().map(|a| (*client, a)))
            .collect()
    }
    pub fn expire(&mut self, now: u64) -> Vec<u64> {
        let mut changed = vec![];
        for (id, member) in &mut self.members {
            if member.account.as_ref().is_some_and(|a| a.expires_at <= now)
                && member.grant.role != Role::Player
            {
                member.grant = Grant::for_role(Role::Player);
                changed.push(*id);
            }
        }
        changed
    }
    pub fn grant(&self, client: u64, now: u64) -> Grant {
        self.members
            .get(&client)
            .filter(|m| m.account.as_ref().is_none_or(|a| a.expires_at > now))
            .map(|m| m.grant.clone())
            .unwrap_or_else(|| Grant::for_role(Role::Player))
    }
    pub fn permits(&self, client: u64, permission: Permission, now: u64) -> bool {
        self.grant(client, now).permissions.contains(&permission)
    }
    pub fn authorize(&self, client: u64, command: &ServerCommand, now: u64) -> Result<(), String> {
        if !command.valid() || !self.permits(client, command.permission(), now) {
            return Err("Permission denied.".into());
        }
        Ok(())
    }
    pub fn take_admin_token(&mut self, client: u64, now: u64) -> bool {
        let Some(member) = self.members.get_mut(&client) else {
            return false;
        };
        let (tokens, last) = &mut member.admin_tokens;
        *tokens = (*tokens + now.saturating_sub(*last).min(30) as f32 * 2.0).min(6.0);
        *last = now;
        if *tokens < 1.0 {
            return false;
        }
        *tokens -= 1.0;
        true
    }
    pub fn can_affect(&self, actor: u64, target: u64, now: u64) -> bool {
        self.members.contains_key(&target)
            && self.grant(actor, now).role > self.grant(target, now).role
    }
    /// This entry point is called by the process console or local host channel.
    pub fn local_grant(&mut self, target: u64, grant: Grant) -> Result<(), String> {
        if !grant.validate() {
            return Err("invalid server permission profile".into());
        }
        let member = self.members.get(&target).ok_or("player is not connected")?;
        let account = member.account.as_ref().map(|a| a.id.clone());
        let old_saved = account.as_ref().and_then(|id| self.saved.get(id)).cloned();
        if let Some(id) = &account {
            if self.saved.len() >= 256 && !self.saved.contains_key(id) {
                return Err("saved role limit reached".into());
            }
            self.saved.insert(id.clone(), grant.clone());
            if let Err(error) = self.save() {
                if let Some(old) = old_saved {
                    self.saved.insert(id.clone(), old);
                } else {
                    self.saved.remove(id);
                }
                return Err(error);
            }
        }
        let member = self.members.get_mut(&target).unwrap();
        member.grant = grant;
        Ok(())
    }
    pub fn nominate(
        &mut self,
        actor: u64,
        target: u64,
        role: Role,
        permissions: &[Permission],
        now: u64,
    ) -> Result<(), String> {
        if !self.permits(actor, Permission::ManageRoles, now)
            || role == Role::Owner
            || !self.can_affect(actor, target, now)
            || role >= self.grant(actor, now).role
        {
            return Err("Permission denied.".into());
        }
        let grant = Grant {
            role,
            permissions: permissions.to_vec(),
        };
        let actor_grant = self.grant(actor, now);
        if !grant.validate()
            || grant
                .permissions
                .iter()
                .any(|p| !actor_grant.permissions.contains(p))
        {
            return Err("Cannot grant permissions you do not have.".into());
        }
        self.local_grant(target, grant)
    }
    pub fn mute(&mut self, client: u64, until: u64) -> Result<(), String> {
        self.sanction(client, until, false)?;
        if let Some(member) = self.members.get_mut(&client) {
            member.muted_until = until;
        }
        Ok(())
    }
    pub fn ban_account(&mut self, client: u64, until: u64) -> Result<(), String> {
        self.sanction(client, until, true)
    }
    fn sanction(&mut self, client: u64, until: u64, ban: bool) -> Result<(), String> {
        let member = self.members.get(&client).ok_or("player is not connected")?;
        let Some(id) = member.account.as_ref().map(|a| a.id.clone()) else {
            return Ok(());
        };
        let now = crate::unix_time().as_secs();
        self.sanctions
            .retain(|_, s| s.banned_until > now || s.muted_until > now);
        if self.sanctions.len() >= 1024 && !self.sanctions.contains_key(&id) {
            return Err("server sanction limit reached".into());
        }
        let old = self.sanctions.get(&id).copied();
        let entry = self.sanctions.entry(id.clone()).or_default();
        if ban {
            entry.banned_until = until;
        } else {
            entry.muted_until = until;
        }
        if let Err(error) = self.save() {
            if let Some(old) = old {
                self.sanctions.insert(id, old);
            } else {
                self.sanctions.remove(&id);
            }
            return Err(error);
        }
        Ok(())
    }
    pub fn muted(&self, client: u64, now: u64) -> bool {
        self.members
            .get(&client)
            .is_some_and(|m| m.muted_until > now)
    }
    pub fn account_valid(&self, client: u64, now: u64) -> bool {
        self.members
            .get(&client)
            .is_some_and(|m| m.account.as_ref().is_some_and(|a| a.expires_at > now))
    }
    fn save(&self) -> Result<(), String> {
        if self.path.is_empty() {
            return Ok(());
        }
        let path = Path::new(&self.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let file = SavedAccess {
            schema: 1,
            server_id: self.server_id.clone(),
            grants: self.saved.clone(),
            sanctions: self.sanctions.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
        crate::storage::atomic_write(path, &bytes).map_err(|e| e.to_string())
    }
}
pub fn account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}

#[derive(Serialize)]
pub struct AuditRecord<'a> {
    pub time: u64,
    pub actor: &'a str,
    pub action: &'a str,
    pub target: &'a str,
    pub reason: &'a str,
}
pub fn audit(path: &str, record: &AuditRecord<'_>) -> Result<(), String> {
    if path.is_empty() {
        return Ok(());
    }
    use std::io::Write;
    let path = Path::new(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > 8 * 1024 * 1024) {
        std::fs::rename(path, path.with_extension("jsonl.1")).map_err(|e| e.to_string())?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.write_all(b"\n"))
        .map_err(|e| e.to_string())
}
pub fn read_audit(path: &str) -> Result<Vec<String>, String> {
    if path.is_empty() || !Path::new(path).exists() {
        return Ok(vec![]);
    }
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let start = len.saturating_sub(48 * 1024);
    file.seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(48 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let raw: Vec<_> = text.lines().collect();
    let mut lines: Vec<_> = raw
        .iter()
        .skip(usize::from(start > 0))
        .rev()
        .take(32)
        .map(|line| line.chars().filter(|c| !c.is_control()).take(240).collect())
        .collect();
    lines.reverse();
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn match_configuration_and_restart_require_the_configure_permission() {
        let mut access = Access::new("test".into(), String::new()).unwrap();
        access.add(1, None, 0).unwrap();
        let configure = ServerCommand::ConfigureMatch {
            mode: tankyard_core::Mode::Domination,
            map: "open".into(),
            difficulty: "Advanced".into(),
        };
        assert!(access.authorize(1, &configure, 0).is_err());
        access
            .local_grant(1, Grant::for_role(Role::Moderator))
            .unwrap();
        assert!(
            access
                .authorize(1, &ServerCommand::RestartRound, 0)
                .is_err()
        );
        access
            .local_grant(
                1,
                Grant {
                    role: Role::Moderator,
                    permissions: vec![Permission::Configure],
                },
            )
            .unwrap();
        assert!(access.authorize(1, &configure, 0).is_ok());
        assert!(access.authorize(1, &ServerCommand::RestartRound, 0).is_ok());
        assert!(access.authorize(1, &ServerCommand::ReloadMods, 0).is_err());
        let invalid = ServerCommand::ConfigureMatch {
            mode: tankyard_core::Mode::Ffa,
            map: "../bad".into(),
            difficulty: "Expert".into(),
        };
        assert!(access.authorize(1, &invalid, 0).is_err());
        let invalid = ServerCommand::ConfigureMatch {
            mode: tankyard_core::Mode::Ffa,
            map: "open".into(),
            difficulty: "impossible".into(),
        };
        assert!(access.authorize(1, &invalid, 0).is_err());
    }
    #[test]
    fn verified_sanctions_and_roles_survive_reconnect_and_are_scoped_to_one_server() {
        let now = crate::unix_time().as_secs();
        let root = std::env::temp_dir().join(format!(
            "tankyard-access-test-{}",
            u64::from_le_bytes(renet_netcode::generate_random_bytes())
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("access.json").to_string_lossy().into_owned();
        let claim = || {
            Some(VerifiedAccount {
                id: "account_1".into(),
                expires_at: now + 3600,
            })
        };
        let mut a = Access::new("one".into(), path.clone()).unwrap();
        a.add(1, claim(), now).unwrap();
        a.local_grant(1, Grant::for_role(Role::Moderator)).unwrap();
        a.mute(1, now + 600).unwrap();
        a.remove(1);
        let mut b = Access::new("one".into(), path.clone()).unwrap();
        b.add(2, claim(), now).unwrap();
        assert!(b.muted(2, now));
        assert!(b.permits(2, Permission::Kick, now));
        assert!(Access::new("two".into(), path).is_err());
        b.ban_account(2, now + 600).unwrap();
        b.remove(2);
        assert!(b.add(3, claim(), now).is_err());
        assert!(b.add(3, None, now).is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn a_delegate_cannot_grant_a_permission_they_lack_or_moderate_a_peer() {
        let mut a = Access::new("one".into(), String::new()).unwrap();
        for id in 1..=3 {
            a.add(id, None, 0).unwrap();
        }
        a.local_grant(
            1,
            Grant {
                role: Role::Admin,
                permissions: vec![Permission::ManageRoles, Permission::Kick],
            },
        )
        .unwrap();
        assert!(
            a.nominate(1, 2, Role::Moderator, &[Permission::Ban], 0)
                .is_err()
        );
        a.nominate(1, 2, Role::Moderator, &[], 0).unwrap();
        assert!(!a.permits(2, Permission::Kick, 0));
        a.local_grant(3, Grant::for_role(Role::Admin)).unwrap();
        assert!(!a.can_affect(1, 3, 0));
    }
    #[test]
    fn roles_deny_by_default_and_delegation_cannot_escalate() {
        let mut a = Access::new("one".into(), String::new()).unwrap();
        for id in 1..=3 {
            a.add(id, None, 0).unwrap();
        }
        assert!(!a.permits(1, Permission::Kick, 0));
        a.local_grant(1, Grant::for_role(Role::Owner)).unwrap();
        a.nominate(
            1,
            2,
            Role::Admin,
            &Grant::for_role(Role::Admin).permissions,
            0,
        )
        .unwrap();
        assert!(a.nominate(2, 3, Role::Owner, &[], 0).is_err());
        assert!(a.nominate(2, 1, Role::Player, &[], 0).is_err());
        assert!(!a.can_affect(2, 1, 0));
        assert!(
            a.nominate(1, 3, Role::Moderator, &[Permission::Kick], 0)
                .is_ok()
        );
        assert!(!a.permits(3, Permission::Ban, 0));
    }
    #[test]
    fn reconnects_do_not_inherit_session_roles_and_expired_claims_lose_permissions() {
        let mut a = Access::new("one".into(), String::new()).unwrap();
        a.add(1, None, 0).unwrap();
        a.local_grant(1, Grant::for_role(Role::Admin)).unwrap();
        a.remove(1);
        a.add(1, None, 0).unwrap();
        assert_eq!(a.grant(1, 0).role, Role::Player);
        a.remove(1);
        a.add(
            1,
            Some(VerifiedAccount {
                id: "account_1".into(),
                expires_at: 20,
            }),
            1,
        )
        .unwrap();
        a.local_grant(1, Grant::for_role(Role::Admin)).unwrap();
        assert!(a.permits(1, Permission::Ban, 19));
        assert!(!a.permits(1, Permission::Ban, 20));
    }
    struct Gateway;
    impl GlobalAuthority for Gateway {
        fn verify(&self, credential: &str, _now: u64) -> Result<GlobalStaff, String> {
            if credential != "verified" {
                return Err("denied".into());
            }
            Ok(GlobalStaff {
                account: "staff_1".into(),
                role: GlobalRole::Moderator,
                expires_at: 100,
            })
        }
        fn enforce(&self, _staff: &GlobalStaff, _action: &GlobalAction) -> Result<(), String> {
            Ok(())
        }
    }
    #[test]
    fn global_authority_is_separate_from_community_roles_and_is_disabled_without_a_gateway() {
        let action = GlobalAction::SuspendAccount {
            account: "player_1".into(),
            seconds: 3600,
            reason: "Test".into(),
        };
        assert!(
            authorize_global(
                ServerKind::Community,
                Some(&Gateway),
                "verified",
                &action,
                1
            )
            .is_err()
        );
        assert!(authorize_global(ServerKind::Official, None, "verified", &action, 1).is_err());
        assert!(
            authorize_global(ServerKind::Official, Some(&Gateway), "forged", &action, 1).is_err()
        );
        assert!(
            authorize_global(ServerKind::Official, Some(&Gateway), "verified", &action, 1).is_ok()
        );
        assert!(
            authorize_global(
                ServerKind::Official,
                Some(&Gateway),
                "verified",
                &action,
                100
            )
            .is_err()
        );
    }
}
