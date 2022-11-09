use crate::{env, GlobalStateArc};

use log::{error, info};
use tokio::net::TcpListener;
use tokio::select;

pub mod components;
pub mod errors;
mod routes;
pub mod session;
pub mod shared;

use session::Session;

/// Starts the Blaze server using the provided global state
/// which is cloned for the spawned sessions.
///
/// `global` The global state
pub async fn start_server(global: GlobalStateArc) {
    let listener = {
        let port = env::u16_env(env::MAIN_PORT);
        match TcpListener::bind(("0.0.0.0", port)).await {
            Ok(value) => {
                info!("Started Main Server on (Port: {port})");
                value
            }
            Err(err) => {
                error!("Failed to bind main server (Port: {}): {:?}", port, err);
                panic!();
            }
        }
    };

    let mut session_id = 1;
    let mut shutdown = global.shutdown.resubscribe();
    loop {
        select! {
            result = listener.accept() => {
                match result {
                    Ok(values) => {
                        Session::spawn(global.clone(), session_id, values);
                        session_id += 1;
                    }
                    Err(err) => {
                        error!("Error occurred while accepting connections: {:?}", err);
                    }
                }
            }
            _ = shutdown.recv() => {
                info!("Stopping main server listener from shutdown trigger.");
                break;
            }
        }
    }
}
