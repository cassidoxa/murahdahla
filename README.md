# Overview

Murahdahla is a bot that administrates asynchronous races, mostly designed around
randomizers, based on the [serenity](https://github.com/serenity-rs/serenity) Rust crate.
This bot was inspired by/a clone of the seed-of-the-week bot written by Neerm for the
FF4 Free Enterprise community.

# Table of Contents  

1. [Setup and Building](#setup-and-building)  
2. [Using The Bot](#using-the-bot)  
3. [Bot Commands](#bot-commands)
4. [Support](#support)
5. [Acknowledgements](#acknowledgements)

# Setup and Building

## Build Requirements

Setting up and running this bot requires some familiarity with linux, MySQL, Cargo, 
and some basic system administration skills. It may work on other unix-like operating
systems as well. Building and running on Windows is probably possible but not
currently supported. There are some difficulties related to compiling the
diesel-cli tool on Windows.

### 1. Minimum Rust version: 1.48

Building requires the Rust compiler and Cargo. You can find these in your distro's repo or
they can be installed via [rustup](https://rustup.rs) (the preferred method.)

### 2. MySQL

The bot uses a MySQL database on the backend

### 3. Mysql client and dev packages

Diesel requires a MySQL client to run and diesel-cli requires the MySQL client dev packages
to build. These should be available in your distro's repo and may be under the name "mariadb"
instead of MySQL. On debian the required dev package is libmariadb-dev-compat.

### 4. diesel-cli

Once you have Cargo you can install this by running `cargo install diesel_cli --no-default-features --features "mysql"`

## Setup

### 1. Clone this repo

### 2. Copy the .env example

Copy and rename `.env.example` to `.env` by running `cp .env.example .env`.

### 3. Diesel Setup

Put the database URL in your `.env` file in the DATABASE\_URL environment variable. Then
run `diesel setup`. If the database needs to be migrated following an update,
run `diesel migration run` or `diesel database reset` (note: this will drop all existing
tables) after pulling the update. I will do my best to note when a migration is required in
release notes or contact bot operators ahead of time. Refer to the MySQL documentation for
instructions on setting up and managing a database.

### 4. Compile

You can compile this program by running `cargo build --release` from the main directory. This
pulls in all the dependencies from crates.io and requires an internet connection. After
compiling, the binary will be located in `murahdahla/target/release`. The program must be
run from a working directory containing the .env file with the proper variables.

## Managing Permissions

Please see the Discord documentation on [bots and apps](https://discordapp.com/developers/docs/intro#bots-and-apps).
Also see instructions for adding a bot you're hosting yourself [here.](https://github.com/jagrosh/MusicBot/wiki/Adding-Your-Bot-To-Your-Server)
Running this bot requires creating an application and a bot user. The bot requires the following
permissions: Manage Roles, Send Messages, Manage Messages, View Channels, and Read Message History.
One you have a bot token, add it into your `.env` file. Adding a bot to your server automatically
gives that bot its own role, named after the bot. You may need to set these permissions manually
on that role. Also note that Discord roles have a hierarchy. The bot will only add and remove
roles that you explicitly designate. If you encounter problems assigning or removing a spoiler
role, check to make sure that it's correctly place in the role hierarchy.

A server owner will be able to run all bot commands (listed below.) Additionally, an owner
can assign a "mod" role and an "admin" role to give other server members permission to
run certain commands.

Note: Only regular games generated by versions of supported randomizers at the time of release
are supported. Some "special" games like ALTTPR festives may not work correctly and there may be
a gap between a new version of a randomizer and its games being supported.

# Using The Bot

## Groups

Before starting a race, a server owner or admin must add a group with the `!addgroup` command
and a yaml file attached (see the example in this repo.) Groups are composed of three channels
and a spoiler role: a submission channel where races are started and where runners enter their
times, a leaderboard channel where the bot displays a leaderboard for a currently active race,
and a spoiler channel where runners can discuss the current game.

Only the bot should have permissions to send messages in the leaderboard channel and the
spoiler role should gate access to both the leaderboard and spoiler channels. You will have
to configure this yourself as the bot currently will not do it automatically. Additionally,
all the channels and roles contained in a yaml file must exist before the bot will accept it.

**NOTE: When a group is active, all messages in the submission channel will be deleted! This
is intentional. This includes commands and time submissions**

## Starting and Stopping Races

Once you have a group set up, you can start a race. Races are timed by real-time (RTA) or
in-game time (IGT.) There are separate commands for each, `!rtastart [argument]` and
`!igtstart [argument]`. The argument you pass can be a URL to a randomizer game's permalink
or some information about a game that will let participants play the same game. If you
pass a URL and the bot supports that randomizer, it will gather some informatio about it
to display. The bot will send one message in the leaderboard channel and one in the
submission channel with the information you've provided (so theoretically a user who knows
there is an active race can just look at that channel and have what they need to get started.)

When a race is stopped, the leaderboard moves from the leaderboard channel to the submission
channel. A race can be stopped with the `!stop` command or simply by starting a new race
with another start command.

## Supported Games

Currently the bot supports permalinks for: 

* ALTTPR (alttpr.com )
* SMZ3 (samus.link)
* SM (total's randomizer at sm.samus.link)
* SM VARIA (currently only supports seeds generated with variabeta.pythonanywhere.com domain)

This means that if you start a race with a permalink from one of these sites, the bot
will gather some information about the game to display in the submission channel and on
the leaderboard. You can also pass other URLs or information (e.g. settings, flag strings,
seeds, etc) to the start commands and those will be displayed as provided as well.

## Time Submissions and Arguments

Subissions will always require a time in the format "HH:MM:SS". Additionally, many randomizers
will have a collection rate or some other information that may be relevant. Currently the
bot supports one optional argument that it may require with a time. When playing a
non-supported, arbitrary game, only a time will be required. The following games will require a
collection rate in addition to a time:

* ALTTPR
* SMZ3
* SM Rando (total)
* SM VARIA

When a time is submitted, the user will be given the designated spoiler role if the
submission was successful. If the submission was unsuccessful, the message will be
deleted but the role will not be assigned. Usually when this happens, it means the submission
was malformed in some way or lacking a required argument. Note that submissions have a
hard maximum of 23:59:59.

# Bot Commands

All of these commands are available to a "maintenance user" which can be set by the `MAINTENANCE_USER`
environment variable in `.env` with a 64-bit Discord user id.

## Admin Commands

**!addgroup** - Requires an attached yaml file (see example in this repo.) Adds a channel group.
Multiple, non-overlapping channel groups can exist per server.

**!removegroup [name]** - Removes a group with the name supplied.

**!listgroups** - Sends a DM with a list of names of current groups.

**!setadminrole [role name]** - Sets a role that will allow users with that role to run admin commands.

**!setmodrole [role name]** - Sets a role that will allow users with that role to run mod commands.

**!removeadminrole [role name]** - Removes previously set admin role. Users with that role will no longer
be able to run admin commands.

**!removemodrole [role name]** - Removes previously set mod role.

## Mod Commands

**!igtstart/!rtastart [URL or game info]**

**!refresh** - Refreshes the leaderboard from the database.

**!removetime [runner name]** - Removes a runner's submission from the leaderboard and their spoiler role.

**!settime [runner name] [time]** - Changes the time of a runner's existing submission

**!setcollection [runner name] [collection rate]** - Changes the collection rate of a runner's
existing submission, if collection rate is being used for that game.


# Support

This bot is still a work in progress. It has been known to break or work incorrectly on
occasion. I am willing to provide support and bug fixes provided you are running the bot on
linux. Feel free to file a github issue or contact me on discord. If you don't have my
contact info on discord, the person running the bot or the person who told you about this
bot likely does. Please try to provide logs if available. If the bot is panicking/crashing,
you can run it with `RUST_BACKTRACE=1` to get a backtrace.

# Acknowledgements

I would like to thank a few people for their help and assistance. First and foremost, Synack
from the ALTTPR community for hosting an instance of the bot, providing
logs, giving advice, and generally being a very solid guy. Without shamelessly borrowing his
pyz3r implementations it would have taken a lot more time and effort to write this. Thank you
as well to timp and the GMP community for using the bot and providing feedback. Big thanks
to Neerm for developing the original seed-of-the-week bot whose functionality this is based on.
