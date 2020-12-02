use std::{
    collections::{HashMap, HashSet},
    convert::TryFrom,
    iter::FromIterator,
};

use anyhow::{anyhow, Result};
use diesel::prelude::*;
use serde::{Deserialize, Deserializer};
use serenity::{
    model::{
        channel::Message,
        guild::Guild,
        id::{ChannelId, GuildId, RoleId},
    },
    prelude::*,
};
use uuid::Uuid;

use crate::{discord::servers::DiscordServer, helpers::*, schema::channels};

#[derive(Debug, Clone, Insertable, Queryable, Identifiable, Associations)]
#[belongs_to(parent = "DiscordServer", foreign_key = "server_id")]
#[table_name = "channels"]
#[primary_key(channel_group_id)]
pub struct ChannelGroup {
    pub channel_group_id: Vec<u8>,
    pub server_id: u64,
    pub group_name: String,
    pub submission: u64,
    pub leaderboard: u64,
    pub spoiler: u64,
    pub spoiler_role: String,
}

#[derive(Debug, Deserialize)]
pub struct ChannelGroupYaml {
    #[serde(skip)]
    #[serde(default = "new_uuid")]
    pub channel_group_id: Vec<u8>,
    pub group_name: String,
    pub submission: String,
    pub leaderboard: String,
    pub spoiler: String,
    pub spoiler_role: String,
}

impl ChannelGroup {
    pub async fn new_from_yaml(
        msg: &Message,
        ctx: &Context,
        yaml: &[u8],
    ) -> Result<Self, BoxedError> {
        use serde_yaml;

        let yaml: ChannelGroupYaml = match serde_yaml::from_slice(yaml) {
            Ok(g) => g,
            Err(e) => return Err(Box::new(e) as BoxedError),
        };

        let server = msg.guild(&ctx).await.unwrap();
        let submission_channel_id = match server.channel_id_from_name(&ctx, &yaml.submission).await
        {
            Some(i) => i,
            None => {
                return Err(anyhow!(
                    "Could not get submission channel id from name provided in yaml"
                )
                .into())
            }
        };
        let leaderboard_channel_id =
            match server.channel_id_from_name(&ctx, &yaml.leaderboard).await {
                Some(i) => i,
                None => {
                    return Err(anyhow!(
                        "Could not get leaderboard channel id from name provided in yaml"
                    )
                    .into())
                }
            };
        let spoiler_channel_id = match server.channel_id_from_name(&ctx, &yaml.spoiler).await {
            Some(i) => i,
            None => {
                return Err(
                    anyhow!("Could not get spoiler channel id from name provided in yaml").into(),
                )
            }
        };

        let new_group = ChannelGroup {
            channel_group_id: yaml.channel_group_id,
            server_id: *server.id.as_u64(),
            group_name: yaml.group_name.clone(),
            submission: *submission_channel_id.as_u64(),
            leaderboard: *leaderboard_channel_id.as_u64(),
            spoiler: *spoiler_channel_id.as_u64(),
            spoiler_role: yaml.spoiler_role.clone(),
        };
        validate_new_group(&ctx, &msg, &new_group).await?;

        Ok(new_group)
    }
}

async fn validate_new_group(
    ctx: &Context,
    msg: &Message,
    new_group: &ChannelGroup,
) -> Result<(), BoxedError> {
    // check to make sure the group & role names are < 255 characters
    if [&new_group.group_name, &new_group.spoiler_role]
        .iter()
        .any(|&s| s.len() > 255usize)
    {
        return Err(anyhow!("Group name or spoiler role exceeds 255 characters").into());
    }

    // check to make sure the channels provided in the yaml are actually in this server
    let bot_channels = [
        &new_group.submission,
        &new_group.leaderboard,
        &new_group.spoiler,
    ];
    let all_channels: HashSet<u64> = msg
        .guild(&ctx)
        .await
        .unwrap()
        .channels
        .keys()
        .map(|k| *k.as_u64())
        .collect();
    match bot_channels.iter().all(|c| all_channels.contains(c)) {
        true => (),
        false => {
            let err: BoxedError =
                anyhow!("Channels provided in group yaml not found in server").into();
            return Err(err);
        }
    };

    // we should have a hash set of all submission channels so lets do a quick
    // comparison of the channel provided in the yaml to the ones we have and also
    // check for duplicate group names
    {
        let data = ctx.data.read().await;
        let sub_channels = data
            .get::<SubmissionSet>()
            .expect("Error getting submission channels");
        match sub_channels.contains(&new_group.submission) {
            false => (),
            true => {
                let err: BoxedError = anyhow!(
                    "Provided yaml contains submission channel which has already been assigned"
                )
                .into();
                return Err(err);
            }
        };

        let groups = data
            .get::<GroupContainer>()
            .expect("Error getting groups from sharemap.");
        match groups
            .values()
            .filter(|g| g.server_id == new_group.server_id)
            .any(|g| g.group_name == new_group.group_name)
        {
            false => (),
            true => {
                let err: BoxedError =
                    anyhow!("Provided yaml contains duplicate group name for this server").into();
                return Err(err);
            }
        }

        Ok(())
    }
}

pub fn get_groups(conn: &PooledConn) -> Result<HashMap<u64, ChannelGroup>> {
    use crate::schema::channels::columns::*;
    use crate::schema::channels::dsl::*;
    use diesel::dsl::count;

    let mut group_vec: Vec<ChannelGroup> = channels.load(conn)?;
    let mut group_map: HashMap<u64, ChannelGroup> = HashMap::with_capacity(group_vec.len() + 1);
    group_vec.drain(..).for_each(|g| {
        group_map.insert(g.submission, g);
    });

    Ok(group_map)
}

pub fn get_submission_channels(conn: &PooledConn) -> Result<HashSet<u64>> {
    use crate::schema::channels::columns::*;

    let mut sub_column: Vec<u64> = channels::table.select(submission).load(conn)?;
    let submission_channels: HashSet<u64> = HashSet::from_iter(sub_column.drain(..));

    Ok(submission_channels)
}
