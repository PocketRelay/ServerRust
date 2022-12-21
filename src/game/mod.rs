use crate::{
    blaze::components::{Components, GameManager, UserSessions},
    utils::types::{GameID, GameSlot, PlayerID, SessionID},
};
use blaze_pk::{codec::Encodable, packet::Packet, types::TdfMap};
use codec::*;
use log::debug;
use player::{GamePlayer, GamePlayerSnapshot};
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{oneshot, RwLock};

use self::rules::RuleSet;

pub mod codec;
pub mod enums;
pub mod manager;
pub mod player;
pub mod rules;

pub struct Game {
    /// Unique ID for this game
    pub id: GameID,
    /// Mutable data for this game
    pub data: RwLock<GameData>,
    /// The list of players in this game
    pub players: RwLock<Vec<GamePlayer>>,
    /// The number of the next available slot
    pub next_slot: RwLock<GameSlot>,
}

#[derive(Serialize)]
pub struct GameSnapshot {
    pub id: GameID,
    pub state: GameState,
    pub setting: u16,
    pub attributes: HashMap<String, String>,
    pub players: Vec<GamePlayerSnapshot>,
}

/// Attributes map type
pub type AttrMap = TdfMap<String, String>;

/// Structure for storing the mutable portion of
/// the game data
pub struct GameData {
    /// The current game state
    pub state: GameState,
    /// The current game setting
    pub setting: u16,
    /// The game attributes
    pub attributes: AttrMap,
}

impl GameData {
    fn new(setting: u16, attributes: AttrMap) -> Self {
        Self {
            state: GameState::Init,
            setting,
            attributes,
        }
    }
}

pub enum GameModifyAction {
    /// Adds a new player to the game
    AddPlayer(GamePlayer),
    /// Modify the state of the game
    SetState(GameState),
    /// Modify the setting of the game
    SetSetting(u16),
    /// Modify the attributes of the game
    SetAttributes(AttrMap),
    /// Trigger a mesh connection update
    UpdateMeshConnection {
        session: SessionID,
        target: PlayerID,
        state: PlayerState,
    },
    /// Remove a player with a sender for responding with
    /// whether the game is empty now or not
    RemovePlayer(RemovePlayerType, oneshot::Sender<bool>),

    /// Request for checking if the game is joinable optionally with
    /// a ruleset for checking attributes against
    CheckJoinable(Option<Arc<RuleSet>>, oneshot::Sender<GameJoinableState>),

    /// Requests a snapshot of the current game state
    Snapshot(oneshot::Sender<GameSnapshot>),
}

pub enum GameJoinableState {
    /// Game is currenlty joinable
    Joinable,
    /// Game is full
    Full,
    /// The game doesn't match the provided rules
    NotMatch,
}

impl Game {
    /// Constant for the maximum number of players allowed in
    /// a game at one time. Used to determine a games full state
    const MAX_PLAYERS: usize = 4;

    /// Creates a new game with the provided details
    ///
    /// `id`         The unique game ID
    /// `attributes` The initial game attributes
    /// `setting`    The initial game setting
    pub fn new(id: GameID, attributes: AttrMap, setting: u16) -> Self {
        Self {
            id,
            data: RwLock::new(GameData::new(setting, attributes)),
            players: RwLock::new(Vec::new()),
            next_slot: RwLock::new(0),
        }
    }

    /// Modifies the game using the provided game modify value
    ///
    /// `action` The modify action
    pub async fn handle_action(&self, action: GameModifyAction) {
        match action {
            GameModifyAction::AddPlayer(player) => self.add_player(player).await,
            GameModifyAction::SetState(state) => self.set_state(state).await,
            GameModifyAction::SetSetting(setting) => self.set_setting(setting).await,
            GameModifyAction::SetAttributes(attributes) => self.set_attributes(attributes).await,
            GameModifyAction::UpdateMeshConnection {
                session,
                target,
                state,
            } => self.update_mesh_connection(session, target, state).await,
            GameModifyAction::RemovePlayer(ty, sender) => {
                let is_empty = self.remove_player(ty).await;
                sender.send(is_empty).ok();
            }
            GameModifyAction::CheckJoinable(rules, sender) => {
                let join_state = self.check_joinable(rules).await;
                sender.send(join_state).ok();
            }
            GameModifyAction::Snapshot(sender) => {
                let snapshot = self.snapshot().await;
                sender.send(snapshot).ok();
            }
        }
    }

