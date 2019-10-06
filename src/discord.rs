use std::{collections::HashMap, env};

use chrono::{offset::Utc, NaiveDateTime, NaiveTime};
use diesel::mysql::MysqlConnection;

use serenity::{
    framework::standard::{
        macros::{command, group},
        Args, CommandResult,
    },
    model::{
        channel::Message,
        gateway::Ready,
        guild::PartialGuild,
        id::{ChannelId, RoleId},
    },
    prelude::*,
};

use crate::db::{
    clear_all_tables, create_game_entry, create_post_entry, create_submission_entry,
    get_active_games, get_leaderboard, get_leaderboard_ids, get_leaderboard_posts,
    get_submission_posts, Game, Post,
};
use crate::error::{BotError, RoleError, SubmissionError};
use crate::z3r;

pub struct Handler;

impl EventHandler for Handler {
    fn message(&self, ctx: Context, msg: Message) {
        let data = ctx.data.read();
        let game_active: bool = *data
            .get::<ActiveGames>()
            .expect("No active game toggle set");
        let guild_id = match msg.guild_id {
            Some(guild_id) => guild_id,
            None => return,
        };
        let guild = match guild_id.to_partial_guild(&ctx.http) {
            Ok(guild) => guild,
            Err(e) => {
                warn!("Couldn't get partial guild from REST API. ERROR: {}", e);
                return;
            }
        };

        let admin_role = match get_admin_role(&guild) {
            Ok(admin_role) => admin_role,
            Err(e) => {
                warn!("Couldn't get admin role. Check if properly set in environment variables. ERROR: {}", e);
                return;
            }
        };

        if msg.author.id != ctx.cache.read().user.id
        && game_active
        && msg.channel_id.as_u64()
            == match &env::var("SUBMISSION_CHANNEL_ID")
                .expect("No submissions channel in the environment")
                .parse::<u64>() {
                    Ok(channel_u64) => channel_u64,
                    Err(e) => {
                        warn!("Error parsing channel id: {}", e);
                        return;
                    }
        }
        // TODO: refactor this
        && (msg
            .member
            .as_ref()
            .unwrap()
            .roles
            .iter()
            .find(|x| x.as_u64() == admin_role.as_u64()) == None ||
            [msg.content.as_str().bytes().nth(0).unwrap()] != "!".as_bytes()
            )
        {
            match process_time_submission(&ctx, &msg) {
                Ok(()) => (),
                Err(e) => {
                    warn!("Error processing time submission: {}", e);
                    return;
                }
            };

            msg.delete(&ctx).unwrap();
            match update_leaderboard(&ctx) {
                Ok(()) => (),
                Err(e) => {
                    warn!("Error updating leaderboard: {}", e);
                    return;
                }
            }
        };
    }

    fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }
}

#[command]
fn start(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    let todays_date = Utc::today().naive_utc();
    let current_time: NaiveDateTime = Utc::now().naive_utc();
    let guild = msg.guild_id.unwrap().to_partial_guild(&ctx.http).unwrap();
    let admin_role = get_admin_role(&guild)?;
    if msg.channel_id.as_u64()
        != &env::var("SUBMISSION_CHANNEL_ID")
            .expect("No submissions channel in the environment")
            .parse::<u64>()?
    {
        return Ok(());
    }

    // check for admin role, validate url, and maybe start the game
    if msg
        .member
        .as_ref()
        .unwrap()
        .roles
        .iter()
        .find(|x| x.as_u64() == admin_role.as_u64())
        == None
    {
        msg.delete(&ctx)?;
        return Ok(());
    }
    //refresh(&ctx, msg)?;

    // TODO: could parse/validate this better but this is good for now
    if args
        .rest()
        .split("/")
        .into_iter()
        .find(|x| x.to_string() == "alttpr.com")
        == None
    {
        msg.delete(&ctx)?;
        return Ok(());
    }

    msg.delete(&ctx)?;
    let game_hash: &str = args
        .rest()
        .split("/")
        .collect::<Vec<&str>>()
        .last()
        .unwrap();
    let game_string = z3r::get_game_string(game_hash, args.rest(), &todays_date)?;
    let post_id_result = msg.channel_id.say(&ctx.http, &game_string);

    let post_id: u64 = match post_id_result {
        Ok(post_id_result) => *post_id_result.id.as_u64(),
        Err(_post_id_result) => {
            msg.delete(&ctx)?;
            return Ok(());
        }
    };

    set_game_active(ctx, true);
    let data = ctx.data.read();
    let connection = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB connection in ShareMap.");

    create_game_entry(connection, *guild.id.as_u64(), &todays_date);
    create_post_entry(
        connection,
        post_id,
        current_time,
        *guild.id.as_u64(),
        *msg.channel_id.as_u64(),
    )?;
    initialize_leaderboard(ctx, connection, guild.id.as_u64(), &game_string)?;
    msg.delete(&ctx)?;
    Ok(())
}

