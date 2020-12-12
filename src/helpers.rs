use std::{
    collections::{HashMap, HashSet},
    error::Error,
};

use anyhow::Result;
use diesel::{
    mysql::MysqlConnection,
    r2d2::{ConnectionManager, Pool, PooledConnection},
};
use serenity::{client::Context, model::id::GuildId, prelude::TypeMapKey};
use uuid::Uuid;

use crate::discord::{channel_groups::ChannelGroup, servers::DiscordServer};

pub type BoxedError = Box<dyn Error + Send + Sync>;
pub type MysqlPool = Pool<ConnectionManager<MysqlConnection>>;
pub type PooledConn = PooledConnection<ConnectionManager<MysqlConnection>>;

pub struct GroupContainer;

// submission channels map to groups 1:1
impl TypeMapKey for GroupContainer {
    type Value = HashMap<u64, ChannelGroup>;
}

pub struct DBPool;

impl TypeMapKey for DBPool {
    type Value = MysqlPool;
}

pub struct ServerContainer;

impl TypeMapKey for ServerContainer {
    type Value = HashMap<GuildId, DiscordServer>;
}

pub struct SubmissionSet;

impl TypeMapKey for SubmissionSet {
    type Value = HashSet<u64>;
}

#[inline]
pub async fn get_connection(ctx: &Context) -> PooledConn {
    let conn = {
        let data = ctx.data.read().await;
        data.get::<DBPool>()
            .expect("Expected DB pool in ShareMap")
            .get()
            .unwrap() // we know the pool is there unless something went very wrong here
    };

    conn
}

#[inline]
pub fn get_pool(database_url: &str) -> Result<MysqlPool> {
    let manager = ConnectionManager::<MysqlConnection>::new(database_url);
    let pool = Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");

    Ok(pool)
}

#[inline]
pub fn new_uuid() -> Vec<u8> {
    let new_uuid = Uuid::new_v4().as_bytes().to_vec();

    new_uuid
}

#[inline]
pub fn bitmask(bits: u32) -> u32 {
    (1u32 << bits) - 1u32
}
