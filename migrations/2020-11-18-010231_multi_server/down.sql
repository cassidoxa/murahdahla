-- Drops the new tables and restores the old ones
SET foreign_key_checks = 0;

DROP TABLE servers;
DROP TABLE channels;
DROP TABLE async_races;
DROP TABLE messages;
DROP TABLE submissions;

CREATE TABLE games(
    game_id INT(10) UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    game_date DATE NOT NULL,
    guild_id BIGINT(20) UNSIGNED NOT NULL,
    game_active TINYINT(1) NOT NULL
);

CREATE TABLE posts(
    post_id BIGINT(20) UNSIGNED PRIMARY KEY,
    post_datetime DATETIME NOT NULL,
    game_id INT(10) UNSIGNED NOT NULL,
    guild_id BIGINT(20) UNSIGNED NOT NULL,
    guild_chanel BIGINT(20) UNSIGNED NOT NULL
);

CREATE TABLE leaderboard(
    runner_id BIGINT(20) UNSIGNED PRIMARY KEY,
    game_id INT(10) UNSIGNED NOT NULL,
    runner_name VARCHAR(32) NOT NULL,
    runner_time TIME NOT NULL,
    runner_collection SMALLINT(3) UNSIGNED NOT NULL,
    runner_forfeit TINYINT(1) NOT NULL,
    submission_datetime DATETIME NOT NULL
);