#[command]
fn stop(ctx: &mut Context, msg: &Message) -> CommandResult {
    let guild = msg.guild_id.unwrap().to_partial_guild(&ctx.http).unwrap();
    let admin_role = get_admin_role(&guild)?;
    set_game_active(ctx, false);
    let data = ctx.data.read();
    let connection = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB connection in ShareMap.");
    let leaderboard_channel: u64 = *data
        .get::<ChannelsContainer>()
        .expect("No submission channel in the environment")
        .get("leaderboard_channel")
        .unwrap()
        .as_u64();
    let submission_channel: u64 = *data
        .get::<ChannelsContainer>()
        .expect("No submission channel in the environment")
        .get("submission_channel")
        .unwrap()
        .as_u64();

    if *msg.channel_id.as_u64() != submission_channel {
        msg.delete(&ctx)?;
        return Ok(());
    }
    if msg
        .member
        .as_ref()
        .unwrap()
        .roles
        .iter()
        .find(|x| x.as_u64() == admin_role.as_u64())
        == None
    {
        msg.delete(&ctx)?;
        return Ok(());
    }
    let active_games: Vec<Game> = get_active_games(connection)?;
    if active_games.len() >= 1 {
        let mut moved_leaderboard = String::with_capacity(2000);
        let mut submission_posts: Vec<Post> =
            get_submission_posts(&submission_channel, connection)?;
        submission_posts.sort_by(|a, b| b.post_datetime.cmp(&a.post_datetime).reverse());
        let leaderboard_posts: Vec<u64> = get_leaderboard_posts(&leaderboard_channel, connection)?;
        let mut most_recent_submission_post: Message = ctx
            .http
            .get_message(submission_channel, submission_posts[0].post_id)?;

        for i in leaderboard_posts {
            let old_leaderboard_post: Message = ctx.http.get_message(leaderboard_channel, i)?;
            moved_leaderboard.push_str(&old_leaderboard_post.content);
            old_leaderboard_post.delete(&ctx)?;
        }

        most_recent_submission_post.edit(&ctx, |x| x.content(moved_leaderboard))?;
        // delete all posts in leaderboard channel
        // add leaderboard to latest post in submission channel
        // remove spoiler role from everyone who has it
        // delete all db tables

        let spoiler_role = get_spoiler_role(&guild)?;
        let leaderboard_ids = get_leaderboard_ids(connection)?;
        for id in leaderboard_ids {
            let member = &mut ctx.http.get_member(*guild.id.as_u64(), id)?;
            member.remove_role(&ctx, spoiler_role)?;
        }
    } else {
        return Ok(());
    }

    clear_all_tables(connection)?;
    msg.delete(&ctx)?;
    Ok(())
}

