use std::{default::Default, fmt};

use anyhow::{anyhow, Result};
use chrono::{Duration, NaiveDateTime, NaiveTime, Utc};
use diesel::prelude::*;
use serenity::{
    client::Context,
    model::{channel::Message, id::ChannelId},
};

use crate::{
    discord::{
        channel_groups::{ChannelGroup, ChannelType},
        messages::BotMessage,
    },
    games::{smtotal, smvaria, smz3, z3r, AsyncRaceData, DataDisplay, GameName},
    helpers::*,
    schema::*,
};

// some strings we'll compare with to check if a user has forfeited
const FORFEIT: [&'static str; 4] = ["ff", "FF", "forfeit", "Forfeit"];

#[derive(Debug, Insertable, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "AsyncRaceData", foreign_key = "race_id")]
#[table_name = "submissions"]
#[primary_key(submission_id)]
pub struct Submission {
    pub submission_id: u32,
    pub runner_id: u64,
    pub race_id: u32,
    pub race_game: GameName,
    pub submission_datetime: NaiveDateTime,
    pub runner_name: String,
    pub runner_time: Option<NaiveTime>,
    pub runner_collection: Option<u16>,
    pub option_number: Option<u32>,
    pub option_text: Option<String>,
    pub runner_forfeit: bool,
}

impl fmt::Display for Submission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.race_game {
            GameName::ALTTPR => write!(
                f,
                "{} - {} - {}/216",
                self.runner_name,
                self.runner_time.unwrap(),
                self.runner_collection.unwrap()
            ),
            GameName::SMZ3 => write!(
                f,
                "{} - {} - {}/316",
                self.runner_name,
                self.runner_time.unwrap(),
                self.runner_collection.unwrap()
            ),
            GameName::FF4FE => write!(f, "{} - {}", self.runner_name, self.runner_time.unwrap()),
            GameName::SMVARIA => write!(
                f,
                "{} - {} - {}%",
                self.runner_name,
                self.runner_time.unwrap(),
                self.runner_collection.unwrap()
            ),
            GameName::SMTotal => write!(
                f,
                "{} - {} - {}%",
                self.runner_name,
                self.runner_time.unwrap(),
                self.runner_collection.unwrap()
            ),
            GameName::Other => write!(f, "{} - {}", self.runner_name, self.runner_time.unwrap()),
        }
    }
}

#[derive(Debug, Clone, Insertable)]
#[table_name = "submissions"]
pub struct NewSubmission {
    pub runner_id: u64,
    pub race_id: u32,
    pub race_game: GameName,
    pub submission_datetime: NaiveDateTime,
    pub runner_name: String,
    pub runner_time: Option<NaiveTime>,
    pub runner_collection: Option<u16>,
    // we will put some optional fields here just in case a future module uses them
    // or somebody wants to extend an existing game with, say, a bonk counter
    pub option_number: Option<u32>,
    pub option_text: Option<String>,
    pub runner_forfeit: bool,
}

impl NewSubmission {
    fn set_runner_id<T: Into<u64>>(&mut self, id: T) -> &mut Self {
        self.runner_id = id.into();

        self
    }

    fn set_race_id<T: Into<u32>>(&mut self, id: T) -> &mut Self {
        self.race_id = id.into();

        self
    }

    fn name<T: Into<String>>(&mut self, name: T) -> &mut Self {
        self.runner_name = name.into();

        self
    }

    fn set_time(&mut self, time: Option<NaiveTime>) -> &mut Self {
        self.runner_time = time;

        self
    }

    pub fn set_collection<T: Into<u16>>(&mut self, cr: Option<T>) -> &mut Self {
        self.runner_collection = match cr {
            Some(cr) => Some(cr.into()),
            None => None,
        };

        self
    }

    pub fn set_optional_number<T: Into<u32>>(&mut self, number: Option<T>) -> &mut Self {
        self.option_number = match number {
            Some(n) => Some(n.into()),
            None => None,
        };

        self
    }

    pub fn set_optional_text<T: Into<String>>(&mut self, text: Option<T>) -> &mut Self {
        self.option_text = match text {
            Some(t) => Some(t.into()),
            None => None,
        };

        self
    }

