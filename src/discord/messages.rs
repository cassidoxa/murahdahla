use anyhow::{anyhow, Result};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use futures::{join, try_join};
use serenity::{
    framework::standard::macros::hook,
    model::{channel::Message, id::ChannelId},
    prelude::*,
    utils::MessageBuilder,
};

use crate::{
    discord::{
        channel_groups::{get_group, in_submission_channel, ChannelGroup, ChannelType},
        servers::add_spoiler_role,
        submissions::{process_submission, refresh_leaderboard, Submission},
    },
    games::{get_maybe_active_race, AsyncRaceData, BoxedGame, GameName},
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
    pub fn from_serenity_msg(
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
    // but let's keep this around for now
    async fn message(&self, _ctx: Context, _msg: Message) {
        ()
    }
}

#[hook]
pub async fn normal_message_hook(ctx: &Context, msg: &Message) {
    use crate::schema::submissions::columns::runner_name;
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
            let _ = delete_sub_msg(&ctx, &msg).await.map_err(|e| warn!("{}", e));
            return;
        }
    };

    // check for duplicates
    if Submission::belonging_to(&race)
        .filter(runner_name.eq(&msg.author.name))
        .first::<Submission>(&conn)
        .ok()
        .is_some()
    {
        info!("Duplicate submission from \"{}\"", &msg.author.name);
        let _ = delete_sub_msg(&ctx, &msg).await.map_err(|e| warn!("{}", e));
        return;
    }

    // here we parse a possible time submission. If we get a good submission, insert
    // it into the database and we'll call a function to refresh the leaderboard from the
    // db below
    match process_submission(&ctx, &msg, &race).await {
        Ok(_) => (),
        Err(e) => {
            let _ = delete_sub_msg(&ctx, &msg).await.map_err(|e| warn!("{}", e));
            warn!("Error processing submission: {}", e);
            return;
        }
    };

    // if processing the submission fails we don't want to do these but otherwise
    // they can probably be done concurrently
    let role_fut = add_spoiler_role(&ctx, &msg, group.spoiler_role_id);
    // refresh leaderboard from db
    let lb_fut = refresh_leaderboard(&ctx, &group, &race);
    let delete_fut = delete_sub_msg(&ctx, &msg);

    match try_join!(role_fut, lb_fut, delete_fut) {
        Ok(_) => (),
        Err(e) => {
            warn!("Error during post-submission: {}", e);
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

    let mut base_game_string = format!("{} - ", race_data.race_date);
    if race_data.race_game != GameName::Other {
        base_game_string.push_str(format!("{} - ", &game.game_name()).as_str());
    }
    base_game_string
        .push_str(format!("({}) - {}", race_data.race_type, &game.settings_str()?).as_str());
    if game.has_url() {
        base_game_string.push_str(format!(" - <{}>", game.game_url().unwrap()).as_str());
    }

    let lb_string = format!("Leaderboard for {}\n", base_game_string);

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
async fn delete_sub_msg(ctx: &Context, msg: &Message) -> Result<(), BoxedError> {
    let del = msg.delete(ctx).await;
    match del {
        Ok(_) => Ok(()),
        Err(e) => Err(anyhow!("Error deleting submission message: {}", e).into()),
    }
}
