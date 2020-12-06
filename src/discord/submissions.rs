use anyhow::Result;
use chrono::{NaiveDateTime, NaiveTime, Utc};
use diesel::prelude::*;
use serenity::{
    client::Context,
    model::{channel::Message, id::RoleId},
};

use crate::{
    discord::channel_groups::ChannelGroup,
    games::{AsyncRaceData, GameName, RaceType},
    helpers::*,
    schema::*,
};

// list of games we've implemented parsing IGT from a file (SRAM) for
const IGT_GAMES: [GameName; 1] = [GameName::ALTTPR];

// some strings we'll compare with to check if a user has forfeited
const FORFEIT: [&'static str; 4] = ["ff", "FF", "forfeit", "Forfeit"];

#[derive(Debug, Insertable, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "AsyncRaceData", foreign_key = "race_id")]
#[table_name = "submissions"]
#[primary_key(submission_id)]
pub struct Submission {
    pub submission_id: Option<u32>,
    pub runner_id: u64,
    pub race_id: u32,
    pub submission_datetime: NaiveDateTime,
    pub runner_name: String,
    pub runner_time: Option<NaiveTime>,
    pub runner_collection: Option<u16>,
    pub option_number: Option<u32>,
    pub option_text: Option<String>,
    pub runner_forfeit: bool,
}

pub async fn process_submission(
    ctx: &Context,
    msg: &Message,
    group: &ChannelGroup,
    race: &AsyncRaceData,
) -> Result<()> {
    // how we process this depends on game, IGT or RTA, and whether or not we have
    // an attached save file we can parse for IGT. The purpose of this function is
    // only to process then add a good submission to the database.
    let conn = get_connection(&ctx).await;
    let mut maybe_submission: Vec<&str> =
        msg.content.as_str().trim_end().split_whitespace().collect();
    // first check to see if the user has forfeited
    if FORFEIT.iter().any(|&x| x == maybe_submission[0]) {
        process_forfeit(&ctx, &msg, &group, &race).await?;
        return Ok(());
    }
    // if we have an attachment, an IGT game, and a game that we can read the save file
    // of, we can try to do that
    if race.race_type == RaceType::IGT
        && IGT_GAMES.iter().any(|&g| g == race.race_game)
        && msg.attachments.len() == 1
    {
        // this can fail so if someone attaches a save let's assume they're not also
        // writing their time etc and return if it fails
        process_sram(&ctx, &msg, &group, &race).await?;
        return Ok(());
    }
    todo!();
}

async fn process_forfeit(
    ctx: &Context,
    msg: &Message,
    group: &ChannelGroup,
    race: &AsyncRaceData,
) -> Result<()> {
    use crate::schema::submissions::columns::*;
    use crate::schema::submissions::dsl::*;

    let submission = Submission {
        submission_id: None,
        runner_id: *msg.author.id.as_u64(),
        race_id: race.race_id,
        submission_datetime: Utc::now().naive_utc(),
        runner_name: msg.author.name.clone(),
        runner_time: None,
        runner_collection: None,
        option_number: None,
        option_text: None,
        runner_forfeit: true,
    };
    let conn = get_connection(&ctx).await;
    diesel::insert_into(submissions)
        .values(&submission)
        .execute(&conn)?;
    let mut member = msg.member(&ctx).await?;
    let _ = member.add_role(&ctx, group.spoiler_role_id).await?;
    info!(
        "Successfully processed submission for user \"{}\"",
        &msg.author.name
    );

    Ok(())
}

async fn process_sram(
    ctx: &Context,
    msg: &Message,
    group: &ChannelGroup,
    race: &AsyncRaceData,
) -> Result<()> {
    use crate::schema::submissions::columns::*;
    use crate::schema::submissions::dsl::*;

    todo!();
}

// fn process_time_submission(ctx: &Context, msg: &Message) -> Result<(), SubmissionError> {
//     let guild_id = match msg.guild_id {
//         Some(id) => id,
//         None => {
//             let err_msg = format!("Error unwrapping guild id from Message");
//             return Err(SubmissionError::new(&err_msg));
//         }
//     };
//     let guild = guild_id.to_partial_guild(&ctx.http)?;
//     let runner_id = msg.author.id;
//     let runner_name = msg.author.name.as_str();
//     let spoiler_role = match get_spoiler_role(&guild) {
//         Ok(role) => role,
//         Err(e) => {
//             let err_msg: String = format!(
//                 "Submission Error: Couldn't get spoiler role from REST API: {}",
//                 e
//             );
//             return Err(SubmissionError::new(&err_msg));
//         }
//     };
//
//     let mut maybe_submission: Vec<&str> =
//         msg.content.as_str().trim_end().split_whitespace().collect();
//
//     let data = ctx.data.read();
//     let db_pool = data
//         .get::<DBConnectionContainer>()
//         .expect("Expected DB pool in ShareMap. Please check environment variables.");
//
//     let ff = vec!["ff", "FF", "forfeit", "Forfeit"];
//     if ff.iter().any(|&x| x == maybe_submission[0]) {
//         info!("User forfeited: {}", &msg.author.name);
//         let mut current_member = match msg.member(ctx) {
//             Some(member) => member,
//             None => {
//                 let err_string: String =
//                     format!("Error getting PartialMember data from {}", &msg.author.name);
//                 return Err(SubmissionError::new(&err_string));
//             }
//         };
//         match current_member.add_role(ctx, spoiler_role) {
//             Ok(()) => (),
//             Err(e) => {
//                 warn!("Processing submission: Error adding role: {}", e);
//                 return Ok(());
//             }
//         };
//         create_submission_entry(
//             db_pool,
//             runner_name,
//             *runner_id.as_u64(),
//             NaiveTime::from_hms(0, 0, 0),
//             0,
//             true,
//         )?;
//         return Ok(());
//     }
//     if maybe_submission.len() != 2 {
//         return Ok(());
//     }
//
//     let maybe_time: &str = &maybe_submission.remove(0).replace("\\", "");
//     let submission_time = match NaiveTime::parse_from_str(&maybe_time, "%H:%M:%S") {
//         Ok(submission_time) => submission_time,
//         Err(_e) => {
//             info!(
//                 "Processing submission: Incorrectly formatted time from {}: {}",
//                 &msg.author.name, &maybe_time
//             );
//             return Ok(());
//         }
//     };
//
//     let maybe_collect: &str = maybe_submission.remove(0);
//     let submission_collect = match maybe_collect.parse::<u16>() {
//         Ok(submission_collect) => submission_collect,
//         Err(_e) => {
//             info!(
//                 "Processing submission: Collection rate couldn't be parsed into 8-bit integer: {} : {}",
//                 &msg.author.name, &maybe_collect
//             );
//             return Ok(());
//         }
//     };
//
//     let mut current_member = match msg.member(ctx) {
//         Some(member) => member,
//         None => {
//             info!(
//                 "Failed retrieving member data from server message. Falling back to http request."
//             );
//             match ctx
//                 .http
//                 .get_member(u64::from(guild_id), u64::from(msg.author.id))
//             {
//                 Ok(member) => member,
//                 Err(e) => {
//                     warn!("Error getting member data via http request: {}", e);
//                     return Ok(());
//                 }
//             }
//         }
//     };
//
//     match current_member.add_role(ctx, spoiler_role) {
//         Ok(()) => (),
//         Err(e) => {
//             warn!(
//                 "Processing submission: Couldn't add spoiler role to {}. Error: {}",
//                 &msg.author.name, e
//             );
//             return Ok(());
//         }
//     };
//
//     create_submission_entry(
//         db_pool,
//         runner_name,
//         *runner_id.as_u64(),
//         submission_time,
//         submission_collect,
//         false,
//     )?;
//
//     info!(
//         "Submission successfully accepted: {} {} {}",
//         runner_name, submission_time, submission_collect
//     );
//     Ok(())
// }
//
// fn update_leaderboard(ctx: &Context, guild_id: u64) -> Result<()> {
//     let current_time: NaiveDateTime = Utc::now().naive_utc();
//     let data = ctx.data.read();
//     let db_pool = data
//         .get::<DBConnectionContainer>()
//         .expect("Expected DB pool in ShareMap.");
//     let leaderboard_channel: u64 = *data
//         .get::<ChannelContainer>()
//         .expect("No submission channel in the environment")
//         .get("leaderboard_channel")
//         .unwrap()
//         .as_u64();
//
//     let all_submissions: Vec<OldSubmission> = match get_leaderboard(db_pool) {
//         Ok(leaderboard) => leaderboard,
//         Err(e) => {
//             warn!("Error getting leaderboard submissios from db: {}", e);
//             return Ok(());
//         }
//     };
//
//     let leaderboard_posts: Vec<Post> = match get_leaderboard_posts(&leaderboard_channel, db_pool) {
//         Ok(posts) => posts,
//         Err(e) => {
//             warn!("Error retrieving leaderboard post data from db: {}", e);
//             return Ok(());
//         }
//     };
//
//     let lb_string_allocation: usize = (&all_submissions.len() * 40) + 150;
//     let mut leaderboard_string = String::with_capacity(lb_string_allocation);
//     let first_post = match ctx
//         .http
//         .get_message(leaderboard_channel, leaderboard_posts[0].post_id)
//     {
//         Ok(post) => post,
//         Err(e) => {
//             warn!("Error retrieving leaderboard header: {}", e);
//             return Ok(());
//         }
//     };
//     let leaderboard_header = &first_post.content.split("\n").collect::<Vec<&str>>()[0];
//     let mut runner_position: u32 = 1;
//     leaderboard_string.push_str(format!("{}\n", leaderboard_header).as_str());
//     all_submissions
//         .iter()
//         .filter(|&f| f.runner_forfeit == false)
//         .for_each(|s| {
//             if &current_time.timestamp() - s.submission_datetime.timestamp() > 21600 {
//                 leaderboard_string.push_str(
//                     format!(
//                         "\n{}) {} - {} - {}/216",
//                         runner_position, s.runner_name, s.runner_time, s.runner_collection
//                     )
//                     .as_str(),
//                 );
//                 runner_position += 1;
//             } else {
//                 leaderboard_string.push_str(
//                     format!(
//                         "\n{}) *{}* - {} - {}/216",
//                         runner_position, s.runner_name, s.runner_time, s.runner_collection
//                     )
//                     .as_str(),
//                 );
//                 runner_position += 1;
//             }
//         });
//
//     fill_leaderboard_update(
//         ctx,
//         db_pool,
//         guild_id,
//         leaderboard_string,
//         leaderboard_posts,
//         leaderboard_channel,
//     )?;
//
//     Ok(())
// }
