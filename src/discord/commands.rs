use std::{collections::HashSet, convert::TryFrom, sync::Arc};

use anyhow::{anyhow, Result};
use chrono::{offset::Utc, NaiveDateTime, NaiveTime};
use diesel::{insert_into, prelude::*};
use futures::{join, try_join};
use serenity::{
    framework::standard::{
        macros::{command, group, hook},
        Args, CommandError, CommandResult,
    },
    model::{
        channel::{Message, ReactionType},
        event::ResumedEvent,
        gateway::Ready,
        guild::PartialGuild,
        id::{ChannelId, RoleId},
    },
    prelude::*,
};
use uuid::Uuid;

use crate::{
    discord::{
        channel_groups::{get_group, in_submission_channel, ChannelGroup},
        messages::{
            build_listgroups_message, get_lb_msgs_data, get_submission_msg_data,
            handle_new_race_messages, BotMessage,
        },
        servers::{add_server, check_permissions, parse_role, Permission, ServerRoleAction},
        submissions::{refresh_leaderboard, Submission},
    },
    games::{
        get_game_boxed, get_maybe_active_race, AsyncRaceData, BoxedGame, NewAsyncRaceData, RaceType,
    },
    helpers::*,
};

const REACT_COMMANDS: [&str; 6] = [
    "addgroup",
    "removegroup",
    "setmodrole",
    "setadminrole",
    "removemodrole",
    "removeadminrole",
];

#[hook]
pub async fn before_hook(ctx: &Context, msg: &Message, _cmd_name: &str) -> bool {
    // before any command is run we check to see if we have the server in the share map
    // if not, we add it to the map and the database
    let server_check = {
        let data = ctx.data.read().await;
        let check = data
            .get::<ServerContainer>()
            .expect("No server hashmap in share map")
            .contains_key(&msg.guild_id.unwrap());

        check
    };
    if server_check == false {
        match add_server(&ctx, &msg).await {
            Ok(_) => (),
            Err(e) => {
                error!("Error adding new server: {}", e);
                return false;
            }
        }
    }

    true
}

#[hook]
pub async fn after_hook(
    ctx: &Context,
    msg: &Message,
    cmd_name: &str,
    error: Result<(), CommandError>,
) {
    let mut successful: bool = true;
    if let Err(e) = error {
        successful = false;
        warn!(
            "Error running \"{}\" command from user \"{}\": {:?}",
            cmd_name, &msg.author.name, e
        );
    }
    if REACT_COMMANDS.iter().any(|&c| c == cmd_name) {
        let reaction = match successful {
            true => ReactionType::try_from("👍").unwrap(), // should never fail i think
            false => ReactionType::try_from("👎").unwrap(),
        };
        match msg.react(&ctx, reaction).await {
            Ok(_) => (),
            Err(e) => {
                warn!(
                    "Error reacting to command \"{}\" from user \"{}\": {}",
                    cmd_name, &msg.author.name, e
                );
            }
        };
    }

    // always delete messages in the submission channel to keep it clean
    if in_submission_channel(&ctx, &msg).await {
        msg.delete(&ctx)
            .await
            .unwrap_or_else(|e| warn!("Error deleting message: {}", e));
    }
    info!("Successfully executed command: {}", cmd_name);

    ()
}

#[group]
#[commands(
    igtstart,
    rtastart,
    stop,
    addgroup,
    removegroup,
    listgroups,
    setmodrole,
    setadminrole,
    removemodrole,
    removeadminrole,
    changeentry,
    refresh
)]
struct General;

#[command]
#[bucket = "startrace"]
pub async fn igtstart(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Mod).await?;
    start_race(&ctx, &msg, args, RaceType::IGT).await?;

    Ok(())
}

#[command]
#[bucket = "startrace"]
pub async fn rtastart(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Mod).await?;
    start_race(&ctx, &msg, args, RaceType::RTA).await?;

    Ok(())
}