    pub fn set_game_info(
        &mut self,
        game: GameName,
        submission_msg: &Vec<&str>,
    ) -> Result<Self, BoxedError> {
        // pass this off to a game-specific function defined in a game's module
        // this can fail if the message does not have correct amount or type of args
        // also we should be preventing a game that's not implemented from starting
        // well up the stack but in the interest of avoiding panics let's return a result
        // with a non-mutable cloned Self since this will be the final building method

        // i feel like there is a more elegant way to do this but this works for now

        self.race_game = game;
        match game {
            GameName::ALTTPR => Ok(z3r::game_info(self, submission_msg)?.clone()),
            GameName::SMZ3 => Ok(smz3::game_info(self, submission_msg)?.clone()),
            GameName::SMTotal => Ok(smtotal::game_info(self, submission_msg)?.clone()),
            GameName::SMVARIA => Ok(smvaria::game_info(self, submission_msg)?.clone()),
            GameName::Other => Ok(self.clone()),
            _ => Err(anyhow!("Game not yet implemented").into()),
        }
    }
}

impl Default for NewSubmission {
    fn default() -> Self {
        NewSubmission {
            runner_id: 0u64,
            race_id: 0u32,
            race_game: GameName::Other,
            submission_datetime: Utc::now().naive_utc(),
            runner_name: String::new(),
            runner_time: None,
            runner_collection: None,
            option_number: None,
            option_text: None,
            runner_forfeit: false,
        }
    }
}

pub async fn process_submission(
    ctx: &Context,
    msg: &Message,
    race: &AsyncRaceData,
) -> Result<(), BoxedError> {
    use crate::schema::submissions::dsl::*;

    // in some cases this will return Ok despite not successfully inserting a submission
    // ie when a submission is malformed. the submitter is expected to know and recognize
    // that the submission was malformed when their message is deleted and they dont
    // have access to the leaderboard and spoilers channel
    let conn = get_connection(&ctx).await;
    let mut maybe_submission_text: Vec<&str> =
        msg.content.as_str().trim_end().split_whitespace().collect();
    if !(maybe_submission_text.len() >= 1) {
        return Ok(());
    }
    // first check to see if the user has forfeited
    // the length check here should short circuit so we don't have to worry
    // about panicking if there's no text
    if maybe_submission_text.len() >= 1 && FORFEIT.iter().any(|&x| x == maybe_submission_text[0]) {
        insert_forfeit(&ctx, &msg, &race).await?;
        info!(
            "Successfully entered submission for user \"{}\"",
            &msg.author.name
        );
        return Ok(());
    }

    // lets start with a default submission struct and add in what can here. then we'll
    // pass it to a game-specific function that will add its own info. when these
    // rows are pulled from the db, each game will have its own submission formatter as
    // well that knows which info that game has and how to display it

    // remove backslashes because *some servers* use numbers as emotes
    // we are also REMOVING the first element of the vector here
    let maybe_time: &str = &maybe_submission_text.remove(0).replace("\\", "");
    let time = match parse_variable_time(&maybe_time) {
        Ok(t) => t,
        Err(e) => {
            return Err(anyhow!(
                "Processing submission: Malformed time from user \"{}\": {} - {}",
                &msg.author.name,
                &maybe_time,
                e
            )
            .into());
        }
    };

    let submission = NewSubmission::default()
        .set_runner_id(msg.author.id)
        .set_race_id(race.race_id)
        .name(&msg.author.name)
        .set_time(Some(time))
        .set_game_info(race.race_game, &maybe_submission_text)?;
    diesel::insert_into(submissions)
        .values(submission)
        .execute(&conn)?;

    Ok(())
}