    async fn check_joinable(&self, rules: Option<Arc<RuleSet>>) -> GameJoinableState {
        let next_slot = *self.next_slot.read().await;
        let is_joinable = next_slot < Self::MAX_PLAYERS;
        if let Some(rules) = rules {
            let data = &*self.data.read().await;
            if !rules.matches(&data.attributes) {
                return GameJoinableState::NotMatch;
            }
        }
        if is_joinable {
            GameJoinableState::Joinable
        } else {
            GameJoinableState::Full
        }
    }

    /// Takes a snapshot of the current game state for serialization
    async fn snapshot(&self) -> GameSnapshot {
        let data = &*self.data.read().await;
        let old_attributes = &data.attributes;
        let mut attributes = HashMap::with_capacity(old_attributes.len());
        for (key, value) in old_attributes.iter() {
            attributes.insert(key.to_owned(), value.to_owned());
        }

        let players = &*self.players.read().await;
        let players = players.iter().map(|value| value.snapshot()).collect();

        GameSnapshot {
            id: self.id,
            state: data.state,
            setting: data.setting,
            attributes,
            players,
        }
    }

    /// Writes the provided packet to all connected sessions.
    /// Does not wait for the write to complete just waits for
    /// it to be placed into each sessions write buffers.
    ///
    /// `packet` The packet to write
    async fn push_all(&self, packet: &Packet) {
        let players = &*self.players.read().await;
        players.iter().for_each(|value| value.push(packet.clone()));
    }

    /// Sends a notification packet to all the connected session
    /// with the provided component and contents
    ///
    /// `component` The packet component
    /// `contents`  The packet contents
    async fn notify_all<C: Encodable>(&self, component: Components, contents: C) {
        let packet = Packet::notify(component, contents);
        self.push_all(&packet).await;
    }

    /// Sets the current game state in the game data and
    /// sends an update notification to all connected clients
    /// notifying them of the changed state
    ///
    /// `state` The new state value
    async fn set_state(&self, state: GameState) {
        debug!("Updating game state (Value: {state:?})");
        {
            let data = &mut *self.data.write().await;
            data.state = state;
        }

        self.notify_all(
            Components::GameManager(GameManager::GameStateChange),
            StateChange { id: self.id, state },
        )
        .await;
    }

    /// Sets the current game setting in the game data and
    /// sends an update notification to all connected clients
    /// notifying them of the changed setting
    ///
    /// `setting` The new setting value
    async fn set_setting(&self, setting: u16) {
        debug!("Updating game setting (Value: {setting})");
        {
            let data = &mut *self.data.write().await;
            data.setting = setting;
        }

        self.notify_all(
            Components::GameManager(GameManager::GameSettingsChange),
            SettingChange {
                id: self.id,
                setting,
            },
        )
        .await;
    }

    /// Sets the current game attributes in the game data and
    /// sends an update notification to all connected clients
    /// notifying them of the changed attributes
    ///
    /// `attributes` The new attributes
    async fn set_attributes(&self, attributes: AttrMap) {
        debug!("Updating game attributes");
        let packet = Packet::notify(
            Components::GameManager(GameManager::GameAttribChange),
            AttributesChange {
                id: self.id,
                attributes: &attributes,
            },
        );
        let data = &mut *self.data.write().await;
        data.attributes.extend(attributes);
        self.push_all(&packet).await;
    }

    /// Updates all the client details for the provided session.
    /// Tells each client to send session updates to the session
    /// and the session to send them as well.
    ///
    /// `session` The session to update for
    async fn update_clients(&self, player: &GamePlayer) {
        debug!("Updating clients with new session details");
        let players = &*self.players.read().await;
        players.iter().for_each(|value| {
            value.write_updates(player);
            player.write_updates(value);
        });
    }

    /// Checks whether the provided session is a player in this game
    ///
    /// `session` The session to check for
    async fn is_player_sid(&self, sid: SessionID) -> bool {
        let players = &*self.players.read().await;
        players.iter().any(|value| value.session_id == sid)
    }

    /// Checks whether this game contains a player with the provided
    /// player ID
    ///
    /// `pid` The player ID
    async fn is_player_pid(&self, pid: PlayerID) -> bool {
        let players = &*self.players.read().await;
        players.iter().any(|value| value.player_id == pid)
    }

    async fn aquire_slot(&self) -> usize {
        let next_slot = &mut *self.next_slot.write().await;
        let slot = *next_slot;
        *next_slot += 1;
        slot
    }

    async fn release_slot(&self) {
        let next_slot = &mut *self.next_slot.write().await;
        *next_slot -= 1;
    }