#[command]
pub async fn stop(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    // this must run in a submission channel because we need a group and a maybe-race
    if !in_submission_channel(&ctx, &msg).await {
        return Err(anyhow!("User \"{}\" ran stop command outside of a submission channel").into());
    }
    let group_fut = get_group(&ctx, &msg);
    let conn_fut = get_connection(&ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    let maybe_active_race = get_maybe_active_race(&conn, &group);
    match maybe_active_race {
        Some(r) => stop_race(&ctx, &r, &group).await?,
        None => {
            return Err(anyhow!(
                "User \"{}\" ran stop command in a channel with no active race",
                &msg.author.name
            )
            .into())
        }
    };

    Ok(())
}

#[command]
pub async fn addgroup(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    use crate::schema::channels::columns::*;
    use crate::schema::channels::dsl::*;
    use diesel::dsl::count;

    check_permissions(&ctx, &msg, Permission::Admin).await?;
    match msg.attachments.len() {
        1 => (),
        _ => {
            let err: BoxedError = anyhow!("!addgroup requires one attachment").into();
            return Err(err);
        }
    }

    // let's check and make sure that no server has more than ten groups
    // for the sake of performance and not crashing the bot
    let conn = get_connection(&ctx).await;
    let num_groups: usize = {
        let data = ctx.data.read().await;
        let group_map = data
            .get::<GroupContainer>()
            .expect("No group container in share map");
        group_map.len()
    };
    if num_groups >= 10 {
        return Err(anyhow!("Cannot add more than 10 groups per server").into());
    }

    let attachment = msg.attachments[0].download().await?;
    let new_group = ChannelGroup::new_from_yaml(&msg, &ctx, &attachment).await?;
    insert_into(channels).values(&new_group).execute(&conn)?;
    {
        let mut data = ctx.data.write().await;
        let submission_set = data
            .get_mut::<SubmissionSet>()
            .expect("No submission set in share map.");
        submission_set.insert(new_group.submission);
        let group_map = data
            .get_mut::<GroupContainer>()
            .expect("No channel group hashmap in share map.");
        group_map.insert(new_group.submission, new_group);
    }

    msg.react(&ctx, ReactionType::try_from("👍")?).await?;
    Ok(())
}

#[command]
pub async fn removegroup(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    use crate::schema::channels::columns::*;
    use crate::schema::channels::dsl::*;

    check_permissions(&ctx, &msg, Permission::Admin).await?;
    let this_group_name = args.single_quoted::<String>()?;
    let this_server_id = *msg.guild_id.unwrap().as_u64();
    let conn = get_connection(&ctx).await;
    // we have to be a little inefficient here unless we change how groups are looked
    // up in the share map
    let group_submission: u64 = channels
        .select(submission)
        .filter(server_id.eq(this_server_id))
        .filter(group_name.eq(this_group_name))
        .get_result(&conn)?;

    {
        let mut data = ctx.data.write().await;
        let mut group_map = data
            .get_mut::<GroupContainer>()
            .expect("No group container in share map");
        group_map
            .remove(&group_submission)
            .ok_or(anyhow!("Error removing group from share map"))?;
        let mut submission_set = data
            .get_mut::<SubmissionSet>()
            .expect("No submission set in share map");
        submission_set.remove(&group_submission);
    };
    diesel::delete(channels)
        .filter(submission.eq(group_submission))
        .execute(&conn)?;

    Ok(())
}

#[command]
pub async fn listgroups(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    use crate::schema::channels::columns::*;
    use crate::schema::channels::dsl::*;

    check_permissions(&ctx, &msg, Permission::Admin).await?;
    let conn = get_connection(&ctx).await;
    let this_server_id = *msg.guild_id.unwrap().as_u64();
    let group_names = {
        let data = ctx.data.read().await;
        let group_map = data
            .get::<GroupContainer>()
            .expect("No group container in share map");
        let group_names: Vec<String> = group_map
            .values()
            .filter(|g| g.server_id == this_server_id)
            .map(|g| g.group_name.clone())
            .collect();

        group_names
    };
    let group_string = build_listgroups_message(group_names);
    msg.author
        .direct_message(&ctx, |m| m.content(group_string))
        .await?;

    Ok(())
}

#[command]
pub async fn setadminrole(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Admin).await?;
    set_role_from_command(&ctx, &msg, args, Permission::Admin, ServerRoleAction::Add).await?;

    Ok(())
}

