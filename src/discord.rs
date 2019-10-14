use std::{collections::HashMap, env};

use chrono::{offset::Utc, NaiveDateTime, NaiveTime};
use diesel::{
    mysql::MysqlConnection,
    r2d2::{ConnectionManager, Pool},
};

use serenity::{
    framework::standard::{
        macros::{command, group},
        Args, CommandResult,
    },
    model::{
        channel::Message,
        event::ResumedEvent,
        gateway::Ready,
        guild::PartialGuild,
        id::{ChannelId, RoleId},
    },
    prelude::*,
};

use crate::db::{
    clear_all_tables, create_game_entry, create_post_entry, create_submission_entry,
    get_active_games, get_leaderboard, get_leaderboard_ids, get_leaderboard_posts,
    get_submission_posts, OldSubmission, Post,
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
            info!(
                "Received message from {}: \"{}\"",
                &msg.author.name, &msg.content
            );
            match process_time_submission(&ctx, &msg) {
                Ok(()) => (),
                Err(e) => {
                    warn!("Error processing time submission: {}", e);
                }
            };

            msg.delete(&ctx)
                .unwrap_or_else(|e| info!("Error deleting message: {}", e));
            match update_leaderboard(&ctx, *guild_id.as_u64()) {
                Ok(()) => (),
                Err(e) => {
                    warn!("Error updating leaderboard: {}", e);
                    return;
                }
            };
        }
    }

    fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }

    fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }
}

#[command]
fn start(ctx: &mut Context, msg: &Message, args: Args) -> CommandResult {
    msg.delete(&ctx)?;
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

    refresh(ctx, &guild)?;

    // TODO: could parse/validate this better but this is good for now
    if args
        .rest()
        .split("/")
        .into_iter()
        .find(|x| x.to_string() == "alttpr.com")
        == None
    {
        return Ok(());
    }

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
            return Ok(());
        }
    };

    set_game_active(ctx, true);
    let data = ctx.data.read();
    let db_pool = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB connection in ShareMap.");

    create_game_entry(db_pool, *guild.id.as_u64(), &todays_date);
    create_post_entry(
        db_pool,
        post_id,
        current_time,
        *guild.id.as_u64(),
        *msg.channel_id.as_u64(),
    )?;
    initialize_leaderboard(ctx, db_pool, guild.id.as_u64(), &game_string)?;

    info!("Game successfully started");
    Ok(())
}

#[command]
fn stop(ctx: &mut Context, msg: &Message) -> CommandResult {
    msg.delete(&ctx)?;
    let guild = msg.guild_id.unwrap().to_partial_guild(&ctx.http).unwrap();
    let admin_role = get_admin_role(&guild)?;
    set_game_active(ctx, false);
    let data = ctx.data.read();
    let db_pool = data
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
        return Ok(());
    }
    // let active_games: Vec<Game> = get_active_games(connection)?;
    if get_active_games(db_pool)?.len() == 0 {
        info!("Stop command used with no active games");
        return Ok(());
    }

    let leaderboard_posts: Vec<Post> = get_leaderboard_posts(&leaderboard_channel, db_pool)?;
    let first_lb_post = ctx
        .http
        .get_message(leaderboard_channel, leaderboard_posts[0].post_id)?;
    let leaderboard_header = &first_lb_post.content.split("\n").collect::<Vec<&str>>()[0];
    //let submission_posts: Vec<Post> = get_submission_posts(&submission_channel, connection)?;
    let all_submissions: Vec<OldSubmission> = match get_leaderboard(db_pool) {
        Ok(leaderboard) => leaderboard,
        Err(e) => {
            warn!("Error getting leaderboard submissios from db: {}", e);
            return Ok(());
        }
    };
    let lb_string_allocation: usize = (&all_submissions.len() * 40) + 150;
    let mut leaderboard_string = String::with_capacity(lb_string_allocation);
    let mut runner_position: u32 = 1;
    leaderboard_string.push_str(format!("{}\n", leaderboard_header).as_str());
    all_submissions
        .iter()
        .filter(|&f| f.runner_forfeit == false)
        .for_each(|s| {
            leaderboard_string.push_str(
                format!(
                    "\n{}) {} - {} - {}/216",
                    runner_position, s.runner_name, s.runner_time, s.runner_collection
                )
                .as_str(),
            );
            runner_position += 1;
        });

    fill_leaderboard_refresh(
        ctx,
        db_pool,
        *guild.id.as_u64(),
        leaderboard_string,
        submission_channel,
    )?;

    for i in leaderboard_posts {
        let old_leaderboard_post: Message = ctx.http.get_message(leaderboard_channel, i.post_id)?;
        old_leaderboard_post.delete(&ctx)?;
    }

    let spoiler_role = get_spoiler_role(&guild)?;
    let leaderboard_ids = get_leaderboard_ids(db_pool)?;
    for id in leaderboard_ids {
        let member = &mut ctx.http.get_member(*guild.id.as_u64(), id)?;
        member.remove_role(&ctx, spoiler_role)?;
    }

    clear_all_tables(db_pool)?;

    info!("Game successfully stopped");
    Ok(())
}

