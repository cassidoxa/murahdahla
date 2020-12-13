-- Drops the new tables and restores the old ones
SET foreign_key_checks = 0;

DROP TABLE servers;
DROP TABLE channels;
DROP TABLE async_races;
DROP TABLE messages;
DROP TABLE submissions;
