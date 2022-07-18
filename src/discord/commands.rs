use std::{convert::TryFrom, str::FromStr};

use anyhow::{anyhow, Result};
use diesel::{insert_into, prelude::*};
use futures::{join, try_join};
use serenity::{
    framework::standard::{
        macros::{command, group, hook},
        Args, CommandError, CommandResult,
    },
    model::channel::{Message, ReactionType},
    prelude::*,
};

use crate::{
    discord::{
        channel_groups::{get_group, in_submission_channel, ChannelGroup, ChannelType},
        messages::{
            build_listgroups_message, get_lb_msgs_data, handle_new_race_messages, BotMessage,
        },
        servers::{add_server, check_permissions, parse_role, Permission, ServerRoleAction},
        submissions::{build_leaderboard, parse_variable_time, Submission},
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
    if !server_check {
        match add_server(ctx, msg).await {
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
    if in_submission_channel(ctx, msg).await {
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
    startigt,
    rtastart,
    startrta,
    stop,
    addgroup,
    removegroup,
    listgroups,
    setmodrole,
    setadminrole,
    removemodrole,
    removeadminrole,
    settime,
    setcollection,
    refresh,
    removetime
)]
struct General;

// it's basically free to have two commands for starting each kind of race so why
// not for the sake of ease-of-use
#[command]
#[bucket = "startrace"]
pub async fn igtstart(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Mod).await?;
    start_race(ctx, msg, args, RaceType::IGT).await?;

    Ok(())
}

#[command]
#[bucket = "startrace"]
pub async fn startigt(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Mod).await?;
    start_race(ctx, msg, args, RaceType::IGT).await?;

    Ok(())
}

#[command]
#[bucket = "startrace"]
pub async fn rtastart(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Mod).await?;
    start_race(ctx, msg, args, RaceType::RTA).await?;

    Ok(())
}

#[command]
#[bucket = "startrace"]
pub async fn startrta(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Mod).await?;
    start_race(ctx, msg, args, RaceType::RTA).await?;

    Ok(())
}

