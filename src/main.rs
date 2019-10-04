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

use std::env;

use dotenv::dotenv;
use serenity::{framework::standard::StandardFramework, prelude::*};

use discord::{ActiveGames, DBConnectionContainer, Handler};

fn main() {
    //connect to discord and database
    dotenv().ok();
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment.");
    let mut client = Client::new(&token, Handler).expect("Error creating client");

    {
        let mut data = client.data.write();
        let db_connection = Mutex::new(db::establish_connection());
        // temp fix
        let active_game: bool = db::check_for_active_game(&db_connection).unwrap();
        data.insert::<DBConnectionContainer>(db_connection);
        data.insert::<ActiveGames>(active_game);
    }

    client.with_framework(
        StandardFramework::new()
            .configure(|c| c.prefix("!"))
            .group(&discord::ADMIN_GROUP),
    );

    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
