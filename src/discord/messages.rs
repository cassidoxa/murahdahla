use anyhow::{anyhow, Result};
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use futures::{join, try_join};
use serenity::{
    async_trait,
    framework::standard::macros::hook,
    model::{channel::Message, id::ChannelId},
    prelude::*,
    utils::MessageBuilder,
};

use crate::{
    discord::{
        channel_groups::{get_group, in_submission_channel, ChannelGroup, ChannelType},
        submissions::process_submission,
    },
    games::{get_maybe_active_race, AsyncRaceData, BoxedGame},
    helpers::*,
    schema::*,
};

#[derive(Debug, Insertable, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "AsyncRaceData", foreign_key = "race_id")]
#[table_name = "messages"]
#[primary_key(message_id)]
pub struct BotMessage {
    pub message_id: u64,
    pub message_datetime: NaiveDateTime,
    pub race_id: u32,
    pub server_id: u64,
    pub channel_id: u64,
    pub channel_type: ChannelType,
}

impl BotMessage {
    fn from_serenity_msg(
        msg: &Message,
        server_id: u64,
        race_id: u32,
        channel_type: ChannelType,
    ) -> Self {
        BotMessage {
            message_id: *msg.id.as_u64(),
            message_datetime: msg.timestamp.naive_utc(),
            race_id: race_id,
            server_id: server_id,
            channel_id: *msg.channel_id.as_u64(),
            channel_type: channel_type,
        }
    }
}

pub struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    // we may not need an event handler since our hooks grab everything we need
    async fn message(&self, ctx: Context, msg: Message) {
        ()
    }
}

