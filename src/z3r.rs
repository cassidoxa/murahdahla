use std::collections::HashMap;

use chrono::naive::NaiveDate;
use reqwest::get;
use serde_json::{from_value, Value};

use crate::error::BotError;

pub fn get_game_string(
    game_id: &str,
    url: &str,
    todays_date: &NaiveDate,
) -> Result<String, BotError> {
    // TODO: .unwrap() is a bad practice, maybe needs better error handling,
    // but should work in all cases for v31 games
    let url_string: String = format!(
        "https://s3.us-east-2.amazonaws.com/alttpr-patches/{}.json",
        game_id
    );
    let game_json: Value = get(url_string.as_str())?.json()?;
    match game_json["spoiler"]["meta"]["spoilers"]
        .as_str()
        .ok_or(BotError::new("Error parsing spoiler information"))
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
        .ok_or(BotError::new("Error parsing game state"))?
    {
        "open" => "Open",
        "standard" => "Standard",
        "inverted" => "Inverted",
        "retro" => "Retro",
        _ => "Unknown State",
    };
    let goal = match game_json["spoiler"]["meta"]["goal"]
        .as_str()
        .ok_or(BotError::new("Error parsing goal"))?
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
        .ok_or(BotError::new("Error parsing GT crystals"))?;
    let ganon_crystals = game_json["spoiler"]["meta"]["entry_crystals_ganon"]
        .as_str()
        .ok_or(BotError::new("Error parsing Ganon crystals"))?;
    let code: Vec<&str> = get_code(&game_json["patch"]);

    let dungeon_items = match game_json["spoiler"]["meta"]["dungeon_items"]
        .as_str()
        .ok_or(BotError::new("Error parsing dungeon item shuffle"))?
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
            .ok_or(BotError::new("Error parsing entrance shuffle"))?
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
        .ok_or(BotError::new("Error parsing logic"))?
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

fn get_code(patch: &Value) -> Vec<&str> {
    let mut code: Vec<&str> = Vec::with_capacity(5);
    for i in patch.as_array().unwrap().iter() {
        if i.as_object().unwrap().contains_key("1573397") {
            code = from_value::<Vec<u8>>(i.get("1573397").unwrap().clone())
                .unwrap()
                .into_iter()
                .map(|x| CODEMAP[&x])
                .collect();
        }
    }

    code
}

lazy_static! {
    static ref CODEMAP: HashMap<u8, &'static str> = {
        let mut map = HashMap::with_capacity(32);
        map.insert(0, "Bow");
        map.insert(1, "Boomerang");
        map.insert(2, "Hookshot");
        map.insert(3, "Bombs");
        map.insert(4, "Mushroom");
        map.insert(5, "Powder");
        map.insert(6, "Ice Rod");
        map.insert(7, "Pendant");
        map.insert(8, "Bombos");
        map.insert(9, "Ether");
        map.insert(10, "Quake");
        map.insert(11, "Lamp");
        map.insert(12, "Hammer");
        map.insert(13, "Shovel");
        map.insert(14, "Flute");
        map.insert(15, "Net");
        map.insert(16, "Book");
        map.insert(17, "Empty Bottle");
        map.insert(18, "Green Potion");
        map.insert(19, "Somaria");
        map.insert(20, "Cape");
        map.insert(21, "Mirror");
        map.insert(22, "Boots");
        map.insert(23, "Gloves");
        map.insert(24, "Flippers");
        map.insert(25, "Pearl");
        map.insert(26, "Shield");
        map.insert(27, "Tunic");
        map.insert(28, "Heart");
        map.insert(29, "Map");
        map.insert(30, "Compass");
        map.insert(31, "Key");

        map
    };
}
