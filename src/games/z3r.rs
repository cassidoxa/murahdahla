use std::{
    convert::{TryFrom, TryInto},
    fmt,
    io::Cursor,
    str::from_utf8,
    str::FromStr,
};

use anyhow::{anyhow, Error, Result};
use byteorder::{ByteOrder, LittleEndian, ReadBytesExt};
use chrono::naive::{NaiveDate, NaiveTime};
use reqwest::get;
use serde_json::{from_value, Value};
use url::Url;

use crate::{
    discord::submissions::NewSubmission,
    games::{bitmask, AsyncGame, GameName, SaveParser},
    helpers::BoxedError,
};

const BASE_URL: &'static str = "https://s3.us-east-2.amazonaws.com/alttpr-patches/";
const SRAM_CHECKSUM: u16 = 0x55AA;
const ROM_NAMES: [&'static str; 2] = ["VT", "ER"];

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
    map: Value,
    url: String,
}

impl Z3rGame {
    pub async fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        let game_id = args_str.split("/").last().unwrap();
        let map = get_patch(game_id).await?;
        let url = args_str.to_string(); // we've already parsed this as a url and should know it's good
        let game = Z3rGame { map: map, url: url };

        Ok(game)
    }
}

async fn get_patch(game_id: &str) -> Result<Value> {
    let url_string: String = format!(
        "https://s3.us-east-2.amazonaws.com/alttpr-patches/{}.json",
        game_id
    );
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
        let game_json = &self.map;
        match game_json["spoiler"]["meta"]["spoilers"]
            .as_str()
            .ok_or::<BoxedError>(anyhow!("Error parsing spoiler meta information").into())
        {
            Ok("mystery") => {
                let code: Vec<&str> = get_code(&game_json["patch"])?;
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
        let code: Vec<&str> = get_code(&game_json["patch"])?;

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
fn get_code(patch: &Value) -> Result<Vec<&'static str>> {
    let mut code: Vec<&'static str> = Vec::with_capacity(5);

    // it's pretty safe to unwrap here unless the alttpr patch format
    // changes suddenly and dramatically
    let code: Vec<&'static str> = patch
        .as_array()
        .ok_or_else(|| anyhow!("Error parsing ALTTPR patch as vector"))?
        .iter()
        .find(|v| v.as_object().unwrap().contains_key("1573397"))
        .ok_or_else(|| anyhow!("Could not find code offset in ALTTPR patch"))?
        .get("1573397")
        .unwrap()
        .as_array()
        .ok_or_else(|| anyhow!("Error parsing ALTTPR patch as vector"))?
        .iter() // now THATS what i call an iterator
        .map(|x| code_map(x.as_u64().unwrap()))
        .collect();

    Ok(code)
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

pub struct Z3rSram([u8; 32768]);

// most of this is copied from z3r sram parsing tool here:
// https://github.com/cassidoxa/z3r-sramr/
// but that is mostly designed to support the python bindings
// if/when i or someone else ever makes it into something more general
// purpose it should be brought in as a dependency
// but mostly we don't need a map with everything

impl Z3rSram {
    pub fn new_from_slice(s: &Vec<u8>) -> Result<Z3rSram, BoxedError> {
        if s.len() != 32768 {
            return Err(anyhow!("Incorrect file size for ALTTPR SRAM").into());
        }
        // i can't figure out how to use a ref to the actual attachment so let's
        // just copy into a buffer here
        let mut buf = [0; 32768];
        buf.copy_from_slice(s);

        let mut cur = Cursor::new(buf);
        let checksum_validity: u16 = LittleEndian::read_u16(&buf[0x3E1..0x3E3]);
        if checksum_validity != SRAM_CHECKSUM || buf[0x4F0] != 0xFF {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid file").into());
        }
        // Check the first two characters of the rom name for VT or ER
        let rom_name = &buf[0x2000..0x2002];
        if ROM_NAMES
            .iter()
            .any(|&x| x == from_utf8(&rom_name).unwrap())
            == false
        {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid ROM name").into());
        }
        // Now we check the SRAM's own "inverse" checksum
        let mut checksum = 0u16;
        cur.set_position(0x00);
        while cur.position() < 0x4FE {
            let bytes = cur.read_u16::<LittleEndian>()?;
            checksum = checksum.overflowing_add(bytes).0;
        }
        let expected_inv_checksum = 0x5A5Au16.overflowing_sub(checksum).0;
        let inv_checksum: u16 = LittleEndian::read_u16(&buf[0x4FE..0x500]);
        if inv_checksum != expected_inv_checksum {
            return Err(anyhow!("ALTTPR SRAM Validation Error: Invalid checksum").into());
        }
        Ok(Z3rSram(buf))
    }
}

impl SaveParser for Z3rSram {
    fn game_finished(&self) -> bool {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let finished = get_stat(&mut cur, 0x423, 8, 0).unwrap();
        match finished {
            1u64 => true,
            0u64 => false,
            _ => false,
        }
    }

    fn get_igt(&self) -> Result<NaiveTime, BoxedError> {
        // just remembered that every array size is a distinct type
        // .as_slice() exists but not stable yet
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let igt = Z3rStat::new_time(Some(&mut cur), 0x43Eu64);
        let time = NaiveTime::parse_from_str(&igt.to_string(), "%H:%M:%S")?;

        Ok(time)
    }

    fn get_collection_rate(&self) -> Option<u64> {
        let slice = &self.0[..];
        let mut cur = Cursor::new(slice);
        let collection = get_stat(&mut cur, 0x423, 8, 0).ok();

        collection
    }
}

enum Z3rStat {
    Number(u32),
    Time(String),
}

impl Z3rStat {
    fn new_time<T: Into<u64>>(cur: Option<&mut Cursor<&[u8]>>, num: T) -> Self {
        let value: u32 = match cur {
            Some(cur) => {
                cur.set_position(num.into());
                cur.read_u32::<LittleEndian>().unwrap()
            }
            None => num.into() as u32,
        };
        let hours: u32 = value / (216000u32);
        let mut rem = value % 216000u32;
        let minutes: u32 = rem / 3600u32;
        rem %= 3600u32;
        let seconds: u32 = rem / 60u32;

        let time = format!("{:0>2}:{:0>2}:{:0>2}", hours, minutes, seconds);

        Z3rStat::Time(time)
    }
}

impl fmt::Display for Z3rStat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{}", *n),
            Self::Time(t) => write!(f, "{}", *t),
        }
    }
}

fn get_stat(
    cur: &mut Cursor<&[u8]>,
    offset: u64,
    bits: u32,
    shift: u32,
) -> Result<u64, BoxedError> {
    cur.set_position(offset);
    let bytes: f32 = ((bits as f32 + shift as f32) / 8f32).ceil();
    let mut value = match bytes as u8 {
        1 => cur.read_u8().unwrap() as u32,
        2 => cur.read_u16::<LittleEndian>().unwrap() as u32,
        _ => return Err(anyhow!("Tried reading more than two bytes at {}", offset).into()),
    };
    value >>= shift;
    value &= bitmask(bits);

    Ok(value as u64)
}
