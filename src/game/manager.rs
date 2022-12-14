use super::{
    player::GamePlayer, rules::RuleSet, Game, GameAddr, GameJoinableState, GameModifyAction,
    GameSnapshot, RemovePlayerType,
};
use crate::utils::types::{GameID, SessionID};
use blaze_pk::types::TdfMap;
use log::debug;
use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::{AtomicU32, Ordering},
    time::SystemTime,
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinSet,
};

/// Structure for managing games and the matchmaking queue
pub struct Games {
    /// Map of Game IDs to the actual games.
    games: RwLock<HashMap<GameID, GameAddr>>,
    /// Queue of players wanting to join games
    queue: Mutex<VecDeque<QueueEntry>>,
    /// ID for the next game to create
    id: AtomicU32,
}

/// Structure for a entry in the matchmaking queue
struct QueueEntry {
    /// The session that is waiting in the queue
    player: GamePlayer,
    /// The rules that games must meet for this
    /// queue entry to join.
    rules: RuleSet,
    /// The time that the queue entry was created at
    time: SystemTime,
}

impl Default for Games {
    fn default() -> Self {
        Self {
            games: Default::default(),
            queue: Default::default(),
            id: AtomicU32::new(1),
        }
    }
}

impl Games {
    /// Takes a snapshot of all the current games for serialization. Returns the list
    /// of snapshots obtained (May not equal the count) and a boolean value indicating
    /// if there are more snapshots in the next offset (For pagination).
    ///
    /// `offset` The number of games to skip from the start of the list
    /// `count`  The number of games to obtain snapshots of
    pub async fn snapshot(&'static self, offset: usize, count: usize) -> (Vec<GameSnapshot>, bool) {
        let mut join_set = JoinSet::new();
        let (count, more) = {
            let games = &*self.games.read().await;
            // Obtained an order set of the keys from the games map
            let keys = {
                let mut keys: Vec<GameID> = games.keys().copied().collect();
                keys.sort();
                keys
            };

            // Whether there is more keys that what was requested
            let more = keys.len() > offset + count;

            // Collect the keys we will be using
            let keys: Vec<GameID> = keys.into_iter().skip(offset).take(count).collect();
            let keys_count = keys.len();

            for key in keys {
                let game = games.get(&key).cloned();
                if let Some(game) = game {
                    join_set.spawn(async move {
                        let game = game;
                        game.snapshot().await
                    });
                }
            }

            (keys_count, more)
        };

        // Start awaiting the snapshots that are being obtained
        let mut snapshots = Vec::with_capacity(count);
        while let Some(result) = join_set.join_next().await {
            if let Ok(Some(snapshot)) = result {
                snapshots.push(snapshot);
            }
        }

        (snapshots, more)
    }

    /// Takes a snapshot of the game with the provided game ID
    ///
    /// `game_id` The ID of the game to take the snapshot of
    pub async fn snapshot_id(&self, game_id: GameID) -> Option<GameSnapshot> {
        let games = &*self.games.read().await;
        let game = games.get(&game_id)?;
        game.snapshot().await
    }

    /// Creates a new game from the initial attributes and
    /// settings provided returning the Game ID of the created
    /// game. This also spawns a task to add the provided host
    /// player to the game then update the games queue
    ///
    /// `attributes` The initial game attributes
    /// `setting`    The initital game setting
    /// `host`       The host player
    pub async fn create_game(
        &'static self,
        attributes: TdfMap<String, String>,
        setting: u16,
        host: GamePlayer,
    ) -> u32 {
        let games = &mut *self.games.write().await;
        let id = self.id.fetch_add(1, Ordering::AcqRel);
        let game = Game::spawn(id, attributes, setting);
        games.insert(id, game.clone());
        game.send(GameModifyAction::AddPlayer(host));
        tokio::spawn(self.update_queue(game));
        id
    }

    /// Updates the matchmaking queue for the provided game. Will look through
    /// the queue checking if the player rules match the game attributes and if
    /// they do then add them to the game.
    ///
    /// `game` The game to update to queue with
    async fn update_queue(&self, game: GameAddr) {
        let queue = &mut *self.queue.lock().await;
        if !queue.is_empty() {
            let mut unmatched = VecDeque::new();
            while let Some(entry) = queue.pop_front() {
                let join_state = game.check_joinable(Some(entry.rules.clone())).await;
                match join_state {
                    GameJoinableState::Full => {
                        // If the game is not joinable push the entry back to the
                        // front of the queue and early return
                        queue.push_front(entry);
                        return;
                    }
                    GameJoinableState::NotMatch => {
                        // TODO: Check started time and timeout
                        // player if they've been waiting too long
                        unmatched.push_back(entry);
                    }
                    GameJoinableState::Joinable => {
                        debug!(
                            "Found player from queue adding them to the game (GID: {})",
                            game.id
                        );
                        let time = SystemTime::now();
                        let elapsed = time.duration_since(entry.time);
                        if let Ok(elapsed) = elapsed {
                            debug!("Matchmaking time elapsed: {}s", elapsed.as_secs())
                        }
                        game.send(GameModifyAction::AddPlayer(entry.player));
                    }
                }
            }
            *queue = unmatched;
        }
    }

    /// Attempts to find a game matching the rules provided by the session and
    /// add that player to the game or if there are no matching games to instead
    /// push the player to the matchmaking queue.
    ///
    /// `session` The session to get the game for
    /// `rules`   The rules the game must match to be valid
    pub fn add_or_queue(&'static self, player: GamePlayer, rules: RuleSet) {
        tokio::spawn(async move {
            let games = &*self.games.read().await;
            for (id, game) in games.iter() {
                let join_state = game.check_joinable(Some(rules.clone())).await;
                if let GameJoinableState::Joinable = join_state {
                    debug!("Found matching game (GID: {})", id);
                    game.send(GameModifyAction::AddPlayer(player));
                    return;
                }
            }

            let queue = &mut self.queue.lock().await;
            queue.push_back(QueueEntry {
                player,
                rules,
                time: SystemTime::now(),
            });
        });
    }

    /// Spawns a new task that will execute the modify action on the game
    /// with the provided `game_id` once a read lock on games has been
    /// aquired
    ///
    /// `game_id` The ID of the game to modify
    /// `action`  The action to exectue
    pub fn modify_game(&'static self, game_id: GameID, action: GameModifyAction) {
        tokio::spawn(async move {
            let games = self.games.read().await;
            if let Some(game) = games.get(&game_id) {
                game.send(action);
            }
        });
    }

    /// Removes any sessions that have the ID provided from the
    /// matchmaking queue
    ///
    /// `sid` The session ID to remove
    pub fn unqueue_session(&'static self, sid: SessionID) {
        tokio::spawn(async move {
            let queue = &mut self.queue.lock().await;
            queue.retain(|value| value.player.addr.id != sid);
        });
    }

    pub fn remove_player(&'static self, game_id: GameID, ty: RemovePlayerType) {
        tokio::spawn(async move {
            let games = self.games.read().await;
            if let Some(game) = games.get(&game_id) {
                let is_empty = game.remove_player(ty).await;
                if is_empty {
                    drop(games);

                    // Remove the empty game
                    let games = &mut *self.games.write().await;
                    games.remove(&game_id);
                }
            }
        });
    }
}
