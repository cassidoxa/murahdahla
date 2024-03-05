#![allow(clippy::extra_unused_lifetimes)] // Diesel Insertable derive macro
use std::{env, sync::OnceLock};

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate log;

use dotenv::dotenv;
use serenity::{framework::standard::StandardFramework, prelude::*};

pub mod discord;
pub mod games;
pub mod helpers;
pub mod schema;

use crate::{
    discord::{
        channel_groups::{get_groups, get_submission_channels},
        commands::{after_hook, before_hook, GENERAL_GROUP},
        intents,
        messages::{normal_message_hook, Handler},
        servers::get_servers,
    },
    helpers::*,
};

static MAINTENANCE_USER: OnceLock<u64> = OnceLock::new();

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().expect("Failed to load .env file");
    env_logger::init();

    let token = env::var("MURAHDAHLA_DISCORD_TOKEN")
        .expect("Expected MURAHDAHLA_DISCORD_TOKEN in the environment.");
    let database_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL in the environment");
    let maintenance_user: u64 = env::var("MAINTENANCE_USER")
        .expect("Expected MAINTENANCE_USER in the environment")
        .parse::<u64>()
        .expect("Expected MAINTENANCE_USER to be parsable to 64-bit integer");
    MAINTENANCE_USER.set(maintenance_user).unwrap();
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!").allow_dm(false))
        .group(&GENERAL_GROUP)
        .before(before_hook)
        .after(after_hook)
        .normal_message(normal_message_hook);

    let mut client = Client::builder(&token, intents())
        .framework(framework)
        .cache_settings(|c| c.max_messages(50))
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        let db_pool = get_pool(&database_url)?;
        let conn = db_pool
            .get()
            .expect("Error retrieving database connection from pool");

        let submission_channel_set = get_submission_channels(&conn)?;
        let servers = get_servers(&conn)?;
        let groups = get_groups(&conn)?;

        data.insert::<DBPool>(db_pool);
        data.insert::<SubmissionSet>(submission_channel_set);
        data.insert::<ServerContainer>(servers);
        data.insert::<GroupContainer>(groups);
    }

    if let Err(e) = client.start().await {
        error!("Client error: {:?}", e);
    }

    Ok(())
}