#[command]
pub async fn stop(ctx: &Context, msg: &Message) -> CommandResult {
    // this must run in a submission channel because we need a group and a maybe-race
    check_permissions(ctx, msg, Permission::Mod).await?;
    if !in_submission_channel(ctx, msg).await {
        return Ok(());
    }
    let group_fut = get_group(ctx, msg);
    let conn_fut = get_connection(ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    let maybe_active_race = get_maybe_active_race(&conn, &group);
    match maybe_active_race {
        Some(r) => stop_race(ctx, &r, &group).await?,
        None => return Ok(()),
    };

    Ok(())
}

#[command]
pub async fn addgroup(ctx: &Context, msg: &Message) -> CommandResult {
    use crate::schema::channels::dsl::*;

    check_permissions(ctx, msg, Permission::Admin).await?;
    match msg.attachments.len() {
        1 => (),
        _ => {
            let err: BoxedError = anyhow!("!addgroup requires one attachment").into();
            return Err(err);
        }
    }

    // let's check and make sure that no server has more than ten groups
    // for the sake of performance and not crashing the bot
    let conn = get_connection(ctx).await;
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
    let new_group = ChannelGroup::new_from_yaml(msg, ctx, &attachment).await?;
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

    check_permissions(ctx, msg, Permission::Admin).await?;
    let this_group_name = args.single_quoted::<String>()?;
    let this_server_id = *msg.guild_id.unwrap().as_u64();
    let conn = get_connection(ctx).await;
    let group_submission: u64 = channels
        .select(submission)
        .filter(server_id.eq(this_server_id))
        .filter(group_name.eq(this_group_name))
        .get_result(&conn)?;

    {
        let mut data = ctx.data.write().await;
        let group_map = data
            .get_mut::<GroupContainer>()
            .expect("No group container in share map");
        group_map
            .remove(&group_submission)
            .ok_or_else(|| anyhow!("Error removing group from share map"))?;
        let submission_set = data
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
pub async fn listgroups(ctx: &Context, msg: &Message) -> CommandResult {
    check_permissions(ctx, msg, Permission::Admin).await?;
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
pub async fn setadminrole(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Admin).await?;
    set_role_from_command(ctx, msg, args, Permission::Admin, ServerRoleAction::Add).await?;

    Ok(())
}

#[command]
pub async fn setmodrole(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Admin).await?;
    set_role_from_command(ctx, msg, args, Permission::Admin, ServerRoleAction::Add).await?;

    Ok(())
}

#[command]
pub async fn removeadminrole(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Admin).await?;
    set_role_from_command(ctx, msg, args, Permission::Admin, ServerRoleAction::Remove).await?;

    Ok(())
}

#[command]
pub async fn removemodrole(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    check_permissions(ctx, msg, Permission::Admin).await?;
    set_role_from_command(ctx, msg, args, Permission::Admin, ServerRoleAction::Remove).await?;

    Ok(())
}

#[command]
pub async fn removetime(ctx: &Context, msg: &Message, args: Args) -> CommandResult {
    use crate::schema::submissions::columns::*;
    use crate::schema::submissions::dsl::*;

    check_permissions(ctx, msg, Permission::Mod).await?;
    if !in_submission_channel(ctx, msg).await {
        return Ok(());
    }
    if args.len() != 1 {
        return Err(anyhow!("removetime command must have a single argument (runner name)").into());
    }
    let maybe_runner: &str = args.rest().trim_end();

    let group_fut = get_group(ctx, msg);
    let conn_fut = get_connection(ctx);
    let (group, conn) = join!(group_fut, conn_fut);
    let race = match get_maybe_active_race(&conn, &group) {
        Some(r) => r,
        None => return Ok(()),
    };
    match diesel::delete(submissions)
        .filter(race_id.eq(race.race_id))
        .filter(runner_name.eq(maybe_runner))
        .execute(&conn)
    {
        Ok(_) => (),
        Err(_) => {
            return Err(anyhow!(
                "Could not remove submission for \"{}\" in this race",
                &maybe_runner
            )
            .into())
        }
    };
    let mut member = msg.member(&ctx).await?;
    match &member.remove_role(&ctx, group.spoiler_role_id).await {
        Ok(()) => (),
        Err(e) => warn!(
            "Error removing role for user \"{}\": {}",
            &msg.author.name, e
        ),
    };
    build_leaderboard(ctx, &group, &race, ChannelType::Leaderboard).await?;

    Ok(())
}

#[command]
pub async fn refresh(ctx: &Context, msg: &Message) -> CommandResult {
    check_permissions(ctx, msg, Permission::Mod).await?;
    if !in_submission_channel(ctx, msg).await {
        return Ok(());
    }
    let group_fut = get_group(ctx, msg);
    let conn_fut = get_connection(ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    let maybe_active_race = get_maybe_active_race(&conn, &group);
    match maybe_active_race {
        Some(r) => build_leaderboard(ctx, &group, &r, ChannelType::Leaderboard).await?,
        None => return Ok(()),
    };

    Ok(())
}

#[command]
pub async fn settime(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    use crate::schema::submissions::columns::*;
    // we could and should write a command that will change an entire submission based on
    // game, especially if we get games were people will be using any optional, non
    // collection rate fields etc. but for now a command that simply changes the time
    // is sufficient.
    check_permissions(ctx, msg, Permission::Mod).await?;
    if !in_submission_channel(ctx, msg).await {
        return Ok(());
    }

    let group_fut = get_group(ctx, msg);
    let conn_fut = get_connection(ctx);
    let (group, conn) = join!(group_fut, conn_fut);
    let race = match get_maybe_active_race(&conn, &group) {
        Some(r) => r,
        None => return Ok(()),
    };
    if args.len() != 2 {
        return Err(
            anyhow!("settime command requires two arguments (runner name and new time)").into(),
        );
    }
    //
    let maybe_runner = args.single::<String>()?;
    let maybe_time = args.single::<String>()?;
    let new_time = parse_variable_time(&maybe_time)?;
    let submission: Submission = match Submission::belonging_to(&race)
        .filter(runner_name.eq(&maybe_runner))
        .first(&conn)
    {
        Ok(s) => s,
        Err(_) => {
            return Err(anyhow!(
                "Could not find submission for runner \"{}\" in this race",
                &maybe_runner
            )
            .into())
        }
    };
    diesel::update(&submission)
        .set(runner_time.eq(new_time))
        .execute(&conn)?;
    build_leaderboard(ctx, &group, &race, ChannelType::Leaderboard).await?;

    Ok(())
}

#[command]
pub async fn setcollection(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    use crate::schema::submissions::columns::*;
    check_permissions(ctx, msg, Permission::Mod).await?;
    if !in_submission_channel(ctx, msg).await {
        return Ok(());
    }

    let group_fut = get_group(ctx, msg);
    let conn_fut = get_connection(ctx);
    let (group, conn) = join!(group_fut, conn_fut);
    let race = match get_maybe_active_race(&conn, &group) {
        Some(r) => r,
        None => return Ok(()),
    };
    if args.len() != 2 {
        return Err(anyhow!(
            "setcollection command requires two arguments (runner name and new collection rate)"
        )
        .into());
    }
    //
    let maybe_runner = args.single::<String>()?;
    let maybe_collection = args.single::<String>()?;
    let new_collection = u16::from_str(&maybe_collection)?;
    let submission: Submission = match Submission::belonging_to(&race)
        .filter(runner_name.eq(&maybe_runner))
        .first(&conn)
    {
        Ok(s) => s,
        Err(_) => {
            return Err(anyhow!(
                "Could not find submission for runner \"{}\" in this race",
                &maybe_runner
            )
            .into())
        }
    };
    diesel::update(&submission)
        .set(runner_collection.eq(new_collection))
        .execute(&conn)?;
    build_leaderboard(ctx, &group, &race, ChannelType::Leaderboard).await?;

    Ok(())
}

async fn set_role_from_command(
    ctx: &Context,
    msg: &Message,
    args: Args,
    role_type: Permission,
    role_action: ServerRoleAction,
) -> Result<(), BoxedError> {
    use crate::schema::servers::columns::*;
    use crate::schema::servers::dsl::*;

    let role_id: Option<u64> = match role_action {
        ServerRoleAction::Add => Some(parse_role(ctx, msg, args).await?),
        ServerRoleAction::Remove => None,
    };
    let this_server_id = msg.guild_id.unwrap();
    let conn = get_connection(ctx).await;

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
        let server = data
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
    args: Args,
    this_race_type: RaceType,
) -> Result<(), BoxedError> {
    use crate::schema::async_races::columns::*;
    use crate::schema::async_races::dsl::*;

    // this command must be run in a submission channel
    if !in_submission_channel(ctx, msg).await {
        return Ok(());
    }
    let group_fut = get_group(ctx, msg);
    let conn_fut = get_connection(ctx);
    let (group, conn) = join!(group_fut, conn_fut);

    // determine if a game is already running in this group. if yes, stop the game
    // before starting a new one.
    let maybe_active_race = get_maybe_active_race(&conn, &group);
    match maybe_active_race {
        Some(r) => stop_race(ctx, &r, &group).await?,
        None => (),
    };
    let game: BoxedGame = get_game_boxed(&args).await?;
    let new_race_data =
        NewAsyncRaceData::new_from_game(&game, &group.channel_group_id, this_race_type)?;
    insert_into(async_races)
        .values(&new_race_data)
        .execute(&conn)?;

    // we need to pull this back out for the race id
    let race_data: AsyncRaceData = async_races
        .filter(channel_group_id.eq(&group.channel_group_id))
        .filter(race_active.eq(true))
        .get_result(&conn)?;

    // use boxed game to build and post messages in submission and leaderboard channels
    // add both messages to messages table. rows in this table belong to async races.
    handle_new_race_messages(ctx, &group, &race_data).await?;

    Ok(())
}

async fn stop_race(
    ctx: &Context,
    race: &AsyncRaceData,
    group: &ChannelGroup,
) -> Result<(), BoxedError> {
    use crate::schema::async_races;
    let conn = get_connection(ctx).await;
    diesel::update(race)
        .set(async_races::race_active.eq(false))
        .execute(&conn)?;
    let leaderboard_msgs_data: Vec<BotMessage> = get_lb_msgs_data(&conn, race.race_id)?;
    if leaderboard_msgs_data.is_empty() {
        // this should never happen
        return Err(
            anyhow!("Tried to stop active game with no leaderboard messages in database").into(),
        );
    }
    for d in leaderboard_msgs_data.iter() {
        ctx.http.delete_message(d.channel_id, d.message_id).await?;
    }

    let lb_fut = build_leaderboard(ctx, group, race, ChannelType::Submission);
    let role_del_fut = remove_spoiler_roles(ctx, group, race);

    try_join!(lb_fut, role_del_fut)?;

    Ok(())
}

async fn remove_spoiler_roles(
    ctx: &Context,
    group: &ChannelGroup,
    race: &AsyncRaceData,
) -> Result<(), BoxedError> {
    // collect the user ids of everyone with a submission in this race
    // so we can use them to remove the spoiler role when the race has stopped
    use crate::schema::submissions::columns::*;

    let conn = get_connection(ctx).await;
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
            Err(e) => warn!("Error removing role for user id \"{}\": {}", id, e),
        };
    }

    Ok(())
}
