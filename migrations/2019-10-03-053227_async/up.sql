CREATE TABLE leaderboard (
    runner_id BIGINT UNSIGNED NOT NULL PRIMARY KEY,
    game_id INT UNSIGNED NOT NULL,
    runner_name VARCHAR(32) NOT NULL,
    runner_time TIME NOT NULL,
    runner_collection TINYINT(3) UNSIGNED NOT NULL,
    runner_forfeit BOOLEAN NOT NULL
);

CREATE TABLE games (
    game_id INT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    game_date DATE NOT NULL,
    guild_id BIGINT UNSIGNED NOT NULL,
    game_active BOOLEAN NOT NULL
);

CREATE TABLE posts (
    post_id BIGINT UNSIGNED PRIMARY KEY,
    post_time TIME NOT NULL,
    game_id INT UNSIGNED NOT NULL,
    guild_id BIGINT UNSIGNED NOT NULL,
    guild_channel BIGINT UNSIGNED NOT NULL
);
