use std::str::FromStr;

use anyhow::{anyhow, Result};
use reqwest;
use serde_json::Value;

use crate::{
    discord::submissions::NewSubmission,
    games::{AsyncGame, GameName},
    helpers::BoxedError,
};

// const BASE_URL: &'static str = "https://randommetroidsolver.pythonanywhere.com/customizer";
const API_URL: &str = "https://variabeta.pythonanywhere.com/randoParamsWebServiceAPI";

#[derive(Debug, Clone)]
pub struct SMVARIAGame {
    map: Value,
    url: String,
}

impl SMVARIAGame {
    pub async fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        let game_slug: &str = args_str.split('/').last().unwrap();
        let url = args_str.to_string();
        let map = get_seed(game_slug).await?;
        let game = SMVARIAGame { map, url };

        Ok(game)
    }
}

async fn get_seed(slug: &str) -> Result<Value> {
    let params = [("guid", &slug)];
    let client = reqwest::Client::new();
    let json_str: String = client
        .post(API_URL)
        .header("Content-Type", "application/json")
        .form(&params)
        .send()
        .await?
        .json::<Value>()
        .await?
        .as_str()
        .ok_or_else(|| anyhow!("Error parsing VARIA API response as str"))?
        .to_owned();

    // feel like there's a better way but I couldn't figure this out
    let seed = Value::from_str(&json_str)?;

    Ok(seed)
}

pub struct SMVARIACollectionRate(u16);

impl TryFrom<u16> for SMVARIACollectionRate {
    type Error = BoxedError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 316 {
            Err(anyhow!("SM VARIA collection rate not between 0 - 100").into())
        } else {
            Ok(SMVARIACollectionRate(value))
        }
    }
}

impl From<SMVARIACollectionRate> for u16 {
    fn from(c: SMVARIACollectionRate) -> Self {
        c.0
    }
}

impl AsyncGame for SMVARIAGame {
    fn game_name(&self) -> GameName {
        GameName::SMVARIA
    }

    fn settings_str(&self) -> Result<String, BoxedError> {
        let game_json = &self
            .map
            .as_object()
            .ok_or_else(|| anyhow!("Error parsing sm.samus.link response as Object"))?;
        let skill_preset = game_json["preset"]
            .as_str()
            .ok_or_else(|| anyhow!("Error parsing VARIA response"))?;
        let split: &str = match game_json["majorsSplit"]
            .as_str()
            .ok_or_else(|| anyhow!("Error parsing VARIA response"))?
        {
            "Major" => "Major/Minor",
            "Full" => "Full",
            "Chozo" => "Chozo",
            _ => "Unknown Item Split",
        };
        let mut base_settings = format!("\"{}\" {} ", skill_preset, split);
        if game_json["areaRandomization"]
            .as_str()
            .ok_or_else(|| anyhow!("Error parsing game state"))?
            == "on"
        {
            base_settings.push_str("Area Rando ")
        }
        if game_json["bossRandomization"]
            .as_str()
            .ok_or_else(|| anyhow!("Error parsing VARIA response"))?
            == "on"
        {
            base_settings.push_str("Boss Rando ")
        }
        if game_json["doorsColorsRando"]
            .as_str()
            .ok_or_else(|| anyhow!("Error parsing VARIA response"))?
            == "on"
        {
            base_settings.push_str("Door Color Rando ")
        }

        Ok(base_settings)
    }

    fn has_url(&self) -> bool {
        true
    }

    fn game_url(&self) -> Option<&str> {
        Some(&self.url)
    }
}

pub fn game_info<'a>(
    submission: &'a mut NewSubmission,
    msg: &Vec<&str>,
) -> Result<&'a mut NewSubmission, BoxedError> {
    // make sure there's enough elements in the vec to maybe use
    if msg.len() != 1 {
        return Err(anyhow!("SM VARIA submission did not include collection rate.").into());
    }

    let number = u16::from_str(msg[0])?;
    let collection = SMVARIACollectionRate::try_from(number)?;
    submission.set_collection(Some(collection));

    Ok(submission)
}
