use serenity::model::gateway::GatewayIntents;

pub mod channel_groups;
pub mod commands;
pub mod messages;
pub mod servers;
pub mod submissions;

pub fn intents() -> GatewayIntents {
    let mut intents: GatewayIntents = GatewayIntents::empty();
    intents.insert(GatewayIntents::MESSAGE_CONTENT);
    intents.insert(GatewayIntents::DIRECT_MESSAGES);
    intents.insert(GatewayIntents::GUILD_MESSAGES);
    intents.insert(GatewayIntents::GUILD_MESSAGE_REACTIONS);

    intents
}
