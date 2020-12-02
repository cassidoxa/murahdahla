use anyhow::{anyhow, Error, Result};
use chrono::naive::NaiveDate;
use reqwest::get;
use serde_json::{from_value, Value};
use url::Url;

use crate::helpers::BoxedError;

pub struct Z3rGame {
    patch: Value,
    url: Url,
}

impl Z3rGame {
    pub async fn new_from_url(url: Url) -> Result<Self, BoxedError> {
        todo!();
    }

    pub async fn get_patch(game_id: &str) -> Result<Value> {
        let url_string: String = format!(
            "https://s3.us-east-2.amazonaws.com/alttpr-patches/{}.json",
            game_id
        );
        let patch_json: Value = get(url_string.as_str()).await?.json().await?;

        Ok(patch_json)
    }
}

pub fn get_game_string(game_json: Value, url: &str, todays_date: &NaiveDate) -> Result<String> {
    match game_json["spoiler"]["meta"]["spoilers"]
        .as_str()
        .ok_or::<Error>(anyhow!("Error parsing spoiler meta information"))
    {
        Ok("mystery") => {
            let code: Vec<&str> = get_code(&game_json["patch"]);
            return Ok(format!(
                "{} - Mystery ({}/{}/{}/{}/{}) - <{}>",
                *todays_date, code[0], code[1], code[2], code[3], code[4], url
            ));
        }
        _ => (),
    }
    let state = match game_json["spoiler"]["meta"]["mode"]
        .as_str()
        .ok_or::<Error>(anyhow!("Error parsing game state"))?
    {
        "open" => "Open",
        "standard" => "Standard",
        "inverted" => "Inverted",
        "retro" => "Retro",
        _ => "Unknown State",
    };
    let goal = match game_json["spoiler"]["meta"]["goal"]
        .as_str()
        .ok_or::<Error>(anyhow!("Error parsing goal").into())?
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
        .ok_or::<Error>(anyhow!("Error parsing GT crystals"))?;
    let ganon_crystals = game_json["spoiler"]["meta"]["entry_crystals_ganon"]
        .as_str()
        .ok_or::<Error>(anyhow!("Error parsing Ganon crystals"))?;
    let code: Vec<&str> = get_code(&game_json["patch"]);

    let dungeon_items = match game_json["spoiler"]["meta"]["dungeon_items"]
        .as_str()
        .ok_or::<Error>(anyhow!("Error parsing dungeon item shuffle"))?
    {
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
            .ok_or::<Error>(anyhow!("Error parsing entrance shuffle"))?
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
        .ok_or::<Error>(anyhow!("Error parsing logic"))?
    {
        "NoGlitches" => "No Glitches ",
        "OverworldGlitches" => "Overworld Glitches ",
        "Major Glitches" => "Major Glitches ",
        "None" => "No Logic ",
        _ => "Unknown Logic ",
    };

    let mut game_string: String = format!(
        "{} - {} {} {}/{} ",
        *todays_date, state, goal, gt_crystals, ganon_crystals
    );
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
            "({}/{}/{}/{}/{}) - <{}>\n",
            code[0], code[1], code[2], code[3], code[4], url
        )
        .as_str(),
    );

    Ok(game_string)
}

#[inline]
fn get_code(patch: &Value) -> Vec<&'static str> {
    // we have to search for the code values here, they will not always
    // be located at the same position in the json
    // TODO: maybe a more performant way to find this
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
