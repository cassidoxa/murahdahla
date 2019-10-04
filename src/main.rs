extern crate chrono;
#[macro_use]
extern crate diesel;
extern crate dotenv;
extern crate fnv;
#[macro_use]
extern crate lazy_static;
extern crate reqwest;
extern crate serde_json;

mod db;
mod discord;
mod schema;
mod z3r;

use std::{collections::HashMap, env, error, fmt};

use dotenv::dotenv;
use serenity::{framework::standard::StandardFramework, model::id::ChannelId, prelude::*};

use discord::{ActiveGames, ChannelsContainer, DBConnectionContainer, Handler};

fn main() -> Result<(), BotError> {
    //connect to discord and database
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment.");
    let mut client = Client::new(&token, Handler).expect("Error creating client");

    {
        let mut data = client.data.write();
        let db_connection = Mutex::new(db::establish_connection());
        let active_game: bool = db::check_for_active_game(&db_connection).unwrap();
        let channels: HashMap<&'static str, ChannelId> = discord::get_channels().unwrap();
        data.insert::<DBConnectionContainer>(db_connection);
        data.insert::<ActiveGames>(active_game);
        data.insert::<ChannelsContainer>(channels);
    }

    client.with_framework(
        StandardFramework::new()
            .configure(|c| c.prefix("!"))
            .group(&discord::ADMIN_GROUP),
    );
    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct BotError;

impl fmt::Display for BotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "The bot crashed")
    }
}

impl error::Error for BotError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}