//fn refresh(ctx: &Context, msg: &Message) -> CommandResult {
//    let _guild = msg.guild_id.unwrap().to_partial_guild(&ctx.http).unwrap();
//    let data = ctx.data.read();
//    let connection = data
//        .get::<DBConnectionContainer>()
//        .expect("Expected DB connection in ShareMap.");
//    let leaderboard_channel: u64 = *data
//        .get::<ChannelsContainer>()
//        .expect("No submission channel in the environment")
//        .get("leaderboard_channel")
//        .unwrap()
//        .as_u64();
//    let submission_channel: u64 = *data
//        .get::<ChannelsContainer>()
//        .expect("No submission channel in the environment")
//        .get("submission_channel")
//        .unwrap()
//        .as_u64();
//    let active_games: Vec<Game> = get_active_games(connection)?;
//    if active_games.len() >= 1 {
//        let mut moved_leaderboard = String::with_capacity(2000);
//        let mut submission_posts: Vec<Post> = get_submission_posts(&submission_channel, connection);
//        submission_posts.sort_by(|a, b| b.post_time.cmp(&a.post_time));
//        let leaderboard_posts: Vec<u64> = get_leaderboard_posts(&leaderboard_channel, connection);
//        let mut most_recent_submission_post: Message = ctx
//            .http
//            .get_message(submission_channel, submission_posts[0].post_id)?;
//
//        for i in leaderboard_posts {
//            let old_leaderboard_post: Message = ctx.http.get_message(leaderboard_channel, i)?;
//            moved_leaderboard.push_str(&old_leaderboard_post.content);
//            old_leaderboard_post.delete(ctx)?;
//        }
//
//        most_recent_submission_post.edit(ctx, |x| x.content(moved_leaderboard))?;
//        // delete all posts in leaderboard channel
//        // add leaderboard to latest post in submission channel
//        // remove spoiler role from everyone who has it
//        // delete all db tables
//    }
//
//    Ok(())
//}
fn get_admin_role(guild: &PartialGuild) -> Result<RoleId, RoleError> {
    let admin_role = guild.role_by_name(
        env::var("DISCORD_ADMIN_ROLE")
            .expect("No bot admin role in environment.")
            .as_str(),
    );
    Ok(admin_role.unwrap().id)
}

fn get_spoiler_role(guild: &PartialGuild) -> Result<RoleId, RoleError> {
    let spoiler_role = guild.role_by_name(
        env::var("DISCORD_SPOILER_ROLE")
            .expect("No spoiler role in environment.")
            .as_str(),
    );
    Ok(spoiler_role.unwrap().id)
}

pub fn get_channels() -> Result<HashMap<&'static str, ChannelId>, serenity::Error> {
    let mut bot_channels: HashMap<&'static str, ChannelId> = HashMap::with_capacity(3);
    bot_channels.insert(
        "submission_channel",
        ChannelId::from(
            env::var("SUBMISSION_CHANNEL_ID")
                .expect("No submission channel in environment ")
                .parse::<u64>()
                .unwrap(),
        ),
    );
    bot_channels.insert(
        "leaderboard_channel",
        ChannelId::from(
            env::var("LEADERBOARD_CHANNEL_ID")
                .expect("No leaderboard channel in environment")
                .parse::<u64>()
                .unwrap(),
        ),
    );
    bot_channels.insert(
        "spoiler_channel",
        ChannelId::from(
            env::var("SPOILER_CHANNEL_ID")
                .expect("No spoiler channel in environment")
                .parse::<u64>()
                .unwrap(),
        ),
    );

    Ok(bot_channels)
}

fn process_time_submission(ctx: &Context, msg: &Message) -> Result<(), SubmissionError> {
    let guild = msg.guild_id.unwrap().to_partial_guild(&ctx.http).unwrap();
    let runner_id = msg.author.id;
    let runner_name = msg.author.name.as_str();
    let spoiler_role = match get_spoiler_role(&guild) {
        Ok(role) => role,
        Err(e) => {
            warn!(
                "Processing submission: Couldn't get spoiler role from REST API: {}",
                e
            );
            return Ok(());
        }
    };

    let mut maybe_submission: Vec<&str> =
        msg.content.as_str().trim_end().split_whitespace().collect();

    let data = ctx.data.read();
    let connection = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB connection in ShareMap. Please check environment variables.");

    let ff = vec!["ff", "FF", "forfeit", "Forfeit"];
    if ff.iter().any(|&x| x == maybe_submission[0]) {
        let mut current_member = msg.member(ctx).unwrap();
        match current_member.add_role(ctx, spoiler_role) {
            Ok(()) => (),
            Err(e) => {
                warn!("Processing submission: Error adding role: {}", e);
                return Ok(());
            }
        };
        create_submission_entry(
            connection,
            runner_name,
            *runner_id.as_u64(),
            NaiveTime::from_hms(0, 0, 0),
            0,
            true,
        )?;
        return Ok(());
    }
    if maybe_submission.len() != 2 {
        return Ok(());
    }

    let maybe_time: &str = &maybe_submission.remove(0).replace("\\", "");
    let submission_time = match NaiveTime::parse_from_str(&maybe_time, "%H:%M:%S") {
        Ok(submission_time) => submission_time,
        Err(_e) => {
            info!(
                "Processing submission: Incorrectly formatted time from {} : {}",
                &msg.author.name, &maybe_time
            );
            return Ok(());
        }
    };

    let maybe_collect: &str = maybe_submission.remove(0);
    let submission_collect = match maybe_collect.parse::<u8>() {
        Ok(submission_collect) => submission_collect,
        Err(_e) => {
            info!(
                "Processing submission: Collection rate couldn't be parsed into 8-bit integer: {} : {}",
                &msg.author.name, &maybe_collect
            );
            return Ok(());
        }
    };

    let mut current_member = match msg.member(ctx) {
        Some(member) => member,
        None => {
            warn!(
                "Processing submission: Error retrieving Member data from API for {}",
                &msg.author.name
            );
            return Ok(());
        }
    };

    match current_member.add_role(ctx, spoiler_role) {
        Ok(()) => (),
        Err(e) => {
            warn!(
                "Processing submission: Couldn't add spoiler role to {}. Error: {}",
                &msg.author.name, e
            );
            return Ok(());
        }
    };

    create_submission_entry(
        connection,
        runner_name,
        *runner_id.as_u64(),
        submission_time,
        submission_collect,
        false,
    )?;
    Ok(())
}

