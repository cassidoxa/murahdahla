use anyhow::{anyhow, Result};
use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serenity::{async_trait, model::channel::Message, prelude::*, utils::MessageBuilder};

use crate::{discord::servers::in_submission_channel, games::AsyncRaceData, helpers::*, schema::*};

#[derive(Debug, Insertable, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "AsyncRaceData", foreign_key = "race_id")]
#[table_name = "messages"]
#[primary_key(message_id)]
pub struct BotMessage {
    pub message_id: u64,
    pub message_datetime: NaiveDateTime,
    pub race_id: u32,
    pub server_id: u64,
    pub race_active: bool,
    pub channel_id: u64,
    pub message_type: String,
}

pub struct Handler;

#[serenity::async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // before handling we check if this message is in a submission channel.
        // if it's not, then we return and ignore it
        ()
    }
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
