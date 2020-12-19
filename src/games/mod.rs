use std::fmt;

use anyhow::{anyhow, Result};
use chrono::{offset::Utc, NaiveDate};
use diesel::{
    backend::Backend, deserialize, deserialize::FromSql, expression::AsExpression,
    helper_types::AsExprOf, prelude::*, sql_types::Text,
};
use serenity::framework::standard::Args;
use url::Url;

use crate::{
    discord::channel_groups::ChannelGroup,
    games::{
        other::OtherGame,
        save_parsing::{SMTotalSram, SMZ3Sram, SaveParser, Z3rSram},
        smtotal::SMTotalGame,
        smvaria::SMVARIAGame,
        smz3::SMZ3Game,
        z3r::Z3rGame,
    },
    helpers::*,
    schema::*,
    BoxedError,
};

pub mod other;
mod save_parsing;
pub mod smtotal;
pub mod smvaria;
pub mod smz3;
pub mod z3r;

pub type BoxedGame = Box<dyn AsyncGame + Send + Sync>;
pub type BoxedSave = Box<dyn SaveParser + Send + Sync + 'static>;

#[derive(Debug, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "ChannelGroup", foreign_key = "channel_group_id")]
#[table_name = "async_races"]
#[primary_key(race_id)]
pub struct AsyncRaceData {
    pub race_id: u32,
    pub channel_group_id: Vec<u8>,
    pub race_active: bool,
    pub race_date: NaiveDate,
    pub race_game: GameName,
    pub race_type: RaceType,
    pub race_info: String,
    pub race_url: Option<String>,
}

#[derive(Debug, Insertable)]
#[table_name = "async_races"]
pub struct NewAsyncRaceData {
    pub channel_group_id: Vec<u8>,
    pub race_active: bool,
    pub race_date: NaiveDate,
    pub race_game: GameName,
    pub race_type: RaceType,
    pub race_info: String,
    pub race_url: Option<String>,
}

impl NewAsyncRaceData {
    pub fn new_from_game(
        game: &BoxedGame,
        group_id: &Vec<u8>,
        race_type: RaceType,
    ) -> Result<Self, BoxedError> {
        let todays_date = Utc::today().naive_utc();
        let settings_string = game.settings_str()?;
        let maybe_url: Option<String>;
        match game.has_url() {
            true => maybe_url = Some(game.game_url().unwrap().to_owned()),
            false => maybe_url = None,
        };

        Ok(NewAsyncRaceData {
            channel_group_id: group_id.clone(),
            race_active: true,
            race_date: todays_date,
            race_game: game.game_name(),
            race_type: race_type,
            race_info: settings_string,
            race_url: maybe_url,
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, FromSqlRow)]
pub enum GameName {
    ALTTPR,
    SMZ3,
    FF4FE,
    SMVARIA,
    SMTotal,
    Other,
}

impl<DB> FromSql<Text, DB> for GameName
where
    DB: Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match String::from_sql(bytes)?.as_str() {
            "ALTTPR" => Ok(GameName::ALTTPR),
            "SMZ3" => Ok(GameName::SMZ3),
            "FF4 FE" => Ok(GameName::FF4FE),
            "SM VARIA" => Ok(GameName::SMVARIA),
            "SM Total" => Ok(GameName::SMTotal),
            "Other" => Ok(GameName::Other),
            x => Err(format!("Unrecognized game name: {}", x).into()),
        }
    }
}

impl AsExpression<Text> for GameName {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl<'a> AsExpression<Text> for &'a GameName {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl fmt::Display for GameName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            GameName::ALTTPR => write!(f, "ALTTPR"),
            GameName::SMZ3 => write!(f, "SMZ3"),
            GameName::FF4FE => write!(f, "FF4 FE"),
            GameName::SMVARIA => write!(f, "SM VARIA"),
            GameName::SMTotal => write!(f, "SM Total"),
            GameName::Other => write!(f, "Other"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, FromSqlRow)]
pub enum RaceType {
    IGT,
    RTA,
}

impl<DB> FromSql<Text, DB> for RaceType
where
    DB: Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match String::from_sql(bytes)?.as_str() {
            "IGT" => Ok(RaceType::IGT),
            "RTA" => Ok(RaceType::RTA),
            x => Err(format!("Unrecognized race type {}", x).into()),
        }
    }
}

impl AsExpression<Text> for RaceType {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl<'a> AsExpression<Text> for &'a RaceType {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl fmt::Display for RaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            RaceType::RTA => write!(f, "RTA"),
            RaceType::IGT => write!(f, "IGT"),
        }
    }
}

pub trait AsyncGame {
    // returns the name of the game played (eg ALTTPR, FF4 FE, SMZ3, etc)
    fn game_name(&self) -> GameName;

