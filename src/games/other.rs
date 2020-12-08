use anyhow::{anyhow, Result};

use crate::{
    games::{AsyncGame, GameName},
    helpers::BoxedError,
};

pub struct OtherGame {
    text: String,
}

impl OtherGame {
    pub fn new_from_str(args_str: &str) -> Result<Self, BoxedError> {
        // arbitrary but lets make sure the string here isn't *too* long
        if args_str.len() > 400usize {
            return Err(anyhow!("String for other game is too long").into());
        }

        Ok(OtherGame {
            text: args_str.to_owned(),
        })
    }
}

impl AsyncGame for OtherGame {
    fn game_name(&self) -> GameName {
        GameName::Other
    }

    fn settings_str(&self) -> Result<String, BoxedError> {
        Ok(self.text.clone())
    }

    fn has_url(&self) -> bool {
        false
    }

    fn game_url<'a>(&'a self) -> Option<&'a str> {
        None
    }
}