fn initialize_leaderboard(
    ctx: &Context,
    db_mutex: &Mutex<MysqlConnection>,
    guild_id: &u64,
    game_string: &str,
) -> Result<(), BotError> {
    let current_time: NaiveDateTime = Utc::now().naive_utc();
    let data = ctx.data.read();
    let leaderboard_channel: ChannelId = *data
        .get::<ChannelsContainer>()
        .expect("No submission channel in the environment")
        .get("leaderboard_channel")
        .unwrap();
    let leaderboard_string = format!("{} {}", "Leaderboard for", game_string);
    let leaderboard_post_id = match leaderboard_channel.say(&ctx.http, leaderboard_string) {
        Ok(leaderboard_message) => *leaderboard_message.id.as_u64(),
        Err(e) => {
            warn!("Error initializing leaderboard: {}", e);
            return Ok(());
        }
    };

    create_post_entry(
        db_mutex,
        leaderboard_post_id,
        current_time,
        *guild_id,
        *leaderboard_channel.as_u64(),
    )?;

    Ok(())
}

fn update_leaderboard(ctx: &Context) -> Result<(), BotError> {
    let data = ctx.data.read();
    let connection = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB connection in ShareMap.");
    let leaderboard_channel: u64 = *data
        .get::<ChannelsContainer>()
        .expect("No submission channel in the environment")
        .get("leaderboard_channel")
        .unwrap()
        .as_u64();

    let mut all_submissions = get_leaderboard(connection)?;
    let leaderboard_posts = get_leaderboard_posts(&leaderboard_channel, connection)?;
    all_submissions.sort_by(|a, b| b.runner_time.cmp(&a.runner_time).reverse());
    if all_submissions.len() <= 50 {
        let mut post = ctx
            .http
            .get_message(leaderboard_channel, leaderboard_posts[0])
            .unwrap();
        let mut edit_string = String::new();
        let mut runner_position: u32 = 0;
        let leaderboard_header = &post.content.split("\n").collect::<Vec<&str>>()[0];
        edit_string.push_str(leaderboard_header);
        for i in all_submissions {
            if i.runner_forfeit != true {
                runner_position += 1;
                edit_string.push_str(
                    format!(
                        "\n{}) {} - {} - {}/216",
                        runner_position, i.runner_name, i.runner_time, i.runner_collection
                    )
                    .as_str(),
                )
            }
        }

        post.edit(ctx, |x| x.content(edit_string)).unwrap();
    }

    Ok(())
}

fn set_game_active(ctx: &mut Context, toggle: bool) {
    let mut data = ctx.data.write();
    *data
        .get_mut::<ActiveGames>()
        .expect("No active games toggle in context.") = toggle;
}

group!({
    name: "admin",
    options: {},
    commands: [start, stop]
});

pub struct DBConnectionContainer;

impl TypeMapKey for DBConnectionContainer {
    type Value = Mutex<MysqlConnection>;
}

pub struct ChannelsContainer;

impl TypeMapKey for ChannelsContainer {
    type Value = HashMap<&'static str, ChannelId>;
}

pub struct ActiveGames;

impl TypeMapKey for ActiveGames {
    type Value = bool;
}
