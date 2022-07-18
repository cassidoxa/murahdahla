use std::{convert::TryFrom, str::FromStr};

use anyhow::{anyhow, Result};
use reqwest::get;
use serde_json::{Map, Value};

use crate::{
    discord::submissions::NewSubmission,
    games::{AsyncGame, GameName},
    helpers::BoxedError,
};

const BASE_URL: &'static str = "https://alttpr-patch-data.s3.us-east-2.amazonaws.com/";
const FILE_SELECT_CODE: u64 = 0x180215; // tables.asm: 1007

const fn code_map(value: u64) -> &'static str {
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

#[derive(Debug, Clone)]
pub struct Z3rGame {
    meta: Value,
    patches: Map<String, Value>,
    url: String,
}

impl Z3rGame {
    pub async fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        let game_id = args_str.split("/").last().unwrap();
        let mut meta = get_patch(game_id).await?;
        let url = args_str.to_string(); // we've already parsed this as a url and should know it's good
        let mut patch_json: Value = meta["patch"].take();
        let patches = patch_to_map(&mut patch_json)?;
        let game = Z3rGame {
            meta: meta,
            patches: patches,
            url: url,
        };

        Ok(game)
    }
}

async fn get_patch(game_id: &str) -> Result<Value> {
    let url = format!("{}{}.json", BASE_URL, game_id);
    let patch_json = get(&url).await?.json().await?;

    Ok(patch_json)
}

pub struct Z3rCollectionRate(u16);

impl TryFrom<u16> for Z3rCollectionRate {
    type Error = BoxedError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value > 216 {
            Err(anyhow!("ALTTPR collection rate not between 0 - 216").into())
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
        let game_json = &self.meta;
        let game_patches = &self.patches;
        match game_json["spoiler"]["meta"]["spoilers"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing spoiler meta information").into())
        {
            Ok("mystery") => {
                let code: Vec<&str> = get_code(game_patches)?;
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
        let code: Vec<&str> = get_code(game_patches)?;

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
fn patch_to_map(patches: &mut Value) -> Result<Map<String, Value>> {
    // Converts the ROM patch data to serde_json's Map type and discards the "outer"
    // keys, giving us a map with offsets mapped to arrays of bytes.
    let mut patch_map: Map<String, Value> = Map::with_capacity(450);
    patches
        .as_array_mut()
        .ok_or_else(|| anyhow!("Error parsing ALTTPR patches into vector"))?
        .into_iter()
        .map(|inner| inner.as_object_mut().unwrap())
        .for_each(|m| {
            let key: String = m.keys().last().unwrap().clone();
            let value: Value = m.remove(&key).unwrap();
            patch_map.insert(key, value);
        });

    Ok(patch_map)
}

#[inline]
fn get_code(patch_map: &Map<String, Value>) -> Result<Vec<&'static str>> {
    let mut code_vec: Vec<&'static str> = Vec::with_capacity(5);
    let index_int = patch_map
        .keys()
        .map(|s| s.parse::<u64>().unwrap())
        .reduce(|a, b| {
            if (b == FILE_SELECT_CODE) || ((b > a) && (b < FILE_SELECT_CODE)) {
                b
            } else {
                a
            }
        })
        .ok_or_else(|| anyhow!("Error finding file select code patch index"))?;
    let index_string = index_int.to_string();
    let patch_slice = patch_map[&index_string]
        .as_array()
        .ok_or_else(|| anyhow!("Error parsing file select code data"))?;
    let mut code_offset = 0u64;
    if index_int != FILE_SELECT_CODE {
        code_offset = FILE_SELECT_CODE - index_int;
    }
    if patch_slice.len() < (code_offset + 5) as usize {
        return Ok(vec!["Bow", "Boomerang", "Hookshot", "Bombs", "Mushroom"]);
    }
    for i in 0..5 {
        let code_byte = patch_slice[(i + code_offset) as usize]
            .as_u64()
            .ok_or_else(|| anyhow!("Error parsing code byte as integer"))?;
        code_vec.push(code_map(code_byte));
    }

    Ok(code_vec)
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
