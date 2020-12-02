#![allow(dead_code, unused_mut, unused_variables, unused_imports)]
use std::{
    collections::{HashMap, HashSet},
    env,
};

use anyhow::Result;
#[macro_use]
extern crate diesel;
use diesel::{
    mysql::MysqlConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use dotenv::dotenv;
#[macro_use]
extern crate log;
use reqwest;
use serenity::{
    framework::standard::StandardFramework,
    http::Http,
    model::id::{ChannelId, GuildId},
    prelude::*,
};
use tokio::prelude::*;

pub mod discord;
pub mod games;
pub mod helpers;
pub mod schema;

use crate::{
    discord::{
        channel_groups::{get_groups, get_submission_channels, ChannelGroup},
        commands::{after_hook, before_hook, GENERAL_GROUP},
        messages::Handler,
        servers::get_servers,
    },
    helpers::*,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().expect("Failed to load .env file");
    env_logger::init();

    let token = env::var("MURAHDAHLA_DISCORD_TOKEN")
        .expect("Expected MURAHDAHLA_DISCORD_TOKEN in the environment.");
    let database_url = env::var("MURAHDAHLA_DATABASE_URL")
        .expect("Expected MURAHDAHLA_DATABASE_URL in the environment");
    let http = Http::new_with_token(&token);
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(e) => panic!("Could not access application info: {:?}", e),
    };
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!").allow_dm(false).owners(owners))
        .group(&GENERAL_GROUP)
        .before(before_hook)
        .after(after_hook);

    let mut client = Client::builder(&token)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Error creating client");

    // get db pool, current servers, channels, permissions
    // we need:
    // hashset of current submission channels
    // permissions struct
    // groups struct
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

    // todo!();
}
