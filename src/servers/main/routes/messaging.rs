use crate::blaze::components::{Components, Messaging};
use crate::servers::main::routes::HandleResult;
use crate::servers::main::{
    models::messaging::{FetchMessageResponse, MessageNotify},
    session::Session,
};
use crate::{constants::VERSION, env};
use blaze_pk::packet::Packet;

/// Routing function for handling packets with the `Stats` component and routing them
/// to the correct routing function. If no routing function is found then the packet
/// is printed to the output and an empty response is sent.
///
/// `session`   The session that the packet was recieved by
/// `component` The component of the packet recieved
/// `packet`    The recieved packet
pub async fn route(session: &mut Session, component: Messaging, packet: &Packet) -> HandleResult {
    match component {
        Messaging::FetchMessages => handle_fetch_messages(session, packet),
        _ => Ok(packet.respond_empty()),
    }
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
///     "SRCE": (0, 0, 0),
///     "STAT": 0,
///     "TARG": (0, 0, 0),
///     "TYPE": 0
/// }
/// ```
///
fn handle_fetch_messages(session: &mut Session, packet: &Packet) -> HandleResult {
    let Some(player) = session.player.as_ref() else {
        // Not authenticated return empty count
        let response = FetchMessageResponse { count: 0 };
        return Ok(packet.respond(response));
    };
    let message = get_menu_message(session, &player.display_name);
    let notify = Packet::notify(
        Components::Messaging(Messaging::SendMessage),
        MessageNotify {
            message,
            player_id: player.id,
        },
    );

    session.push(notify);
    let response = FetchMessageResponse { count: 1 };
    Ok(packet.respond(response))
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
        message = message.replace("{v}", VERSION);
    }
    if message.contains("{n}") {
        message = message.replace("{n}", player_name);
    }
    if message.contains("{ip}") {
        message = message.replace("{ip}", &session.addr.to_string());
    }
    // Line terminator for the end of the message
    message.push(char::from(0x0A));
    message
}