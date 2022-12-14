use crate::{
    servers::main::{
        models::{
            auth::AuthResponse,
            errors::{ServerError, ServerResult},
            user_sessions::*,
        },
        session::Session,
    },
    state::GlobalState,
    utils::components::{Components as C, UserSessions as U},
};
use blaze_pk::{
    packet::{Request, Response},
    router::Router,
};
use database::Player;
use log::error;

/// Routing function for adding all the routes in this file to the
/// provided router
///
/// `router` The router to add to
pub fn route(router: &mut Router<C, Session>) {
    router.route(C::UserSessions(U::ResumeSession), handle_resume_session);
    router.route(C::UserSessions(U::UpdateNetworkInfo), handle_update_network);
    router.route(
        C::UserSessions(U::UpdateHardwareFlags),
        handle_update_hardware_flag,
    );
}

/// Attempts to resume an existing session for a player that has the
/// provided session token.
///
/// ```
/// Route: UserSessions(ResumeSession)
/// ID: 207
/// Content: {
///     "SKEY": "127_CHARACTER_TOKEN"
/// }
/// ```
async fn handle_resume_session(
    session: &mut Session,
    req: Request<ResumeSessionRequest>,
) -> ServerResult<Response> {
    let db = GlobalState::database();

    // Find the player that the token is for
    let player: Player = match Player::by_token(db, &req.session_token).await {
        // Valid session token
        Ok(Some(player)) => player,
        // Session that was attempted to resume is expired
        Ok(None) => return Err(ServerError::InvalidSession),
        // Error occurred while looking up token
        Err(err) => {
            error!("Error while attempt to resume session: {err:?}");
            return Err(ServerError::ServerUnavailable);
        }
    };

    let (player, session_token) = session.set_player(player).await?;

    let res = AuthResponse {
        player,
        session_token,
        silent: true,
    };

    Ok(req.response(res))
}

/// Handles updating the stored networking information for the current session
/// this is required for clients to be able to connect to each-other
///
/// ```
/// Route: UserSessions(UpdateNetworkInfo)
/// ID: 8
/// Content: {
///     "ADDR": Union("VALUE", 2, {
///         "EXIP": {
///             "IP": 0,
///             "PORT": 0
///         },
///         "INIP": {
///             "IP": 0,
///             "PORT": 0
///         }
///     }),
///     "NLMP": Map { // Map of latency to Quality of service servers
///         "ea-sjc": 156,
///         "rs-iad": 0xFFF0FFF
///         "rs-lhr": 0xFFF0FFF
///     }
///     "NQOS": {
///         "DBPS": 0,
///         "NATT": 4,
///         "UBPS": 0
///     }
/// }
/// ```
async fn handle_update_network(session: &mut Session, req: UpdateNetworkRequest) {
    session.set_network_info(req.address, req.qos);
}

/// Handles updating the stored hardware flag with the client provided hardware flag
///
/// ```
/// Route: UserSessions(UpdateHardwareFlags)
/// ID: 22
/// Content: {
///     "HWFG": 0
/// }
/// ```
async fn handle_update_hardware_flag(session: &mut Session, req: HardwareFlagRequest) {
    session.set_hardware_flag(req.hardware_flag);
}
