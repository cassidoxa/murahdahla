use std::{convert::TryFrom, default::Default, str::FromStr};

use anyhow::{anyhow, Result};
use base64;
use reqwest::get;
use serde::Deserialize;
use serde_json::{from_str, Value};
use uuid::Uuid;

use crate::{
    discord::submissions::NewSubmission,
    games::{AsyncGame, GameName},
    helpers::BoxedError,
};

const BASE_URL: &'static str = "https://sm.samus.link/api/seed/";

#[derive(Debug, Clone)]
pub struct SMTotalGame {
    map: Value,
    url: String,
}

#[derive(Debug, Deserialize)]
pub struct SMTotalSettings {
    logic: String,
    #[serde(skip_deserializing)]
    goal: String,
    placement: String,
    #[serde(skip_deserializing)]
    seed: String,
    #[serde(skip_deserializing)]
    race: String,
    #[serde(skip_deserializing)]
    gamemode: String,
    #[serde(skip_deserializing)]
    players: String,
}

impl Default for SMTotalSettings {
    fn default() -> Self {
        SMTotalSettings {
            logic: String::new(),
            goal: String::new(),
            placement: String::new(),
            seed: String::new(),
            race: String::new(),
            gamemode: String::new(),
            players: String::new(),
        }
    }
}

impl SMTotalGame {
    pub async fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        let game_slug: &str = args_str.split("/").last().unwrap();
        let map = get_seed(game_slug).await?;
        let url = args_str.to_string(); // we've already parsed this as a url and should know it's good
        let game = SMTotalGame { map: map, url: url };

        Ok(game)
    }
}

async fn get_seed(slug: &str) -> Result<Value> {
    let mut buf = [0; 36];

    let padded_slug = format!("{}==", slug);
    let guid_vec = base64::decode_config(padded_slug, base64::URL_SAFE)?;
    let guid = Uuid::from_slice(&guid_vec)?;
    let guid_str = guid.to_simple().encode_lower(&mut buf);
    let url = format!("{}{}", BASE_URL, guid_str);
    let seed = get(&url).await?.json().await?;

    Ok(seed)
}

pub struct SMTotalCollectionRate(u16);

impl TryFrom<u16> for SMTotalCollectionRate {
    type Error = BoxedError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 316 {
            Err(anyhow!("SM (Total) collection rate not between 0 - 100").into())
        } else {
            Ok(SMTotalCollectionRate(value))
        }
    }
}

// we implement Into here because this only works one way
impl Into<u16> for SMTotalCollectionRate {
    fn into(self) -> u16 {
        self.0
    }
}

impl AsyncGame for SMTotalGame {
    fn game_name(&self) -> GameName {
        GameName::SMTotal
    }

    fn settings_str(&self) -> Result<String, BoxedError> {
        let settings_map = &self
            .map
            .as_object()
            .ok_or_else(|| anyhow!("Error parsing sm.samus.link response as Object"))?
            .get("worlds")
            .ok_or_else(|| anyhow!("Error retreiving SM (Total) world from object"))?
            .as_array()
            .ok_or_else(|| anyhow!("Error parsing worlds array"))?[0]
            .as_object()
            .ok_or_else(|| {
                anyhow!("Error parsing first element of SM (Total) world array as object")
            })?
            .get("settings")
            .ok_or_else(|| anyhow!("Error retrieving settings from sm.samus.link Object"))?;
        let settings: SMTotalSettings = from_str(
            settings_map
                .as_str()
                .ok_or_else(|| anyhow!("Error deserializing SM (Total) settings"))?,
        )?;

        let logic = match settings.logic.as_str() {
            "tournament" => "Tournament",
            "casual" => "Casual",
            _ => "Unknown Logic",
        };
        let placement = match settings.placement.as_str() {
            "split" => "Major/Minor",
            "full" => "Full",
            _ => "Unknown Item Placement",
        };

        let code = &self.map["hash"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing goal").into())?;

        let game_string: String = format!("{} {} ({}) ", logic, placement, code);

        Ok(game_string)
    }

    fn has_url(&self) -> bool {
        true
    }

    fn game_url<'a>(&'a self) -> Option<&'a str> {
        Some(&self.url)
    }
}

pub fn game_info<'a>(
    submission: &'a mut NewSubmission,
    msg: &Vec<&str>,
) -> Result<&'a mut NewSubmission, BoxedError> {
    // make sure there's enough elements in the vec to maybe use
    if msg.len() != 1 {
        return Err(anyhow!("SM (Total) submission did not include collection rate.").into());
    }

    let number = u16::from_str(&msg[0])?;
    let collection = SMTotalCollectionRate::try_from(number)?;
    submission.set_collection(Some(collection));

    Ok(submission)
}
