pub use crate::schema::{games, leaderboard, posts};
pub use chrono::{offset::Utc, NaiveDate, NaiveDateTime, NaiveTime};
pub use diesel::{
    dsl::exists,
    mysql::MysqlConnection,
    prelude::*,
    r2d2::{ConnectionManager, Pool},
    result::Error,
};
use serenity::prelude::TypeMapKey;

type MysqlPool = Pool<ConnectionManager<MysqlConnection>>;
type PooledConn = PoolConnection<MysqlConnectionManager>;

pub struct DBConnectionContainer;

impl TypeMapKey for DBConnectionContainer {
    type Value = MysqlPool;
}

#[inline]
pub fn get_pool(database_url: &str) -> Result<MysqlPool, Error> {
    let manager = ConnectionManager::<MysqlConnection>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");

    Ok(pool)
}

#[inline]
pub fn create_game_entry(db_pool: &MysqlPool, guild: u64, todays_date: &NaiveDate) {
    let conn = db_pool
        .get()
        .expect("Error getting connection from db pool while inserting new game");
    let new_game = NewGame {
        game_date: *todays_date,
        guild_id: guild,
        game_active: true,
    };

    diesel::insert_into(games::table)
        .values(new_game)
        .execute(&conn)
        .expect("Error creating new game in database");
}

#[inline]
pub fn create_post_entry(
    db_pool: &MysqlPool,
    post: u64,
    time: NaiveDateTime,
    guild: u64,
    channel: u64,
) -> Result<(), Error> {
    use crate::schema::games::columns::*;
    let conn = db_pool
        .get()
        .expect("Error getting connection from db pool while inserting post");
    let game: u32 = games::table
        .select(game_id)
        .filter(guild_id.eq(guild))
        .filter(game_active.eq(true))
        .get_result(&conn)?;

    let new_post = Post {
        post_id: post,
        post_datetime: time,
        game_id: game,
        guild_id: guild,
        guild_channel: channel,
    };

    diesel::insert_into(posts::table)
        .values(new_post)
        .execute(&conn)?;

    Ok(())
}

#[inline]
pub fn create_submission_entry(
    db_pool: &MysqlPool,
    runner: &str,
    id: u64,
    time: NaiveTime,
    collection: u16,
    forfeit: bool,
) -> Result<(), Error> {
    use crate::schema::{games::columns::*, leaderboard::columns::runner_id as runner_ids};

    let current_time: NaiveDateTime = Utc::now().naive_utc();
    let conn = db_pool
        .get()
        .expect("Error getting connection from db pool while inserting submission");
    let duplicate_check = leaderboard::table
        .select(runner_ids)
        .load::<u64>(&conn)?
        .iter()
        .any(|x| *x == id);

    if duplicate_check {
        return Ok(());
    }

    let game: u32 = games::table
        .select(game_id)
        .filter(game_active.eq(true))
        .get_result(&conn)?;

    let new_submission = NewSubmission {
        runner_id: id,
        game_id: game,
        runner_name: runner,
        runner_time: time,
        runner_collection: collection,
        runner_forfeit: forfeit,
        submission_datetime: current_time,
    };

    diesel::insert_into(leaderboard::table)
        .values(new_submission)
        .execute(&conn)?;

    Ok(())
}

#[inline]
pub fn get_leaderboard(db_pool: &MysqlPool) -> Result<Vec<OldSubmission>, Error> {
    let conn = db_pool
        .get()
        .expect("Error getting connection from db pool while getting leaderboard");
    let mut all_submissions: Vec<OldSubmission> =
        leaderboard::table.load::<OldSubmission>(&conn)?;
    all_submissions.sort_by(|a, b| {
        b.runner_time
            .cmp(&a.runner_time)
            .reverse()
            .then(b.runner_collection.cmp(&a.runner_collection).reverse())
    });
    Ok(all_submissions)
}

#[inline]
pub fn get_leaderboard_ids(db_pool: &MysqlPool) -> Result<Vec<u64>, Error> {
    use crate::schema::leaderboard::columns::runner_id;
    let conn = db_pool
        .get()
        .expect("Error getting connection from db pool while getting leaderboard post ids.");
    let leaderboard_ids: Vec<u64> = leaderboard::table.select(runner_id).load::<u64>(&conn)?;

    Ok(leaderboard_ids)
}

#[inline]
pub fn get_leaderboard_posts(
    leaderboard_channel: &u64,
    db_pool: &MysqlPool,
) -> Result<Vec<Post>, Error> {
    use crate::schema::posts::columns::guild_channel;
    let conn = db_pool
        .get()
        .expect("Error getting db connection from pool while getting leaderboard posts");
    let mut all_posts = posts::table
        .filter(guild_channel.eq(leaderboard_channel))
        .load::<Post>(&conn)?;
    all_posts.sort_by(|a, b| b.post_datetime.cmp(&a.post_datetime).reverse());
    Ok(all_posts)
}

#[inline]
pub fn get_submission_posts(
    submission_channel: &u64,
    db_pool: &MysqlPool,
) -> Result<Vec<Post>, Error> {
    use crate::schema::posts::columns::guild_channel;
    let conn = db_pool
        .get()
        .expect("Error getting db connection from pool while getting submission posts");
    let mut all_posts = posts::table
        .filter(guild_channel.eq(*submission_channel))
        .load::<Post>(&conn)
        .unwrap();
    all_posts.sort_by(|a, b| b.post_datetime.cmp(&a.post_datetime).reverse());
    Ok(all_posts)
}