#[hook]
pub async fn normal_message_hook(ctx: &Context, msg: &Message) {
    // the only non-command messages we're interested in are time submissions from
    // non bot users
    if !in_submission_channel(&ctx, &msg).await
        || (msg.author.id == { ctx.cache.current_user_id().await })
    {
        return;
    }
    let group_fut = get_group(&ctx, &msg);
    let conn_fut = get_connection(&ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    let maybe_active_race: Option<AsyncRaceData> = get_maybe_active_race(&conn, &group);
    let race = match maybe_active_race {
        Some(r) => r,
        None => {
            // if there's no active race we still want to delete messages and keep this
            // channel tidy before returning
            delete_sub_msg(&ctx, &msg).await;
            return;
        }
    };

    // here we parse a possible time submission. If we get a good submission, insert
    // it into the database and we'll call a function to refresh the leaderboard from the
    // db below
    match process_submission(&ctx, &msg, &group, &race).await {
        Ok(()) => (),
        Err(e) => {
            warn!("Error processing submission: {}", e);
            return;
        }
    };

    ()
}

pub fn build_listgroups_message(mut groups: Vec<String>) -> String {
    match groups.len() {
        0 => {
            MessageBuilder::new()
                .push_codeblock("There are no groups in this server.", None)
                .push("\n")
                .push("Use the `!addgroup` command with a yaml file to add a group. See the example at <https://github.com/cassidoxa/murahdahla>")
                .build()
        }
        1 => {
            MessageBuilder::new()
                .push_codeblock(groups.remove(0), None)
                .build()
        }
        _ => {
            // 20 bytes seems like enough for most servers :shrug:
            let mut group_list: String = String::with_capacity(20);
            group_list.push_str(groups.remove(0).as_str());
            groups
                .drain(..)
                .for_each(|g| group_list.push_str(format!(", {}", g).as_str()));
            MessageBuilder::new()
                .push_codeblock(group_list, None)
                .build()
        }
    }
}

pub async fn handle_new_race_messages(
    ctx: &Context,
    group: &ChannelGroup,
    game: &BoxedGame,
    race_data: &AsyncRaceData,
) -> Result<(), BoxedError> {
    use crate::schema::messages::dsl::*;

    let mut base_game_string = format!(
        "{} - {} ({}) - {}",
        race_data.race_date,
        &game.game_name(),
        race_data.race_type,
        &game.settings_str()?
    );
    if game.has_url() {
        base_game_string.push_str(format!(" - {}", game.game_url().unwrap()).as_str());
    }
    let lb_string = format!("Leaderboard for {}:\n", base_game_string);

    let sub_channel = ChannelId::from(group.submission);
    let lb_channel = ChannelId::from(group.leaderboard);

    let (lb_message, sub_message) = try_join!(
        lb_channel.say(&ctx, &lb_string),
        sub_channel.say(&ctx, &base_game_string)
    )?;

    // a reference to PooledConnection is not Send so we need to grab a connection here
    // instead of passing one in
    let conn = get_connection(&ctx).await;
    let new_messages = vec![
        BotMessage::from_serenity_msg(
            &sub_message,
            group.server_id,
            race_data.race_id,
            ChannelType::Submission,
        ),
        BotMessage::from_serenity_msg(
            &lb_message,
            group.server_id,
            race_data.race_id,
            ChannelType::Leaderboard,
        ),
    ];
    diesel::insert_into(messages)
        .values(&new_messages)
        .execute(&conn)?;

    Ok(())
}

#[inline]
pub fn get_lb_msgs_data(conn: &PooledConn, this_race_id: u32) -> Result<Vec<BotMessage>> {
    // retrieves data about bot messages in a leaderboard channel for a given race id
    use crate::schema::messages::columns::*;
    use crate::schema::messages::dsl::messages;
    let mut active_posts = messages
        .filter(race_id.eq(this_race_id))
        .filter(channel_type.eq(ChannelType::Leaderboard))
        .load::<BotMessage>(conn)?;
    active_posts.sort_by(|a, b| b.message_datetime.cmp(&a.message_datetime).reverse());
    Ok(active_posts)
}

#[inline]
pub fn get_submission_msg_data(conn: &PooledConn, this_race_id: u32) -> Result<BotMessage> {
    // this function should only ever be called when we know there is currently an active race
    // with an associated submission message
    use crate::schema::messages::columns::*;
    use crate::schema::messages::dsl::messages;

    let sub_message = messages
        .filter(race_id.eq(this_race_id))
        .filter(channel_type.eq(ChannelType::Submission))
        .first::<BotMessage>(conn)?;

    Ok(sub_message)
}

#[inline]
async fn delete_sub_msg(ctx: &Context, msg: &Message) -> () {
    let _ = msg
        .delete(ctx)
        .await
        .map_err(|e| warn!("Error deleting message in submission channel: {}", e));
}

// #[serenity::async_trait]
// impl EventHandler for Handler {
//     fn message(&self, ctx: Context, msg: Message) {
//         let data = ctx.data.read();
//         let game_active: bool = *data
//             .get::<ActiveGames>()
//             .expect("No active game toggle set");
//         let guild_id = match msg.guild_id {
//             Some(guild_id) => guild_id,
//             None => return,
//         };
//         let guild = match guild_id.to_partial_guild(&ctx.http) {
//             Ok(guild) => guild,
//             Err(e) => {
//                 warn!("Couldn't get partial guild from REST API. ERROR: {}", e);
//                 return;
//             }
//         };
//
//         let admin_role = match get_admin_role(&guild) {
//             Ok(admin_role) => admin_role,
//             Err(e) => {
//                 warn!("Couldn't get admin role. Check if properly set in environment variables. ERROR: {}", e);
//                 return;
//             }
//         };
//
//         if msg.author.id != ctx.cache.read().user.id
//             && game_active
//             && msg.channel_id.as_u64()
//                 == match &env::var("SUBMISSION_CHANNEL_ID")
//                     .expect("No submissions channel in the environment")
//                     .parse::<u64>()
//                 {
//                     Ok(channel_u64) => channel_u64,
//                     Err(e) => {
//                         warn!("Error parsing channel id: {}", e);
//                         return;
//                     }
//                 }
//             && (msg
//                 .member
//                 .as_ref()
//                 .unwrap()
//                 .roles
//                 .iter()
//                 .find(|x| x.as_u64() == admin_role.as_u64())
//                 == None
//                 || [msg.content.as_str().bytes().nth(0).unwrap()] != "!".as_bytes())
//         {
//             info!(
//                 "Received message from {}: \"{}\"",
//                 &msg.author.name, &msg.content
//             );
//             match process_time_submission(&ctx, &msg) {
//                 Ok(()) => (),
//                 Err(e) => {
//                     warn!("Error processing time submission: {}", e);
//                 }
//             };
//
//             msg.delete(&ctx)
//                 .unwrap_or_else(|e| info!("Error deleting message: {}", e));
//             match update_leaderboard(&ctx, *guild_id.as_u64()) {
//                 Ok(()) => (),
//                 Err(e) => {
//                     warn!("Error updating leaderboard: {}", e);
//                     return;
//                 }
//             };
//         }
//     }
//
//     fn ready(&self, _: Context, ready: Ready) {
//         info!("{} is connected!", ready.user.name);
//     }
//
//     fn resume(&self, _: Context, _: ResumedEvent) {
//         info!("Resumed");
//     }
// }
//
// fn resize_leaderboard(
//     ctx: &Context,
//     db_pool: &Pool<ConnectionManager<MysqlConnection>>,
//     guild_id: u64,
//     leaderboard_channel: u64,
//     new_posts: usize,
// ) -> Result<Vec<Post>> {
//     // we need one more post than we have to hold all submissions
//     for _n in 0..new_posts {
//         let new_message: Message =
//             ChannelId::from(leaderboard_channel).say(&ctx.http, "Placeholder")?;
//         create_post_entry(
//             db_pool,
//             *new_message.id.as_u64(),
//             Utc::now().naive_utc(),
//             guild_id,
//             *new_message.channel_id.as_u64(),
//         )?;
//     }
//     let leaderboard_posts: Vec<Post> = get_leaderboard_posts(&leaderboard_channel, db_pool)?;
//
//     Ok(leaderboard_posts)
// }
//
// fn fill_leaderboard_update(
//     ctx: &Context,
//     db_pool: &Pool<ConnectionManager<MysqlConnection>>,
//     guild_id: u64,
//     leaderboard_string: String,
//     mut leaderboard_posts: Vec<Post>,
//     channel: u64,
// ) -> Result<()> {
//     let necessary_posts: usize = leaderboard_string.len() / 2000 + 1;
//
//     if necessary_posts > leaderboard_posts.len() {
//         let new_posts: usize = necessary_posts - leaderboard_posts.len();
//         leaderboard_posts = match resize_leaderboard(ctx, db_pool, guild_id, channel, new_posts) {
//             Ok(posts) => posts,
//             Err(e) => {
//                 warn!("Error resizing leaderboard: {}", e);
//                 return Ok(());
//             }
//         };
//     }
//
//     // fill buffer then send the post until there's no more
//     let mut post_buffer = String::with_capacity(2000);
//     let mut post_iterator = leaderboard_posts.into_iter().peekable();
//     let mut submission_iterator = leaderboard_string
//         .split("\n")
//         .collect::<Vec<&str>>()
//         .into_iter()
//         .peekable();
//
//     loop {
//         if post_iterator.peek().is_none() {
//             warn!("Error: Ran out of space for leaderboard");
//             break;
//         }
//
//         match submission_iterator.peek() {
//             Some(line) => {
//                 if line.len() + &post_buffer.len() <= 2000 {
//                     post_buffer
//                         .push_str(format!("\n{}", submission_iterator.next().unwrap()).as_str())
//                 } else if line.len() + post_buffer.len() > 2000 {
//                     let mut post = ctx
//                         .http
//                         .get_message(channel, post_iterator.next().unwrap().post_id)?;
//                     post.edit(ctx, |x| x.content(&post_buffer))?;
//                     post_buffer.clear();
//                 }
//             }
//             None => {
//                 let mut post = ctx
//                     .http
//                     .get_message(channel, post_iterator.next().unwrap().post_id)?;
//                 post.edit(ctx, |x| x.content(post_buffer))?;
//                 break;
//             }
//         };
//     }
//
//     Ok(())
// }