    /// Adds the provided player to this game
    ///
    /// `session` The session to add
    async fn add_player(&self, mut player: GamePlayer) {
        let slot = self.aquire_slot().await;
        player.game_id = self.id;

        self.notify_player_joining(&player, slot).await;
        self.update_clients(&player).await;
        self.notify_game_setup(&player, slot).await;

        player.set_game(Some(self.id));

        let packet = player.create_set_session();
        self.push_all(&packet).await;

        {
            let players = &mut *self.players.write().await;
            players.push(player);
        }

        debug!("Adding player complete");
    }

    /// Notifies all the players in the game that a new player has
    /// joined the game.
    async fn notify_player_joining(&self, player: &GamePlayer, slot: GameSlot) {
        if slot == 0 {
            return;
        }
        let packet = Packet::notify(
            Components::GameManager(GameManager::PlayerJoining),
            PlayerJoining { slot, player },
        );
        self.push_all(&packet).await;
        player.push(packet);
    }

    /// Notifies the provided player that the game has been setup and
    /// is ready for them to attempt to join.
    ///
    /// `session` The session to notify
    /// `slot`    The slot the player is joining into
    async fn notify_game_setup(&self, player: &GamePlayer, slot: GameSlot) {
        let players = &*self.players.read().await;
        let game_data = &*self.data.read().await;

        let ty = match slot {
            0 => GameDetailsType::Created,
            _ => GameDetailsType::Joined,
        };

        let packet = Packet::notify(
            Components::GameManager(GameManager::GameSetup),
            GameDetails {
                id: self.id,
                players,
                game_data,
                player,
                ty,
            },
        );

        player.push(packet);
    }

    /// Sets the state for the provided session notifying all
    /// the players that the players state has changed.
    ///
    /// `session` The session to change the state of
    /// `state`   The new state value
    async fn set_player_state(
        &self,
        session: SessionID,
        state: PlayerState,
    ) -> Option<PlayerState> {
        let (player_id, old_state) = {
            let players = &mut *self.players.write().await;
            let player = players
                .iter_mut()
                .find(|value| value.session_id == session)?;
            let old_state = player.state;
            player.state = state;
            (player.player_id, old_state)
        };

        let packet = Packet::notify(
            Components::GameManager(GameManager::GamePlayerStateChange),
            PlayerStateChange {
                gid: self.id,
                pid: player_id,
                state,
            },
        );
        self.push_all(&packet).await;
        Some(old_state)
    }

    /// Modifies the psudo admin list this list doesn't actually exist in
    /// our implementation but we still need to tell the clients these
    /// changes.
    ///
    /// `target`    The player to target for the admin list
    /// `operation` Whether to add or remove the player from the admin list
    async fn modify_admin_list(&self, target: PlayerID, operation: AdminListOperation) {
        let host_id = {
            let players = &*self.players.read().await;
            let Some(host) = players.first() else {
                return;
            };
            host.player_id
        };
        let packet = Packet::notify(
            Components::GameManager(GameManager::AdminListChange),
            AdminListChange {
                game_id: self.id,
                player_id: target,
                operation,
                host_id,
            },
        );
        self.push_all(&packet).await;
    }

    /// Handles updating a mesh connection between two targets. If the target
    /// that the mesh was connected to was a player in the game then the
    /// joining was complete and on_join_complete is processed.
    ///
    /// `session` The session updating its mesh connection
    /// `target`  The pid of the connected target
    async fn update_mesh_connection(
        &self,
        session: SessionID,
        target: PlayerID,
        state: PlayerState,
    ) {
        debug!("Updating mesh connection");
        match state {
            PlayerState::Disconnected => {
                debug!("Disconnected mesh")
            }
            PlayerState::Connecting => {
                if self.is_player_sid(session).await && self.is_player_pid(target).await {
                    self.set_player_state(session, PlayerState::Connected).await;
                    self.on_join_complete(session).await;
                    debug!("Connected player to game")
                } else {
                    debug!("Connected player mesh")
                }
            }
            PlayerState::Connected => {}
            _ => {}
        }
    }

    /// Handles informing the other players in the game when a player joining
    /// is complete (After the mesh connection is updated) and modifies the
    /// admin list to include the newly added session
    ///
    /// `session` The session that completed joining
    async fn on_join_complete(&self, session: SessionID) {
        let players = &*self.players.read().await;
        let Some(player) = players.iter().find(|value| value.session_id == session) else {
            return;
        };
        let packet = Packet::notify(
            Components::GameManager(GameManager::PlayerJoinCompleted),
            JoinComplete {
                game_id: self.id,
                player_id: player.player_id,
            },
        );
        self.push_all(&packet).await;
        self.modify_admin_list(player.player_id, AdminListOperation::Add)
            .await;
    }

