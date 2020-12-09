use std::{
    collections::{HashMap, HashSet},
    fmt,
    iter::FromIterator,
};

use anyhow::{anyhow, Result};
use diesel::{
    backend::Backend, deserialize, deserialize::FromSql, expression::AsExpression,
    helper_types::AsExprOf, prelude::*, sql_types::Text,
};
use serde::Deserialize;
use serenity::{model::channel::Message, prelude::*};

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
    pub spoiler_role_id: u64,
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
        yaml_bytes: &[u8],
    ) -> Result<Self, BoxedError> {
        let yaml: ChannelGroupYaml = match serde_yaml::from_slice(yaml_bytes) {
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
        let spoiler_role_id = match server.role_by_name(&yaml.spoiler_role) {
            Some(r) => r.id,
            None => {
                return Err(anyhow!(
                    "Could not get spoiler channel role id from role name provided in yaml"
                )
                .into())
            }
        };

        let new_group = ChannelGroup {
            channel_group_id: yaml.channel_group_id,
            server_id: *server.id.as_u64(),
            group_name: yaml.group_name.clone(),
            submission: *submission_channel_id.as_u64(),
            leaderboard: *leaderboard_channel_id.as_u64(),
            spoiler: *spoiler_channel_id.as_u64(),
            spoiler_role_id: *spoiler_role_id.as_u64(),
        };
        validate_new_group(&ctx, &msg, &new_group, &yaml.spoiler_role).await?;

        Ok(new_group)
    }
}

#[derive(Debug, Clone, Copy, FromSqlRow)]
pub enum ChannelType {
    Submission,
    Leaderboard,
    Spoiler,
}

impl<DB> FromSql<Text, DB> for ChannelType
where
    DB: Backend,
    String: FromSql<Text, DB>,
{
    fn from_sql(bytes: Option<&DB::RawValue>) -> deserialize::Result<Self> {
        match String::from_sql(bytes)?.as_str() {
            "submission" => Ok(ChannelType::Submission),
            "leaderboard" => Ok(ChannelType::Leaderboard),
            "spoiler" => Ok(ChannelType::Spoiler),
            x => Err(format!("Unrecognized channel type: {}", x).into()),
        }
    }
}

impl AsExpression<Text> for ChannelType {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl<'a> AsExpression<Text> for &'a ChannelType {
    type Expression = AsExprOf<String, Text>;

    fn as_expression(self) -> Self::Expression {
        <String as AsExpression<Text>>::as_expression(self.to_string())
    }
}

impl fmt::Display for ChannelType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ChannelType::Submission => write!(f, "submission"),
            ChannelType::Leaderboard => write!(f, "leaderboard"),
            ChannelType::Spoiler => write!(f, "spoiler"),
        }
    }
}

async fn validate_new_group(
    ctx: &Context,
    msg: &Message,
    new_group: &ChannelGroup,
    spoiler_role_name: &str,
) -> Result<(), BoxedError> {
    // check to make sure the group & role names are < 255 characters
    if [&new_group.group_name, spoiler_role_name]
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

#[inline]
pub fn get_groups(conn: &PooledConn) -> Result<HashMap<u64, ChannelGroup>> {
    use crate::schema::channels::dsl::*;

    let mut group_vec: Vec<ChannelGroup> = channels.load(conn)?;
    let mut group_map: HashMap<u64, ChannelGroup> = HashMap::with_capacity(group_vec.len() + 1);
    group_vec.drain(..).for_each(|g| {
        group_map.insert(g.submission, g);
    });

    Ok(group_map)
}

pub async fn get_group(ctx: &Context, msg: &Message) -> ChannelGroup {
    // this should only be called when we've checked that the message is in
    // a submission channel so we know there is a group in the map
    let data = ctx.data.read().await;
    let group = data
        .get::<GroupContainer>()
        .expect("No group container in share map")
        .get(msg.channel_id.as_u64())
        .unwrap();

    group.clone()
}

#[inline]
pub fn get_submission_channels(conn: &PooledConn) -> Result<HashSet<u64>> {
    use crate::schema::channels::columns::*;

    let mut sub_column: Vec<u64> = channels::table.select(submission).load(conn)?;
    let submission_channels: HashSet<u64> = HashSet::from_iter(sub_column.drain(..));

    Ok(submission_channels)
}

pub async fn in_submission_channel(ctx: &Context, msg: &Message) -> bool {
    let data = ctx.data.read().await;
    let channels = data
        .get::<SubmissionSet>()
        .expect("Error getting submission channels");
    channels.contains(msg.channel_id.as_u64())
}