async fn insert_forfeit(ctx: &Context, msg: &Message, race: &AsyncRaceData) -> Result<()> {
    use crate::schema::submissions::dsl::*;

    let submission = NewSubmission {
        runner_id: *msg.author.id.as_u64(),
        race_id: race.race_id,
        race_game: race.race_game,
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

    Ok(())
}

pub async fn build_leaderboard(
    ctx: &Context,
    group: &ChannelGroup,
    race: &AsyncRaceData,
    target: ChannelType,
) -> Result<(), BoxedError> {
    // the caller needs to have checked if there is currently an active race
    // which means we have a leaderboard message to work with
    use crate::schema::messages::columns::*;
    use crate::schema::submissions::columns::runner_forfeit;

    let target_channel_id: u64 = match target {
        ChannelType::Leaderboard => group.leaderboard,
        ChannelType::Submission => group.submission,
        _ => return Err(anyhow!("Did not specify a target channel to put leaderboard in").into()),
    };
    let conn = get_connection(&ctx).await;
    // collect a vector of submissions for this race and sort it
    let mut leaderboard: Vec<Submission> = Submission::belonging_to(race)
        .filter(runner_forfeit.eq(false))
        .load::<Submission>(&conn)?;
    leaderboard.sort_by(|a, b| {
        b.runner_time
            .cmp(&a.runner_time)
            .reverse()
            .then(b.runner_collection.cmp(&a.runner_collection).reverse())
            .then(b.option_number.cmp(&a.option_number).reverse())
    });
    let time_now = Utc::now().naive_utc();
    let mut lb_posts_data: Vec<BotMessage> = BotMessage::belonging_to(race)
        .filter(channel_type.eq(target))
        .load::<BotMessage>(&conn)?;
    lb_posts_data.sort_by(|a, b| b.message_datetime.cmp(&a.message_datetime).reverse());
    let leaderboard_header = race.leaderboard_string();
    // approximating how much to allocate here
    let mut lb_string = String::with_capacity(leaderboard.len() * 40 + 150);
    let mut count: u32 = 1;
    lb_string.push_str(format!("{}\n", leaderboard_header).as_str());
    leaderboard.iter().for_each(|s| {
        // we italicize more recent submissions, but only in the leaderboard channel
        if (time_now - s.submission_datetime < Duration::seconds(21600i64))
            && target == ChannelType::Leaderboard
        {
            lb_string.push_str(format!("\n{}) *{}*", count, &s).as_str());
            count += 1;
        } else {
            lb_string.push_str(format!("\n{}) {}", count, &s).as_str());
            count += 1;
        }
    });

    fill_leaderboard(
        &ctx,
        &mut lb_posts_data,
        &lb_string,
        &group,
        target,
        target_channel_id,
    )
    .await?;

    Ok(())
}

async fn fill_leaderboard(
    ctx: &Context,
    mut lb_posts_data: &mut Vec<BotMessage>,
    lb_string: &String,
    group: &ChannelGroup,
    target: ChannelType,
    target_channel_id: u64,
) -> Result<(), BoxedError> {
    let necessary_posts: usize = lb_string.len() / 2000 + 1;
    if necessary_posts > lb_posts_data.len() {
        lb_posts_data = resize_leaderboard(
            &ctx,
            group.server_id,
            target,
            target_channel_id,
            lb_posts_data,
        )
        .await?;
    }
    // fill buffer then send the post until there's no more
    let mut post_buffer = String::with_capacity(2000);
    let mut post_iterator = lb_posts_data.into_iter().peekable();
    let mut submission_iterator = lb_string
        .split("\n")
        .collect::<Vec<&str>>()
        .into_iter()
        .peekable();

    loop {
        if post_iterator.peek().is_none() {
            return Err(anyhow!("Ran out of space for leaderboard").into());
        }

        match submission_iterator.peek() {
            Some(line) => {
                if line.len() + &post_buffer.len() <= 2000 {
                    post_buffer
                        .push_str(format!("\n{}", submission_iterator.next().unwrap()).as_str())
                } else if line.len() + post_buffer.len() > 2000 {
                    let mut post = ctx
                        .http
                        .get_message(target_channel_id, post_iterator.next().unwrap().message_id)
                        .await?;
                    post.edit(ctx, |x| x.content(&post_buffer)).await?;
                    post_buffer.clear();
                }
            }
            None => {
                let mut post = ctx
                    .http
                    .get_message(target_channel_id, post_iterator.next().unwrap().message_id)
                    .await?;
                post.edit(ctx, |x| x.content(post_buffer)).await?;
                break;
            }
        };
    }

    Ok(())
}

async fn resize_leaderboard<'a>(
    ctx: &Context,
    this_server_id: u64,
    target: ChannelType,
    target_channel_id: u64,
    lb_posts: &'a mut Vec<BotMessage>,
) -> Result<&'a mut Vec<BotMessage>, BoxedError> {
    use crate::schema::messages::dsl::*;
    // we only ever need one more post than we have to hold all submissions
    let conn = get_connection(&ctx).await;
    let new_message: Message = ChannelId::from(target_channel_id)
        .say(&ctx, "Placeholder")
        .await?;
    let new_msg_data =
        BotMessage::from_serenity_msg(&new_message, this_server_id, lb_posts[0].race_id, target);

    diesel::insert_into(messages)
        .values(&new_msg_data)
        .execute(&conn)?;
    lb_posts.push(new_msg_data);

    Ok(lb_posts)
}

pub fn parse_variable_time(maybe_time: &str) -> Result<NaiveTime> {
    // technically NaiveTime represents a time of day but it works for our purposes
    let mut time_string = String::with_capacity(9);
    let split_time = maybe_time.clone().split(":");
    match split_time.count() {
        0 => return Err(anyhow!("Empty submission time")),
        1 => {
            time_string.push_str("00:00:");
            time_string.push_str(maybe_time);
        }
        2 => {
            time_string.push_str("00:");
            time_string.push_str(maybe_time);
        }
        3 => {
            time_string.push_str(maybe_time);
        }
        _ => return Err(anyhow!("Tried to parse malformed time")),
    };
    let time = NaiveTime::parse_from_str(&time_string, "%H:%M:%S").map_err(|e| anyhow!("{}", e));

    time
}
