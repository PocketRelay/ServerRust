use super::{middleware::token::TokenAuth, stores::token::TokenStore};
use crate::env;
use actix_web::{
    web::{Data, ServiceConfig},
    Scope,
};
use std::sync::Arc;

mod games;
mod gaw;
mod leaderboard;
mod players;
mod public;
mod qos;
mod server;
mod token;

/// Function for configuring the provided service config with all the
/// application routes.
///
/// `cfg`         Service config to configure
/// `token_store` The token store for token authentication
pub fn configure(cfg: &mut ServiceConfig, token_store: Arc<TokenStore>) {
    server::configure(cfg);
    public::configure(cfg);
    gaw::configure(cfg);
    qos::configure(cfg);

    // If the API is enabled
    if env::from_env(env::API) {
        cfg.app_data(Data::from(token_store.clone()));
        token::configure(cfg);
        leaderboard::configure(cfg);

        // Auth protected routes
        let middleware = TokenAuth::new(token_store);
        cfg.service(
            Scope::new("")
                .wrap(middleware)
                .configure(games::configure)
                .configure(players::configure),
        );
    }
}