#[command]
pub async fn setmodrole(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Admin).await?;
    set_role_from_command(&ctx, &msg, args, Permission::Mod, ServerRoleAction::Add).await?;

    Ok(())
}

#[command]
pub async fn removeadminrole(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Admin).await?;
    set_role_from_command(
        &ctx,
        &msg,
        args,
        Permission::Admin,
        ServerRoleAction::Remove,
    )
    .await?;

    Ok(())
}

#[command]
pub async fn removemodrole(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Admin).await?;
    set_role_from_command(
        &ctx,
        &msg,
        args,
        Permission::Admin,
        ServerRoleAction::Remove,
    )
    .await?;

    Ok(())
}

#[command]
pub async fn removetime(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Mod).await?;
    todo!();
}

#[command]
pub async fn refresh(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Mod).await?;
    if !in_submission_channel(&ctx, &msg).await {
        return Err(anyhow!("Refresh command must be run in a submissions channel").into());
    }
    let group_fut = get_group(&ctx, &msg);
    let conn_fut = get_connection(&ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    let maybe_active_race = get_maybe_active_race(&conn, &group);
    match maybe_active_race {
        Some(r) => refresh_leaderboard(&ctx, &group, &r).await?,
        None => return Err(anyhow!("Ran refresh command with no active race").into()),
    };

    Ok(())
}

#[command]
pub async fn changeentry(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    check_permissions(&ctx, &msg, Permission::Mod).await?;
    todo!();
}

async fn set_role_from_command(
    ctx: &Context,
    msg: &Message,
    mut args: Args,
    role_type: Permission,
    role_action: ServerRoleAction,
) -> Result<(), BoxedError> {
    use crate::schema::servers::columns::*;
    use crate::schema::servers::dsl::*;

    let role_id: Option<u64> = match role_action {
        ServerRoleAction::Add => Some(parse_role(&ctx, &msg, args).await?),
        ServerRoleAction::Remove => None,
    };
    let this_server_id = msg.guild_id.unwrap();
    let conn = get_connection(&ctx).await;

    match role_type {
        Permission::Admin => {
            diesel::update(servers.find(*this_server_id.as_u64()))
                .set(admin_role_id.eq(role_id))
                .execute(&conn)?;
        }
        Permission::Mod => {
            diesel::update(servers.find(*this_server_id.as_u64()))
                .set(mod_role_id.eq(role_id))
                .execute(&conn)?;
        }
        _ => (),
    };
    {
        let mut data = ctx.data.write().await;
        let mut server = data
            .get_mut::<ServerContainer>()
            .expect("No server container in share map")
            .get_mut(&this_server_id)
            .unwrap(); // the server will be here on account of the before hook
        server.set_role(role_id, role_type);
    }

    msg.react(&ctx, ReactionType::try_from("👍")?).await?;

    Ok(())
}