fn refresh(ctx: &Context, guild: &PartialGuild) -> Result<(), BotError> {
    let data = ctx.data.read();
    let connection = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB connection in ShareMap.");

    let leaderboard_channel: u64 = *data
        .get::<ChannelsContainer>()
        .expect("No leaderboard channel in the environment")
        .get("leaderboard_channel")
        .unwrap()
        .as_u64();
    let submission_channel: u64 = *data
        .get::<ChannelsContainer>()
        .expect("No submission channel in the environment")
        .get("submission_channel")
        .unwrap()
        .as_u64();
    if get_active_games(connection)?.len() == 0 {
        return Ok(());
    }

    let leaderboard_posts: Vec<Post> = get_leaderboard_posts(&leaderboard_channel, connection)?;
    let first_lb_post = ctx
        .http
        .get_message(leaderboard_channel, leaderboard_posts[0].post_id)?;
    let leaderboard_header = &first_lb_post.content.split("\n").collect::<Vec<&str>>()[0];
    //let submission_posts: Vec<Post> = get_submission_posts(&submission_channel, connection)?;
    let all_submissions: Vec<OldSubmission> = match get_leaderboard(connection) {
        Ok(leaderboard) => leaderboard,
        Err(e) => {
            warn!("Error getting leaderboard submissios from db: {}", e);
            return Ok(());
        }
    };
    let lb_string_allocation: usize = (&all_submissions.len() * 40) + 150;
    let mut leaderboard_string = String::with_capacity(lb_string_allocation);
    let mut runner_position: u32 = 1;
    leaderboard_string.push_str(format!("{}\n", leaderboard_header).as_str());
    all_submissions
        .iter()
        .filter(|&f| f.runner_forfeit == false)
        .for_each(|s| {
            leaderboard_string.push_str(
                format!(
                    "\n{}) {} - {} - {}/216",
                    runner_position, s.runner_name, s.runner_time, s.runner_collection
                )
                .as_str(),
            );
            runner_position += 1;
        });

    fill_leaderboard_refresh(
        ctx,
        connection,
        *guild.id.as_u64(),
        leaderboard_string,
        submission_channel,
    )?;

    for i in leaderboard_posts {
        let old_leaderboard_post: Message = ctx.http.get_message(leaderboard_channel, i.post_id)?;
        old_leaderboard_post.delete(ctx)?;
    }

    let spoiler_role = get_spoiler_role(&guild)?;
    let leaderboard_ids = get_leaderboard_ids(connection)?;
    for id in leaderboard_ids {
        let member = &mut ctx.http.get_member(*guild.id.as_u64(), id)?;
        member.remove_role(ctx, spoiler_role)?;
    }

    clear_all_tables(connection)?;

    info!("Game successfully refreshed");
    Ok(())
}

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

pub fn get_channels_from_env() -> Result<HashMap<&'static str, ChannelId>, serenity::Error> {
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
    let db_pool = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB pool in ShareMap. Please check environment variables.");

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
            db_pool,
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
                "Processing submission: Incorrectly formatted time from {}: {}",
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
        db_pool,
        runner_name,
        *runner_id.as_u64(),
        submission_time,
        submission_collect,
        false,
    )?;

    info!(
        "Submission successfully accepted: {} {} {}",
        runner_name, submission_time, submission_collect
    );
    Ok(())
}