    async fn remove_player(&self, ty: RemovePlayerType) -> bool {
        let (player, slot, reason, is_empty) = {
            let players = &mut *self.players.write().await;
            if players.is_empty() {
                return true;
            }
            let (index, reason) = match ty {
                RemovePlayerType::Player(player_id, reason) => (
                    players
                        .iter()
                        .position(|value| value.player_id == player_id),
                    reason,
                ),
                RemovePlayerType::Session(session_id) => (
                    players
                        .iter()
                        .position(|value| value.session_id == session_id),
                    RemoveReason::Generic,
                ),
            };

            let (player, index) = match index {
                Some(index) => (players.remove(index), index),
                None => return false,
            };
            (player, index, reason, players.is_empty())
        };

        player.set_game(None);
        self.notify_player_removed(&player, reason).await;
        self.notify_fetch_data(&player).await;
        self.modify_admin_list(player.player_id, AdminListOperation::Remove)
            .await;

        // Possibly not needed
        // let packet = player.create_set_session();
        // self.push_all(&packet).await;
        debug!(
            "Removed player from game (PID: {}, GID: {})",
            player.player_id, self.id
        );
        // If the player was in the host slot
        if slot == 0 {
            self.try_migrate_host().await;
        }
        self.release_slot().await;

        is_empty
    }

    /// Notifies all the session and the removed session that a
    /// session was removed from the game.
    ///
    /// `player`    The player that was removed
    /// `player_id` The player ID of the removed player
    async fn notify_player_removed(&self, player: &GamePlayer, reason: RemoveReason) {
        let packet = Packet::notify(
            Components::GameManager(GameManager::PlayerRemoved),
            PlayerRemoved {
                game_id: self.id,
                player_id: player.player_id,
                reason,
            },
        );
        self.push_all(&packet).await;
        player.push(packet);
    }

    /// Notifies all the sessions in the game to fetch the player data
    /// for the provided session and the session to fetch the extended
    /// data for all the other sessions. Will early return if there
    /// are no players left.
    ///
    /// `session`   The session to update with the other clients
    /// `player_id` The player id of the session to update
    async fn notify_fetch_data(&self, player: &GamePlayer) {
        let removed_packet = Packet::notify(
            Components::UserSessions(UserSessions::FetchExtendedData),
            FetchExtendedData {
                player_id: player.player_id,
            },
        );
        self.push_all(&removed_packet).await;

        let players = &*self.players.read().await;
        for other_player in players {
            let packet = Packet::notify(
                Components::UserSessions(UserSessions::FetchExtendedData),
                FetchExtendedData {
                    player_id: other_player.player_id,
                },
            );
            player.push(packet)
        }
    }

    /// Attempts to migrate the host of this game if there are still players
    /// left in the game.
    async fn try_migrate_host(&self) {
        let players = &*self.players.read().await;
        let Some(new_host) = players.first() else { return; };

        self.set_state(GameState::HostMigration).await;
        debug!("Starting host migration (GID: {})", self.id);
        self.notify_migrate_start(new_host).await;
        self.set_state(GameState::InGame).await;
        self.notify_migrate_finish().await;
        self.update_clients(new_host).await;

        debug!("Finished host migration (GID: {})", self.id);
    }

    /// Notifies all the sessions in this game that host migration has
    /// begun.
    ///
    /// `new_host` The session that is being migrated to host
    async fn notify_migrate_start(&self, new_host: &GamePlayer) {
        let packet = Packet::notify(
            Components::GameManager(GameManager::HostMigrationStart),
            HostMigrateStart {
                game_id: self.id,
                host_id: new_host.player_id,
            },
        );
        self.push_all(&packet).await;
    }

    /// Notifies to all sessions that the migration is complete
    async fn notify_migrate_finish(&self) {
        let packet = Packet::notify(
            Components::GameManager(GameManager::HostMigrationFinished),
            HostMigrateFinished { game_id: self.id },
        );
        self.push_all(&packet).await;
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        debug!("Game has been dropped (GID: {})", self.id)
    }
}

#[derive(Debug)]
pub enum RemovePlayerType {
    Session(SessionID),
    Player(PlayerID, RemoveReason),
}