async fn start_race(
    ctx: &Context,
    msg: &Message,
    mut args: Args,
    this_race_type: RaceType,
) -> Result<(), BoxedError> {
    use crate::schema::async_races::columns::*;
    use crate::schema::async_races::dsl::*;

    // this command must be run in a submission channel
    if !in_submission_channel(&ctx, &msg).await {
        return Err(anyhow!("Games must be started in a submissions channel").into());
    }
    let group_fut = get_group(&ctx, &msg);
    let conn_fut = get_connection(&ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    // determine if a game is already running in this group. if yes, stop the game
    // before starting a new one.
    let maybe_active_race = get_maybe_active_race(&conn, &group);
    match maybe_active_race {
        Some(r) => stop_race(&ctx, &r, &group).await?,
        None => (),
    };
    let game: BoxedGame = get_game_boxed(&args).await?;
    let new_race_data =
        NewAsyncRaceData::new_from_game(&game, &group.channel_group_id, this_race_type);
    insert_into(async_races)
        .values(&new_race_data)
        .execute(&conn)?;

    // pull this back out to get the race id :shrug:
    // mysql does not support returning statements
    let race_data: AsyncRaceData = async_races
        .filter(channel_group_id.eq(&group.channel_group_id))
        .filter(race_active.eq(true))
        .get_result(&conn)?;

    // use boxed game to build and post messages in submission and leaderboard channels
    // add both messages to messages table. rows in this table belong to async races.
    handle_new_race_messages(&ctx, &group, &game, &race_data).await?;

    Ok(())
}

async fn stop_race(ctx: &Context, race: &AsyncRaceData, group: &ChannelGroup) -> Result<()> {
    use crate::schema::async_races;
    use crate::schema::messages;
    let conn = get_connection(&ctx).await;
    diesel::update(race)
        .set(async_races::race_active.eq(false))
        .execute(&conn)?;

    // there will always only be one submission message per race
    let sub_msg_data = get_submission_msg_data(&conn, race.race_id)?;
    let mut submission_msg = ctx
        .http
        .get_message(sub_msg_data.channel_id, sub_msg_data.message_id)
        .await?;

    // theoretically we already have a perfectly formed leaderboard in this group
    // so we can just grab it then edit the submission msg + send any extras for
    // larger leaderboards.
    let mut leaderboard_msgs_data: Vec<BotMessage> = get_lb_msgs_data(&conn, race.race_id)?;
    if leaderboard_msgs_data.len() == 0 {
        // this should never happen
        let err: &'static str =
            "Tried to stop active game with no leaderboard messages in database";
        error!("{}", &err);
        return Err(anyhow!(err).into());
    } else if leaderboard_msgs_data.len() == 1 {
        let lb_msg_data = &leaderboard_msgs_data[0];
        let lb_msg = ctx
            .http
            .get_message(lb_msg_data.channel_id, lb_msg_data.message_id)
            .await?;
        let sub_msg_fut = submission_msg.edit(&ctx, |m| m.content(&lb_msg.content));
        let lb_msg_fut = lb_msg.delete(&ctx);
        try_join!(sub_msg_fut, lb_msg_fut)?;
    } else {
        let sub_channel = ChannelId::from(sub_msg_data.channel_id);
        let lb_msg_data = leaderboard_msgs_data.remove(0);
        let lb_msg = ctx
            .http
            .get_message(lb_msg_data.channel_id, lb_msg_data.message_id)
            .await?;
        let sub_msg_fut = submission_msg.edit(&ctx, |m| m.content(&lb_msg.content));
        let lb_msg_fut = lb_msg.delete(&ctx);
        try_join!(sub_msg_fut, lb_msg_fut)?;
        // we loop through the additional messages if we have a leaderboard > 1 message
        for d in leaderboard_msgs_data.iter() {
            let msg = ctx.http.get_message(d.channel_id, d.message_id).await?;
            let sub_msg_fut = sub_channel.say(&ctx, &msg.content);
            let lb_msg_fut = msg.delete(&ctx);
            try_join!(sub_msg_fut, lb_msg_fut)?;
        }
    }

    // remove spoiler roles to revoke access to spoiler channel when game is over
    remove_spoiler_roles(&ctx, &group, &race).await?;

    Ok(())
}

async fn remove_spoiler_roles(
    ctx: &Context,
    group: &ChannelGroup,
    race: &AsyncRaceData,
) -> Result<()> {
    // collect the user ids of everyone with a submission in this race
    // so we can use them to remove the spoiler role when the race has stopped
    use crate::schema::submissions::columns::*;
    use crate::schema::submissions::dsl::*;

    let conn = get_connection(&ctx).await;
    let user_ids = Submission::belonging_to(race)
        .select(runner_id)
        .load::<u64>(&conn)?;
    for id in user_ids {
        let mut member = match ctx.http.get_member(group.server_id, id).await {
            Ok(m) => m,
            Err(e) => {
                warn!("Error getting member from id: {}", e);
                continue;
            }
        };
        match &member.remove_role(&ctx, group.spoiler_role_id).await {
            Ok(()) => (),
            Err(e) => warn!("Error removing role: {}", e),
        };
    }

    Ok(())
}
