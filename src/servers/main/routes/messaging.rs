use crate::{
    servers::main::{models::messaging::*, session::Session},
    utils::{
        components::{Components as C, Messaging as M},
        constants, env,
    },
};
use blaze_pk::{packet::Packet, router::Router};

/// Routing function for adding all the routes in this file to the
/// provided router
///
/// `router` The router to add to
pub fn route(router: &mut Router<C, Session>) {
    router.route(C::Messaging(M::FetchMessages), handle_fetch_messages);
}

/// Handles requests from the client to fetch the server messages. The initial response contains
/// the amount of messages and then each message is sent using a SendMessage notification.
///
/// ```
/// Route: Messaging(FetchMessages)
/// ID: 24
/// Content: {
///     "FLAG": 0,
///     "MGID": 0,
///     "PIDX": 0,
///     "PSIZ": 0,
///     "SMSK": 0,
///     "SORT": 0,
///     " (0, 0, 0),
///     "STAT": 0,
///     "TARG": (0, 0, 0),
///     "TYPE": 0
/// }
/// ```
///
async fn handle_fetch_messages(session: &mut Session) -> FetchMessageResponse {
    let Some(player) = session.player.as_ref() else {
        // Not authenticated return empty count
        return  FetchMessageResponse { count: 0 };
    };
    let message = get_menu_message(session, &player.display_name);
    let notify = Packet::notify(
        C::Messaging(M::SendMessage),
        MessageNotify {
            message,
            player_id: player.id,
        },
    );

    session.push(notify);
    FetchMessageResponse { count: 1 }
}

/// Retrieves the menu message from the environment variables and replaces
/// any variables inside the message with the correct values for this session
///
/// # Variables
/// - {v} = Server Version
/// - {n} = Player Display Name
/// - {ip} = Session IP Address
fn get_menu_message(session: &Session, player_name: &str) -> String {
    let mut message = env::env(env::MENU_MESSAGE);
    if message.contains("{v}") {
        message = message.replace("{v}", constants::VERSION);
    }
    if message.contains("{n}") {
        message = message.replace("{n}", player_name);
    }
    if message.contains("{ip}") {
        message = message.replace("{ip}", &session.socket_addr.to_string());
    }
    // Line terminator for the end of the message
    message.push(char::from(0x0A));
    message
}
