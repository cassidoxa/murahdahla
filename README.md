# Overview

Murahdahla is a bot that administrates asynchronous [ALttPR](https://alttpr.com) races
based on the [serenity](https://github.com/serenity-rs/serenity) Rust library.

# Setup

Building requires the Rust compiler and Cargo. You can find these in your distro's repo or
they can be installed via [rustup](https://rustup.rs) (the preferred method.) 

## 1. Clone this repo

## 2. Diesel Setup

Murahdahla requires a MySQL database and uses the [Diesel ORM](https://github.com/diesel-rs/diesel)
to manage the database. First install `diesel-cli` by running

`cargo install diesel_cli --no-default-features --features "mysql"`

Put the database URL in your `.env` file. Then run `diesel migration setup`. If the database needs
to be migrated following an update, you must run `diesel migration run` after pulling the update.
Refer to the MySQL documentation for instructions on setting up a database.

## 4. Setting up environment variables

Copy and rename `.env.example` to `.env`. 

## 3. Compile

You can compile this program by running `cargo build --release` from the main directory. This
pulls in all the dependencies from crates.io and requires an internet connection. After compiling the binary
will be located in `murahdahla/target/release`.

# Running the Bot and Managing Permissions

Please see the Discord documentation on [bots and apps](https://discordapp.com/developers/docs/intro#bots-and-apps).
Also see instructions for adding a bot you're hosting yourself [here.](https://github.com/jagrosh/MusicBot/wiki/Adding-Your-Bot-To-Your-Server)
Running this bot requires creating an application and a bot user. The bot requires the following
permissions: Manage Roles, Send Messages, Manage Messages, View Channels, and Read Message History. Add the bot's
token into the appropriate field in your `.env` file. Adding a bot to your server automatically
gives that bot its own role, named after the bot. You may need to set these permissions manually
on that role.

Murahdahla depends on three channels and two roles for administrating async races: a submission channel where times
are entered, a leaderboard channel where a leaderboard for the current race is displayed, and a 
spoiler channel where people can discuss the game, visible only after submitting a time. The first
channel should be visible to everyone on the server, the rest should be visible only to those with
a "spoilers" role that you create and enter into the `.env` file by its name. 

The second role is a bot admin role; the bot will only accept commands to start and stop races
from users with this role. Create this role (or use an existing role) and add its name to `.env`.
You must also add the channels into `.env` by their channel number, which is the last number after
the final slash `/` in the URL when that channel is opened.

Once the binary is generated, you can move it and run it anywhere, as long as the `.env` file is
in the directory it's being run from with all the fields filled in correctly.

Notes: Currently Murahdahla only supports async races in one server per bot client. Adding the bot
to multiple servers and running multiple races concurrently will result in unintened behavior.
Only games generated by v31 of the randomizer and customizer are supported.

# Starting and Stopping Races

To start a race, a user with the bot admin role can use the `!start` command followed by a permalink
to the game they want to race (ex: `!start https://alttpr.com/en/h/R6vBYxW5MP`). When you want to
start a new race, you currently *must* use the `!stop` command beforehand, which removes the spoiler
role from everyone who has it and edits the leaderboard into that game's message in the submission
channel.
