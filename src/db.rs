use std::{env, fmt};

pub use crate::schema::{games, leaderboard, posts};
pub use chrono::{offset::Utc, NaiveDate, NaiveTime};
pub use diesel::{dsl::exists, mysql::MysqlConnection, prelude::*};
use serenity::{framework::standard::CommandError, prelude::*};

pub fn establish_connection() -> MysqlConnection {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    MysqlConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url))
}

pub fn create_game_entry(db_mutex: &Mutex<MysqlConnection>, guild: u64, todays_date: &NaiveDate) {
    let conn = &*db_mutex.lock();
    let new_game = NewGame {
        game_date: *todays_date,
        guild_id: guild,
        game_active: true,
    };

    diesel::insert_into(games::table)
        .values(new_game)
        .execute(conn)
        .unwrap();
}

pub fn create_post_entry(
    db_mutex: &Mutex<MysqlConnection>,
    post: u64,
    time: NaiveTime,
    guild: u64,
    channel: u64,
) {
    use crate::schema::games::columns::*;
    let conn = &*db_mutex.lock();
    let game: u32 = games::table
        .select(game_id)
        .filter(guild_id.eq(guild))
        .filter(game_active.eq(true))
        .get_result(conn)
        .unwrap();

    let new_post = Post {
        post_id: post,
        post_time: time,
        game_id: game,
        guild_id: guild,
        guild_channel: channel,
    };

    diesel::insert_into(posts::table)
        .values(new_post)
        .execute(conn)
        .unwrap();
}

pub fn create_submission_entry(
    db_mutex: &Mutex<MysqlConnection>,
    runner: &str,
    id: u64,
    time: NaiveTime,
    collection: u8,
    forfeit: bool,
) -> Result<(), SubmissionError> {
    use crate::schema::{games::columns::*, leaderboard::columns::runner_id as runner_ids};
    let conn = &*db_mutex.lock();
    let duplicate_check = leaderboard::table
        .select(runner_ids)
        .load::<u64>(conn)?
        .iter()
        .any(|x| *x == id);

    if duplicate_check {
        return Ok(());
    }

    let game: u32 = games::table
        .select(game_id)
        .filter(game_active.eq(true))
        .get_result(conn)
        .unwrap();

    let new_submission = NewSubmission {
        runner_id: id,
        game_id: game,
        runner_name: runner,
        runner_time: time,
        runner_collection: collection,
        runner_forfeit: forfeit,
    };

    diesel::insert_into(leaderboard::table)
        .values(new_submission)
        .execute(conn)
        .unwrap();

    Ok(())
}

pub fn get_leaderboard(db_mutex: &Mutex<MysqlConnection>) -> Vec<OldSubmission> {
    let conn = &*db_mutex.lock();
    let all_submissions: Vec<OldSubmission> =
        leaderboard::table.load::<OldSubmission>(conn).unwrap();
    all_submissions
}

pub fn get_leaderboard_ids(db_mutex: &Mutex<MysqlConnection>) -> Vec<u64> {
    use crate::schema::leaderboard::columns::runner_id;
    let conn = &*db_mutex.lock();
    let leaderboard_ids: Vec<u64> = leaderboard::table
        .select(runner_id)
        .load::<u64>(conn)
        .unwrap();

    leaderboard_ids
}

pub fn get_leaderboard_posts(
    leaderboard_channel: &u64,
    db_mutex: &Mutex<MysqlConnection>,
) -> Vec<u64> {
    use crate::schema::posts::columns::{guild_channel, post_id};
    let conn = &*db_mutex.lock();
    let all_posts = posts::table
        .filter(guild_channel.eq(*leaderboard_channel))
        .select(post_id)
        .load::<u64>(conn)
        .unwrap();
    all_posts
}

pub fn get_submission_posts(
    submission_channel: &u64,
    db_mutex: &Mutex<MysqlConnection>,
) -> Vec<Post> {
    use crate::schema::posts::columns::guild_channel;
    let conn = &*db_mutex.lock();
    let all_posts = posts::table
        .filter(guild_channel.eq(*submission_channel))
        .load::<Post>(conn)
        .unwrap();
    all_posts
}

pub fn get_active_games(db_mutex: &Mutex<MysqlConnection>) -> Result<Vec<Game>, CommandError> {
    let conn = &*db_mutex.lock();
    let current_games: Vec<Game> = games::table.load::<Game>(conn)?;
    Ok(current_games)
}

pub fn clear_all_tables(db_mutex: &Mutex<MysqlConnection>) -> Result<(), CommandError> {
    let conn = &*db_mutex.lock();
    diesel::delete(posts::table).execute(conn)?;
    diesel::delete(leaderboard::table).execute(conn)?;
    diesel::delete(games::table).execute(conn)?;

    Ok(())
}

// temp fix
pub fn check_for_active_game(db_mutex: &Mutex<MysqlConnection>) -> Result<bool, CommandError> {
    use crate::schema::games::columns::game_active;
    let conn = &*db_mutex.lock();
    let active_game: bool =
        diesel::dsl::select(exists(games::table.filter(game_active.eq(true)))).get_result(conn)?;
    Ok(active_game)
}

#[derive(Debug, Insertable)]
#[table_name = "games"]
pub struct NewGame {
    pub game_date: NaiveDate,
    pub guild_id: u64,
    pub game_active: bool,
}
#[derive(Debug, Queryable)]
pub struct Game {
    pub game_id: u32,
    pub game_date: NaiveDate,
    pub guild_id: u64,
    pub game_active: bool,
}
#[derive(Debug, Insertable)]
#[table_name = "leaderboard"]
pub struct NewSubmission<'a> {
    pub runner_id: u64,
    pub game_id: u32,
    pub runner_name: &'a str,
    pub runner_time: NaiveTime,
    pub runner_collection: u8,
    pub runner_forfeit: bool,
}
#[derive(Debug, Queryable, Ord, Eq, PartialEq, PartialOrd)]
pub struct OldSubmission {
    pub runner_id: u64,
    pub game_id: u32,
    pub runner_name: String,
    pub runner_time: NaiveTime,
    pub runner_collection: u8,
    pub runner_forfeit: bool,
}
#[derive(Debug, Insertable, Queryable)]
#[table_name = "posts"]
pub struct Post {
    pub post_id: u64,
    pub post_time: NaiveTime,
    pub game_id: u32,
    pub guild_id: u64,
    pub guild_channel: u64,
}

#[derive(Debug, Clone)]
pub struct SubmissionError(pub String);

impl<T: fmt::Display> From<T> for SubmissionError {
    #[inline]
    fn from(d: T) -> Self {
        SubmissionError(d.to_string())
    }
}
