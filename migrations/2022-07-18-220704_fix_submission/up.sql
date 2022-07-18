SET @SubmissionConstraint = (SELECT CONSTRAINT_NAME
    FROM INFORMATION_SCHEMA.KEY_COLUMN_USAGE
    WHERE TABLE_NAME='submissions' AND COLUMN_NAME='race_id');

SET @DelStmt = CONCAT('ALTER TABLE submissions DROP FOREIGN KEY ', @SubmissionConstraint);
PREPARE stmt FROM @DelStmt;
EXECUTE stmt;

ALTER TABLE submissions ADD FOREIGN KEY (race_id)
    REFERENCES async_races(race_id)
    ON DELETE CASCADE;
