use std::{
    convert::{From, TryFrom},
    error::Error,
    fmt,
};

use anyhow::{anyhow, Result};
use chrono::{offset::Utc, NaiveDate};
use diesel::{
    backend::Backend,
    deserialize,
    deserialize::{FromSql, FromSqlRow},
    expression::AsExpression,
    helper_types::AsExprOf,
    mysql::Mysql,
    prelude::*,
    row::Row,
    sql_types::Text,
};
use serenity::framework::standard::Args;
use url::Url;
use uuid::Uuid;

use crate::{
    discord::{channel_groups::ChannelGroup, servers::DiscordServer},
    schema::*,
    BoxedError,
};

pub mod z3r;

const PERMALINKS: [&str; 3] = ["alttpr.com", "samus.link", "ff4fe.com"];

#[derive(Debug, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "ChannelGroup", foreign_key = "channel_group_id")]
#[table_name = "async_races"]
#[primary_key(race_id)]
pub struct AsyncRaceData {
    pub race_id: u32,
    pub channel_group_id: Vec<u8>,
    pub race_active: bool,
    pub race_date: NaiveDate,
    pub race_game: Game,
    pub race_type: RaceType,
}

#[derive(Debug, Insertable)]
#[table_name = "async_races"]
pub struct NewAsyncRaceData {
    pub channel_group_id: Vec<u8>,
    pub race_active: bool,
    pub race_date: NaiveDate,
    pub race_game: Game,
    pub race_type: RaceType,
}

impl NewAsyncRaceData {
    pub fn new(group_id: &Vec<u8>, game: Game, race_type: RaceType) -> Self {
        let todays_date = Utc::today().naive_utc();
        let mut copied_id = Vec::with_capacity(group_id.len());
        copied_id.copy_from_slice(&group_id[..]);
        NewAsyncRaceData {
            channel_group_id: copied_id,
            race_active: true,
            race_date: todays_date,
            race_game: game,
            race_type: race_type,
        }
    }
}

#[derive(Debug, Copy, Clone, FromSqlRow)]
pub enum Game {
    ALTTPR,
    SMZ3,
    FF4FE,
    SMVARIA,
    SMTotal,
    Other,
}

impl<DB> FromSql<Text, DB> for Game
where
    DB: Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match String::from_sql(bytes)?.as_str() {
            "ALTTPR" => Ok(Game::ALTTPR),
            "SMZ3" => Ok(Game::SMZ3),
            "FF4 FE" => Ok(Game::FF4FE),
            "SM VARIA" => Ok(Game::SMVARIA),
            "SM Total" => Ok(Game::SMTotal),
            "Other" => Ok(Game::Other),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

impl AsExpression<Text> for Game {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl<'a> AsExpression<Text> for &'a Game {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl fmt::Display for Game {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Game::ALTTPR => write!(f, "ALTTPR"),
            Game::SMZ3 => write!(f, "SMZ3"),
            Game::FF4FE => write!(f, "FF4 FE"),
            Game::SMVARIA => write!(f, "SM VARIA"),
            Game::SMTotal => write!(f, "SM Total"),
            Game::Other => write!(f, "Other"),
        }
    }
}

#[derive(Debug, Copy, Clone, FromSqlRow)]
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
            x => Err(format!("Unrecognized variant {}", x).into()),
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

pub trait AsyncGame<'a> {
    // returns the name of the game played (eg ALTTPR, FF4 FE, SMZ3, etc)
    fn game_name(&'a self) -> &'a str;

    // returns a string with some information about settings or full flags
    fn settings_string(&'a self) -> &'a str;

    // returns IGT or RTA
    fn game_type(&self) -> RaceType;
}

pub fn determine_game(args: &Args) -> Game {
    let game_url = match Url::parse(args.rest()) {
        Ok(u) => u,
        Err(_) => return Game::Other,
    };
    match game_url.host_str() {
        Some(g) if g == "alttpr.com" => Game::ALTTPR,
        Some(g) if g == "samus.link" => Game::SMZ3,
        Some(g) if g == "ff4fe.com" => Game::FF4FE,
        Some(g) if g == "randommetroidsolver.pythonanywhere.com" => Game::SMVARIA,
        Some(g) if g == "sm.samus.link" => Game::SMTotal,
        Some(_) => Game::Other,
        None => Game::Other,
    }
}

pub fn get_game_string(args: &Args, race_data: &AsyncRaceData) -> String {
    todo!();
    // let base_string = match race_data
}
