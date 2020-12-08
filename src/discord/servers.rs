use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use diesel::prelude::*;
use serenity::{
    framework::standard::Args,
    model::{
        channel::Message,
        guild::Guild,
        id::{ChannelId, GuildId, RoleId},
    },
    prelude::*,
};

use crate::{helpers::*, schema::servers};

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum Permission {
    None,
    Mod,
    Admin,
}

#[derive(Debug, PartialEq)]
pub enum ServerRoleAction {
    Add,
    Remove,
}

#[derive(Debug, Clone, Copy, Insertable, Queryable, Identifiable)]
#[table_name = "servers"]
#[primary_key(server_id)]
pub struct DiscordServer {
    pub server_id: u64,
    pub owner_id: u64,
    pub admin_role_id: Option<u64>,
    pub mod_role_id: Option<u64>,
}

impl DiscordServer {
    fn determine_user_permissions<T: Into<u64>>(self, id: T, roles: &Vec<RoleId>) -> Permission {
        if &self.owner_id == &id.into() {
            return Permission::Admin;
        };
        if self.admin_role_id.is_none() && self.mod_role_id.is_none() {
            return Permission::None;
        };

        match self.admin_role_id.is_some() {
            false => (),
            true => {
                let has_admin = roles
                    .iter()
                    .any(|r| r.as_u64() == &self.admin_role_id.unwrap());
                if has_admin {
                    return Permission::Admin;
                };
            }
        };
        match self.mod_role_id.is_some() {
            false => (),
            true => {
                let has_admin = roles
                    .iter()
                    .any(|r| r.as_u64() == &self.mod_role_id.unwrap());
                if has_admin {
                    return Permission::Mod;
                };
            }
        };

        Permission::None
    }

    pub fn set_role(&mut self, role_id: Option<u64>, role_type: Permission) {
        match role_type {
            Permission::Mod => self.mod_role_id = role_id,
            Permission::Admin => self.admin_role_id = role_id,
            Permission::None => (),
        };
    }
}

pub async fn parse_role(ctx: &Context, msg: &Message, mut args: Args) -> Result<u64, BoxedError> {
    let role_name = args.single_quoted::<String>()?;
    let guild = msg.guild(&ctx).await.unwrap();
    let role_id: u64 = match guild.role_by_name(&role_name) {
        Some(r) => *r.id.as_u64(),
        None => return Err(anyhow!("Tried to set role that doesn't exist on server").into()),
    };

    Ok(role_id)
}

pub fn get_servers(conn: &PooledConn) -> Result<HashMap<GuildId, DiscordServer>> {
    use crate::schema::servers::columns::*;
    use crate::schema::servers::dsl::*;
    use diesel::dsl::count;

    let mut server_vec: Vec<DiscordServer> = servers.load(conn)?;
    let num_servers: usize = servers.select(count(server_id)).execute(conn)?;
    let mut server_map: HashMap<GuildId, DiscordServer> = HashMap::with_capacity(num_servers + 1);

    server_vec.drain(..).for_each(|s| {
        server_map.insert(GuildId::from(s.server_id), s);
    });

    Ok(server_map)
}

pub async fn check_permissions(ctx: &Context, msg: &Message, req: Permission) -> Result<()> {
    let server: Guild = msg.guild(&ctx).await.unwrap();
    if server.owner_id == msg.author.id {
        return Ok(());
    }; // owner can do any command
    let user_roles = &msg.member.as_ref().unwrap().roles;
    let server_data: DiscordServer = {
        let data = ctx.data.read().await;
        *data
            .get::<ServerContainer>()
            .expect("No server hashmap in share map")
            .get(&server.id)
            .unwrap()
    };
    let user_permissions = server_data.determine_user_permissions(msg.author.id, user_roles);
    match user_permissions >= req {
        true => Ok(()),
        false => Err(anyhow!(
            "User \"{}\" does not have required permissions",
            &msg.author.name
        )),
    }
}

pub async fn add_server(ctx: &Context, msg: &Message) -> Result<()> {
    use crate::schema::servers::dsl::*;
    use diesel::insert_or_ignore_into;

    let guild_id = msg.guild_id.unwrap();
    let new_server = DiscordServer {
        server_id: *guild_id.as_u64(),
        owner_id: *msg.guild(&ctx).await.unwrap().owner_id.as_u64(),
        admin_role_id: None,
        mod_role_id: None,
    };

    let conn = get_connection(&ctx).await;
    insert_or_ignore_into(servers)
        .values(&new_server)
        .execute(&conn)?;
    {
        let mut data = ctx.data.write().await;
        let server_map = data
            .get_mut::<ServerContainer>()
            .expect("No server hashmap in share map.");
        server_map.insert(guild_id, new_server);
    }

    Ok(())
}

pub async fn add_spoiler_role(
    ctx: &Context,
    msg: &Message,
    role_id: u64,
) -> Result<(), BoxedError> {
    let mut member = msg.member(&ctx).await?;
    let _ = member.add_role(&ctx, role_id).await?;

    Ok(())
}
