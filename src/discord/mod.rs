use serenity::model::gateway::GatewayIntents;

pub mod channel_groups;
pub mod commands;
pub mod messages;
pub mod servers;
pub mod submissions;

pub const MURAHDAHLA_INTENTS: GatewayIntents = GatewayIntents::non_privileged();
