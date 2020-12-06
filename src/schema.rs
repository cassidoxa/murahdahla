table! {
    async_races (race_id) {
        race_id -> Unsigned<Integer>,
        channel_group_id -> Binary,
        race_active -> Bool,
        race_date -> Date,
        race_game -> Tinytext,
        race_type -> Tinytext,
    }
}

table! {
    channels (channel_group_id) {
        channel_group_id -> Binary,
        server_id -> Unsigned<Bigint>,
        group_name -> Tinytext,
        submission -> Unsigned<Bigint>,
        leaderboard -> Unsigned<Bigint>,
        spoiler -> Unsigned<Bigint>,
        spoiler_role_id -> Unsigned<Bigint>,
    }
}

table! {
    messages (message_id) {
        message_id -> Unsigned<Bigint>,
        message_datetime -> Datetime,
        race_id -> Unsigned<Integer>,
        server_id -> Unsigned<Bigint>,
        channel_id -> Unsigned<Bigint>,
        channel_type -> Tinytext,
    }
}

table! {
    servers (server_id) {
        server_id -> Unsigned<Bigint>,
        owner_id -> Unsigned<Bigint>,
        admin_role_id -> Nullable<Unsigned<Bigint>>,
        mod_role_id -> Nullable<Unsigned<Bigint>>,
    }
}

table! {
    submissions (submission_id) {
        submission_id -> Unsigned<Integer>,
        runner_id -> Unsigned<Bigint>,
        race_id -> Unsigned<Integer>,
        submission_datetime -> Datetime,
        runner_name -> Varchar,
        runner_time -> Nullable<Time>,
        runner_collection -> Nullable<Unsigned<Smallint>>,
        option_number -> Nullable<Unsigned<Integer>>,
        option_text -> Nullable<Tinytext>,
        runner_forfeit -> Bool,
    }
}

joinable!(async_races -> channels (channel_group_id));
joinable!(channels -> servers (server_id));
joinable!(messages -> async_races (race_id));
joinable!(submissions -> async_races (race_id));

allow_tables_to_appear_in_same_query!(
    async_races,
    channels,
    messages,
    servers,
    submissions,
);
