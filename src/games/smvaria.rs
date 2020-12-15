use std::{convert::TryFrom, str::FromStr};

use anyhow::{anyhow, Result};
use reqwest::get;
use scraper::{Html, Selector};

use crate::{
    discord::submissions::NewSubmission,
    games::{AsyncGame, GameName},
    helpers::BoxedError,
};

// const BASE_URL: &'static str = "https://randommetroidsolver.pythonanywhere.com/customizer";

#[derive(Debug, Clone)]
pub struct SMVARIAGame {
    url: String,
    skill: String,
    split: String,
    area: bool,
    boss: bool,
    door_color: bool,
}

impl SMVARIAGame {
    pub async fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        let url = args_str.to_string();
        let html_response: String = get(&url).await?.text().await?;
        let html = Html::parse_fragment(&html_response);

        let select_err: &'static str = "Error creating selector for VARIA HTML parsing";
        let settings =
            Selector::parse(r#"div[id="seedInfoVisibility"]"#).map_err(|_| anyhow!(select_err))?;
        let table = Selector::parse("table").map_err(|_| anyhow!(select_err))?;
        let tr = Selector::parse("tr").map_err(|_| anyhow!(select_err))?;
        let td = Selector::parse("td").map_err(|_| anyhow!(select_err))?;

        let settings_fragment = html.select(&settings).next().unwrap();
        let skill: String = settings_fragment
            .select(&table)
            .nth(1)
            .unwrap()
            .select(&tr)
            .nth(0)
            .unwrap()
            .select(&td)
            .nth(1)
            .unwrap()
            .inner_html();
        let split: String = match settings_fragment
            .select(&table)
            .nth(2)
            .unwrap()
            .select(&tr)
            .nth(1)
            .unwrap()
            .select(&td)
            .nth(1)
            .unwrap()
            .inner_html()
            .as_str()
        {
            "Major" => "Major/Minor".to_owned(),
            "Full" => "Full".to_owned(),
            "Chozo" => "Chozo".to_owned(),
            _ => "Unknown Item Split".to_owned(),
        };
        let area: bool = match settings_fragment
            .select(&table)
            .nth(4)
            .unwrap()
            .select(&tr)
            .nth(0)
            .unwrap()
            .select(&td)
            .nth(1)
            .unwrap()
            .inner_html()
            .as_str()
        {
            "on" => true,
            _ => false,
        };
        let boss: bool = match settings_fragment
            .select(&table)
            .nth(4)
            .unwrap()
            .select(&tr)
            .nth(5)
            .unwrap()
            .select(&td)
            .nth(1)
            .unwrap()
            .inner_html()
            .as_str()
        {
            "on" => true,
            _ => false,
        };
        let door_color: bool = match settings_fragment
            .select(&table)
            .nth(4)
            .unwrap()
            .select(&tr)
            .nth(3)
            .unwrap()
            .select(&td)
            .nth(1)
            .unwrap()
            .inner_html()
            .as_str()
        {
            "on" => true,
            _ => false,
        };

        let game = SMVARIAGame {
            url,
            skill,
            split,
            area,
            boss,
            door_color,
        };

        Ok(game)
    }
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

// we implement Into here because this only works one way
impl Into<u16> for SMVARIACollectionRate {
    fn into(self) -> u16 {
        self.0
    }
}

impl AsyncGame for SMVARIAGame {
    fn game_name(&self) -> GameName {
        GameName::SMVARIA
    }

    fn settings_str(&self) -> Result<String, BoxedError> {
        let skill_preset: &str = &self.skill;
        let split: &str = &self.split;
        let mut base_settings = format!("\"{}\" {}", skill_preset, split);
        match self.area {
            false => (),
            true => base_settings.push_str("Area Rando "),
        };
        match self.boss {
            false => (),
            true => base_settings.push_str("Boss Rando "),
        };
        match self.door_color {
            false => (),
            true => base_settings.push_str("Door Color Rando "),
        };

        Ok(base_settings)
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
        return Err(anyhow!("SM VARIA submission did not include collection rate.").into());
    }

    let number = u16::from_str(&msg[0])?;
    let collection = SMVARIACollectionRate::try_from(number)?;
    submission.set_collection(Some(collection));

    Ok(submission)
}

#[derive(Debug, Clone)]
struct Selectors {
    settings: Selector,
    table: Selector,
    tr: Selector,
    td: Selector,
}