    // returns a string with some information about settings or full flags
    fn settings_str(&self) -> Result<String, BoxedError>;

    // whether this game has an associated url.
    fn has_url(&self) -> bool;

    // return game url if it exists
    fn game_url<'a>(&'a self) -> Option<&'a str>;
}

pub fn determine_game(args_str: &str) -> GameName {
    // we parse as a url here just to determine the game then discard the url
    // TODO: if we have, say, a festive alttpr url without /h/, we could make it an
    // other game
    let game_url = match Url::parse(args_str) {
        Ok(u) => u,
        Err(_) => return GameName::Other,
    };
    match game_url.host_str() {
        Some(g) if (g == "alttpr.com" && game_url.path().contains("/h/")) => GameName::ALTTPR,
        Some(g) if (g == "samus.link" && game_url.path().contains("/seed")) => GameName::SMZ3,
        Some(g) if (g == "sm.samus.link" && game_url.path().contains("/seed")) => GameName::SMTotal,
        Some(g)
            if ((g == "randommetroidsolver.pythonanywhere.com" || g == "varia.run")
                && game_url.path().contains("/customizer")) =>
        {
            GameName::SMVARIA
        }
        // Some(g) if g == "ff4fe.com" => GameName::FF4FE,
        Some(_) => GameName::Other,
        None => GameName::Other,
    }
}

pub async fn get_game_boxed(args: &Args) -> Result<BoxedGame, BoxedError> {
    let game_category = determine_game(args.rest());
    match game_category {
        GameName::ALTTPR => Ok(Box::new(Z3rGame::new_from_str(args.rest()).await?)),
        GameName::SMZ3 => Ok(Box::new(SMZ3Game::new_from_str(args.rest()).await?)),
        GameName::SMTotal => Ok(Box::new(SMTotalGame::new_from_str(args.rest()).await?)),
        GameName::SMVARIA => Ok(Box::new(SMVARIAGame::new_from_str(args.rest()).await?)),
        GameName::Other => Ok(Box::new(OtherGame::new_from_str(args.rest())?)),
        _ => Err(anyhow!("Tried to start unknown game").into()),
    }
}

pub fn get_save_boxed(maybe_save: &Vec<u8>, game: GameName) -> Result<BoxedSave, BoxedError> {
    match game {
        GameName::ALTTPR => Ok(Box::new(Z3rSram::new_from_slice(maybe_save)?)),
        GameName::SMZ3 => Ok(Box::new(SMZ3Sram::new_from_slice(maybe_save)?)),
        GameName::SMTotal => Ok(Box::new(SMTotalSram::new_from_slice(maybe_save)?)),
        _ => Err(anyhow!("Received file for game that doesn't support save parsing").into()),
    }
}

pub fn get_maybe_active_race(conn: &PooledConn, group: &ChannelGroup) -> Option<AsyncRaceData> {
    use crate::schema::async_races::columns::*;

    AsyncRaceData::belonging_to(group)
        .filter(race_active.eq(true))
        .get_result(conn)
        .ok()
}

pub trait DataDisplay {
    fn base_string(&self) -> String;

    fn leaderboard_string(&self) -> String;
}

impl DataDisplay for NewAsyncRaceData {
    fn base_string(&self) -> String {
        let mut base_game_string = format!("{} - ", self.race_date);
        if self.race_game != GameName::Other {
            base_game_string.push_str(format!("{} - ", self.race_game).as_str());
        }
        base_game_string.push_str(format!("({}) - {}", self.race_type, self.race_info).as_str());
        if self.race_url.is_some() {
            base_game_string.push_str(format!(" - <{}>", self.race_url.as_ref().unwrap()).as_str());
        }

        base_game_string
    }

    fn leaderboard_string(&self) -> String {
        let base_game_string = self.base_string();
        let lb_string = format!("Leaderboard for {}", base_game_string);

        lb_string
    }
}

impl DataDisplay for AsyncRaceData {
    // we could maybe return &str instead of Strings here and maybe save a bit of
    // memory?
    fn base_string(&self) -> String {
        let mut base_game_string = format!("{} - ", self.race_date);
        if self.race_game != GameName::Other {
            base_game_string.push_str(format!("{} ", self.race_game).as_str());
        }
        base_game_string.push_str(format!("({}) - {}", self.race_type, self.race_info).as_str());
        if self.race_url.is_some() {
            base_game_string.push_str(format!(" - <{}>", self.race_url.as_ref().unwrap()).as_str());
        }

        base_game_string
    }

    fn leaderboard_string(&self) -> String {
        let base_game_string = self.base_string();
        let lb_string = format!("Leaderboard for {}", base_game_string);

        lb_string
    }
}
