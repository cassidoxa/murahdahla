use std::{convert::TryFrom, str::FromStr};

use anyhow::{anyhow, Error, Result};
use chrono::naive::NaiveDate;
use reqwest::get;
use serde_json::{from_value, Value};
use url::Url;

use crate::{
    discord::submissions::NewSubmission,
    games::{AsyncGame, GameName},
    helpers::BoxedError,
};

pub struct Z3rGame {
    patch: Value,
    url: String,
}

impl Z3rGame {
    pub async fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        // we know we have an alttpr.com url. let's verify it's a link to a
        // generated game. if the url contains "/h/" we should be good
        match args_str.contains("/h/") {
            true => (),
            false => return Err(anyhow!("alttpr.com link does not contain a game").into()),
        };
        let game_id = args_str.split("/").last().unwrap();
        let patch = get_patch(game_id).await?;
        let url = args_str.to_string(); // we've already parsed this as a url and should know it's good
        let game = Z3rGame {
            patch: patch,
            url: url,
        };

        Ok(game)
    }
}

async fn get_patch(game_id: &str) -> Result<Value> {
    let url_string: String = format!(
        "https://s3.us-east-2.amazonaws.com/alttpr-patches/{}.json",
        game_id
    );
    let patch_json = get(url_string.as_str()).await?.json().await?;

    Ok(patch_json)
}

pub struct Z3rCollectionRate(u16);

impl TryFrom<u16> for Z3rCollectionRate {
    type Error = BoxedError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 216 {
            Err(anyhow!("ALTTPR Collection Rate must be a number from 0 to 216").into())
        } else {
            Ok(Z3rCollectionRate(value))
        }
    }
}

// we implement Into here because this only works one way
impl Into<u16> for Z3rCollectionRate {
    fn into(self) -> u16 {
        self.0
    }
}

impl AsyncGame for Z3rGame {
    fn game_name(&self) -> GameName {
        GameName::ALTTPR
    }