fn initialize_leaderboard(
    ctx: &Context,
    db_pool: &Pool<ConnectionManager<MysqlConnection>>,
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
    let leaderboard_string = format!("{} {}", "Leaderboard for", &game_string);
    let leaderboard_post_id = match leaderboard_channel.say(&ctx.http, leaderboard_string) {
        Ok(leaderboard_message) => *leaderboard_message.id.as_u64(),
        Err(e) => {
            warn!("Error initializing leaderboard: {}", e);
            return Ok(());
        }
    };

    create_post_entry(
        db_pool,
        leaderboard_post_id,
        current_time,
        *guild_id,
        *leaderboard_channel.as_u64(),
    )?;

    Ok(())
}

fn update_leaderboard(ctx: &Context, guild_id: u64) -> Result<(), BotError> {
    let data = ctx.data.read();
    let db_pool = data
        .get::<DBConnectionContainer>()
        .expect("Expected DB pool in ShareMap.");
    let leaderboard_channel: u64 = *data
        .get::<ChannelsContainer>()
        .expect("No submission channel in the environment")
        .get("leaderboard_channel")
        .unwrap()
        .as_u64();

    let all_submissions: Vec<OldSubmission> = match get_leaderboard(db_pool) {
        Ok(leaderboard) => leaderboard,
        Err(e) => {
            warn!("Error getting leaderboard submissios from db: {}", e);
            return Ok(());
        }
    };

    let leaderboard_posts: Vec<Post> = match get_leaderboard_posts(&leaderboard_channel, db_pool) {
        Ok(posts) => posts,
        Err(e) => {
            warn!("Error retrieving leaderboard post data from db: {}", e);
            return Ok(());
        }
    };

    let lb_string_allocation: usize = (&all_submissions.len() * 40) + 150;
    let mut leaderboard_string = String::with_capacity(lb_string_allocation);
    let first_post = match ctx
        .http
        .get_message(leaderboard_channel, leaderboard_posts[0].post_id)
    {
        Ok(post) => post,
        Err(e) => {
            warn!("Error retrieving leaderboard header: {}", e);
            return Ok(());
        }
    };
    let leaderboard_header = &first_post.content.split("\n").collect::<Vec<&str>>()[0];
    let mut runner_position: u32 = 1;
    leaderboard_string.push_str(format!("{}\n", leaderboard_header).as_str());
    all_submissions
        .iter()
        .filter(|&f| f.runner_forfeit == false)
        .for_each(|s| {
            leaderboard_string.push_str(
                format!(
                    "\n{}) {} - {} - {}/216",
                    runner_position, s.runner_name, s.runner_time, s.runner_collection
                )
                .as_str(),
            );
            runner_position += 1;
        });

    fill_leaderboard_update(
        ctx,
        db_pool,
        guild_id,
        leaderboard_string,
        leaderboard_posts,
        leaderboard_channel,
    )?;

    Ok(())
}

fn set_game_active(ctx: &mut Context, toggle: bool) {
    let mut data = ctx.data.write();
    *data
        .get_mut::<ActiveGames>()
        .expect("No active games toggle in ShareMap.") = toggle;
}

fn resize_leaderboard(
    ctx: &Context,
    db_pool: &Pool<ConnectionManager<MysqlConnection>>,
    guild_id: u64,
    leaderboard_channel: u64,
    new_posts: usize,
) -> Result<Vec<Post>, BotError> {
    // we need one more post than we have to hold all submissions
    for _n in 0..new_posts {
        let new_message: Message =
            ChannelId::from(leaderboard_channel).say(&ctx.http, "Placeholder")?;
        create_post_entry(
            db_pool,
            *new_message.id.as_u64(),
            Utc::now().naive_utc(),
            guild_id,
            *new_message.channel_id.as_u64(),
        )?;
    }
    let leaderboard_posts: Vec<Post> = get_leaderboard_posts(&leaderboard_channel, db_pool)?;

    Ok(leaderboard_posts)
}

