use std::{error::Error, fmt};

#[derive(Debug, Clone)]
pub struct BotError {
    details: String,
}

impl BotError {
    fn new(msg: &str) -> BotError {
        BotError {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for BotError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Murahdahla crashed: {}", self.details)
    }
}

impl Error for BotError {
    fn description(&self) -> &str {
        &self.details
    }
}

impl From<serenity::Error> for BotError {
    fn from(err: serenity::Error) -> Self {
        BotError::new(err.description())
    }
}

impl From<diesel::result::Error> for BotError {
    fn from(err: diesel::result::Error) -> Self {
        BotError::new(err.description())
    }
}

pub struct RoleError {
    details: String,
}

impl RoleError {
    fn new(msg: &str) -> RoleError {
        RoleError {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for RoleError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl From<serenity::Error> for RoleError {
    fn from(err: serenity::Error) -> Self {
        RoleError::new(err.description())
    }
}