    fn settings_str(&self) -> Result<String, BoxedError> {
        // TODO: check for "special" here because we need to handle festives etc differently
        let game_json = &self.patch;
        match game_json["spoiler"]["meta"]["spoilers"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing spoiler meta information").into())
        {
            Ok("mystery") => {
                let code: Vec<&str> = get_code(&game_json["patch"]);
                return Ok(format!(
                    "Mystery ({}/{}/{}/{}/{})",
                    code[0], code[1], code[2], code[3], code[4]
                ));
            }
            _ => (),
        }
        let state = match game_json["spoiler"]["meta"]["mode"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing game state").into())?
        {
            "open" => "Open",
            "standard" => "Standard",
            "inverted" => "Inverted",
            "retro" => "Retro",
            _ => "Unknown State",
        };
        let goal = match game_json["spoiler"]["meta"]["goal"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing goal").into())?
        {
            "ganon" => "Defeat Ganon",
            "fast_ganon" => "Fast Ganon",
            "dungeons" => "All Dungeons",
            "pedestal" => "Pedestal",
            "triforce-hunt" => "Triforce Hunt",
            _ => "Unknown Goal",
        };
        let gt_crystals = game_json["spoiler"]["meta"]["entry_crystals_tower"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing GT crystals").into())?;
        let ganon_crystals = game_json["spoiler"]["meta"]["entry_crystals_ganon"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing Ganon crystals").into())?;
        let code: Vec<&str> = get_code(&game_json["patch"]);

        let dungeon_items = match game_json["spoiler"]["meta"]["dungeon_items"]
            .as_str()
            .ok_or::<BoxedError>(
            anyhow!("Error parsing dungeon item shuffle").into(),
        )? {
            "standard" => "Standard ",
            "mc" => "MC ",
            "mcs" => "MCS ",
            "full" => "Keysanity ",
            _ => "Unknown Dungeon Item Shuffle ",
        };
        let mut shuffle = "Vanilla Shuffle ";
        if game_json["spoiler"]["meta"].get("shuffle") != None {
            shuffle = match game_json["spoiler"]["meta"]["shuffle"]
                .as_str()
                .ok_or::<BoxedError>(anyhow!("Error parsing entrance shuffle").into())?
            {
                "simple" => "Simple Shuffle ",
                "restricted" => "Restricted Shuffle ",
                "full" => "Full Shuffle ",
                "crossed" => "Crossed Shuffle ",
                "insanity" => "Insanity Shuffle ",
                _ => "Unknown Shuffle ",
            };
        }
        let logic = match game_json["spoiler"]["meta"]["logic"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing logic").into())?
        {
            "NoGlitches" => "No Glitches ",
            "OverworldGlitches" => "Overworld Glitches ",
            "Major Glitches" => "Major Glitches ",
            "None" => "No Logic ",
            _ => "Unknown Logic ",
        };

        let mut game_string: String =
            format!("{} {} {}/{} ", state, goal, gt_crystals, ganon_crystals);
        if dungeon_items != "Standard " {
            game_string.push_str(dungeon_items);
        }
        if shuffle != "Vanilla Shuffle " {
            game_string.push_str(shuffle);
        }
        if logic != "No Glitches " {
            game_string.push_str(logic);
        }
        game_string.push_str(
            format!(
                "({}/{}/{}/{}/{})",
                code[0], code[1], code[2], code[3], code[4]
            )
            .as_str(),
        );

        Ok(game_string)
    }

    fn has_url(&self) -> bool {
        true
    }

    fn game_url<'a>(&'a self) -> Option<&'a str> {
        Some(&self.url)
    }
}

#[inline]
fn get_code(patch: &Value) -> Vec<&'static str> {
    // we have to search for the code values here, they will not always
    // be located at the same position in the json
    // TODO: sort & binary search??
    let mut code: Vec<&'static str> = Vec::with_capacity(5);
    for i in patch.as_array().unwrap().iter() {
        if i.as_object().unwrap().contains_key("1573397") {
            code = from_value::<Vec<u8>>(i.get("1573397").unwrap().clone())
                .unwrap()
                .into_iter()
                .map(|x| code_map(x))
                .collect();
            break;
        }
    }

    code
}

const fn code_map(value: u8) -> &'static str {
    match value {
        0 => "Bow",
        1 => "Boomerang",
        2 => "Hookshot",
        3 => "Bombs",
        4 => "Mushroom",
        5 => "Powder",
        6 => "Ice Rod",
        7 => "Pendant",
        8 => "Bombos",
        9 => "Ether",
        10 => "Quake",
        11 => "Lamp",
        12 => "Hammer",
        13 => "Shovel",
        14 => "Flute",
        15 => "Net",
        16 => "Book",
        17 => "Empty Bottle",
        18 => "Green Potion",
        19 => "Somaria",
        20 => "Cape",
        21 => "Mirror",
        22 => "Boots",
        23 => "Gloves",
        24 => "Flippers",
        25 => "Pearl",
        26 => "Shield",
        27 => "Tunic",
        28 => "Heart",
        29 => "Map",
        30 => "Compass",
        31 => "Key",
        _ => "Unknown",
    }
}

pub fn game_info<'a>(
    submission: &'a mut NewSubmission,
    msg: &Vec<&str>,
) -> Result<&'a mut NewSubmission, BoxedError> {
    // for alttpr we just use the collection rate by default. we could also set one of
    // the optional values here if we wanted to take some other input. suppose we
    // wanted a bonk counter for example
    // see the Display trait on Submissions for how this gets formatted on discord

    // but first we make sure there's enough elements in the vec to maybe use
    if msg.len() != 1 {
        return Err(anyhow!("ALTTPR submission did not include collection rate.").into());
    }

    let number = u16::from_str(&msg[0])?;
    let collection = Z3rCollectionRate::try_from(number)?;
    submission.set_collection(Some(collection));

    Ok(submission)
}
