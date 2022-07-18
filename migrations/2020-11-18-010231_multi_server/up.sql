CREATE TABLE servers(
    server_id BIGINT(20) UNSIGNED PRIMARY KEY,
    owner_id BIGINT(20) UNSIGNED NOT NULL,
    admin_role_id BIGINT(20) UNSIGNED,
    mod_role_id BIGINT(20) UNSIGNED
);

CREATE TABLE channels(
    channel_group_id BINARY(16) PRIMARY KEY,
    server_id BIGINT(20) UNSIGNED NOT NULL,
    group_name TINYTEXT NOT NULL,
    submission BIGINT(20) UNSIGNED NOT NULL,
    leaderboard BIGINT(20) UNSIGNED NOT NULL,
    spoiler BIGINT(20) UNSIGNED NOT NULL,
    spoiler_role_id BIGINT(20) UNSIGNED NOT NULL,
    FOREIGN KEY (server_id)
        REFERENCES servers(server_id)
        ON DELETE CASCADE
);

CREATE TABLE async_races(
    race_id INT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    channel_group_id BINARY(16) NOT NULL,
    race_active TINYINT(1) NOT NULL,
    race_date DATE NOT NULL,
    race_game TINYTEXT NOT NULL,
    race_type TINYTEXT NOT NULL,
    race_info TEXT NOT NULL,
    race_url TINYTEXT,
    INDEX (channel_group_id),
    FOREIGN KEY (channel_group_id)
        REFERENCES channels(channel_group_id)
        ON DELETE CASCADE
);

CREATE TABLE messages(
    message_id BIGINT(20) UNSIGNED PRIMARY KEY,
    message_datetime DATETIME NOT NULL,
    race_id INT UNSIGNED NOT NULL,
    server_id BIGINT(20) UNSIGNED NOT NULL,
    channel_id BIGINT(20) UNSIGNED NOT NULL,
    channel_type TINYTEXT NOT NULL,
    INDEX (race_id),
    FOREIGN KEY (race_id)
        REFERENCES async_races(race_id)
        ON DELETE CASCADE
);

CREATE TABLE submissions(
    submission_id INT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    runner_id BIGINT(20) UNSIGNED NOT NULL,
    race_id INT UNSIGNED NOT NULL,
    race_game TINYTEXT NOT NULL,
    submission_datetime DATETIME NOT NULL,
    runner_name VARCHAR(32) NOT NULL,
    runner_time TIME,
    runner_collection SMALLINT(3) UNSIGNED,
    option_number INT UNSIGNED,
    option_text TINYTEXT,
    runner_forfeit TINYINT(1) NOT NULL,
    INDEX (race_id),
    FOREIGN KEY (race_id)
        REFERENCES async_races(race_id)
        ON DELETE CASCADE
);
