use std::env;

#[macro_use]
extern crate diesel;
use dotenv::dotenv;
#[macro_use]
extern crate log;
use serenity::{framework::standard::StandardFramework, prelude::*};

pub mod discord;
pub mod games;
pub mod helpers;
pub mod schema;

use crate::{
    discord::{
        channel_groups::{get_groups, get_submission_channels},
        commands::{after_hook, before_hook, GENERAL_GROUP},
        messages::{normal_message_hook, Handler},
        servers::get_servers,
        MURAHDAHLA_INTENTS,
    },
    helpers::*,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().expect("Failed to load .env file");
    env_logger::init();

    let token = env::var("MURAHDAHLA_DISCORD_TOKEN")
        .expect("Expected MURAHDAHLA_DISCORD_TOKEN in the environment.");
    let database_url = env::var("DATABASE_URL").expect("Expected DATABASE_URL in the environment");
    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!").allow_dm(false))
        .group(&GENERAL_GROUP)
        .before(before_hook)
        .after(after_hook)
        .normal_message(normal_message_hook)
        // we probably want a better rate limiting solution but let's put a nominal limit
        // on it for now since startrace will be making requests
        .bucket("startrace", |b| b.delay(2))
        .await;

    let mut client = Client::builder(&token, MURAHDAHLA_INTENTS)
        .framework(framework)
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