fn fill_leaderboard_update(
    ctx: &Context,
    db_pool: &Pool<ConnectionManager<MysqlConnection>>,
    guild_id: u64,
    leaderboard_string: String,
    mut leaderboard_posts: Vec<Post>,
    channel: u64,
) -> Result<(), BotError> {
    let necessary_posts: usize = leaderboard_string.len() / 2000 + 1;

    if necessary_posts > leaderboard_posts.len() {
        let new_posts: usize = necessary_posts - leaderboard_posts.len();
        leaderboard_posts = match resize_leaderboard(ctx, db_pool, guild_id, channel, new_posts) {
            Ok(posts) => posts,
            Err(e) => {
                warn!("Error resizing leaderboard: {}", e);
                return Ok(());
            }
        };
    }

    // fill buffer then send the post until there's no more
    let mut post_buffer = String::with_capacity(2000);
    let mut post_iterator = leaderboard_posts.into_iter().peekable();
    let mut submission_iterator = leaderboard_string
        .split("\n")
        .collect::<Vec<&str>>()
        .into_iter()
        .peekable();

    loop {
        if post_iterator.peek().is_none() {
            warn!("Error: Ran out of space for leaderboard");
            break;
        }

        match submission_iterator.peek() {
            Some(line) => {
                if line.len() + &post_buffer.len() <= 2000 {
                    post_buffer
                        .push_str(format!("\n{}", submission_iterator.next().unwrap()).as_str())
                } else if line.len() + post_buffer.len() > 2000 {
                    let mut post = ctx
                        .http
                        .get_message(channel, post_iterator.next().unwrap().post_id)?;
                    post.edit(ctx, |x| x.content(&post_buffer))?;
                    post_buffer.clear();
                }
            }
            None => {
                let mut post = ctx
                    .http
                    .get_message(channel, post_iterator.next().unwrap().post_id)?;
                post.edit(ctx, |x| x.content(post_buffer))?;
                break;
            }
        };
    }

    Ok(())
}

fn fill_leaderboard_refresh(
    ctx: &Context,
    db_pool: &Pool<ConnectionManager<MysqlConnection>>,
    guild_id: u64,
    leaderboard_string: String,
    channel: u64,
) -> Result<(), BotError> {
    let necessary_posts: usize = leaderboard_string.len() / 2000 + 1;
    if necessary_posts > 1 {
        for _n in 1..necessary_posts {
            let new_message = ChannelId::from(channel).say(&ctx.http, "Placeholder")?;
            create_post_entry(
                db_pool,
                *new_message.id.as_u64(),
                Utc::now().naive_utc(),
                guild_id,
                *new_message.channel_id.as_u64(),
            )?;
        }
    }
    let submission_posts = get_submission_posts(&channel, db_pool)?;
    // fill buffer then send the post until there's no more
    let mut post_buffer = String::with_capacity(2000);
    let mut post_iterator = submission_posts.into_iter().peekable();
    let mut submission_iterator = leaderboard_string
        .split("\n")
        .collect::<Vec<&str>>()
        .into_iter()
        .peekable();

    loop {
        if post_iterator.peek().is_none() {
            warn!("Error: Ran out of space moving leaderboard to submission channel");
            break;
        }

        match submission_iterator.peek() {
            Some(line) => {
                if line.len() + &post_buffer.len() <= 2000 {
                    post_buffer
                        .push_str(format!("\n{}", submission_iterator.next().unwrap()).as_str())
                } else if line.len() + post_buffer.len() > 2000 {
                    let mut post = ctx
                        .http
                        .get_message(channel, post_iterator.next().unwrap().post_id)?;
                    post.edit(ctx, |x| x.content(&post_buffer))?;
                    post_buffer.clear();
                }
            }
            None => {
                let mut post = ctx
                    .http
                    .get_message(channel, post_iterator.next().unwrap().post_id)?;
                post.edit(ctx, |x| x.content(post_buffer))?;
                break;
            }
        };
    }

    Ok(())
}

group!({
    name: "admin",
    options: {},
    commands: [start, stop]
});

pub struct DBConnectionContainer;

impl TypeMapKey for DBConnectionContainer {
    type Value = Pool<ConnectionManager<MysqlConnection>>;
}

pub struct ChannelsContainer;

impl TypeMapKey for ChannelsContainer {
    type Value = HashMap<&'static str, ChannelId>;
}

pub struct ActiveGames;

impl TypeMapKey for ActiveGames {
    type Value = bool;
}
