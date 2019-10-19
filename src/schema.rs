table! {
    games (game_id) {
        game_id -> Unsigned<Integer>,
        game_date -> Date,
        guild_id -> Unsigned<Bigint>,
        game_active -> Bool,
    }
}

table! {
    leaderboard (runner_id) {
        runner_id -> Unsigned<Bigint>,
        game_id -> Unsigned<Integer>,
        runner_name -> Varchar,
        runner_time -> Time,
        runner_collection -> Unsigned<Tinyint>,
        runner_forfeit -> Bool,
        submission_datetime -> Datetime,
    }
}

table! {
    posts (post_id) {
        post_id -> Unsigned<Bigint>,
        post_datetime -> Datetime,
        game_id -> Unsigned<Integer>,
        guild_id -> Unsigned<Bigint>,
        guild_channel -> Unsigned<Bigint>,
    }
}

allow_tables_to_appear_in_same_query!(
    games,
    leaderboard,
    posts,
);
