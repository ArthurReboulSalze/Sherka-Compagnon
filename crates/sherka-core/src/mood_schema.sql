CREATE TABLE IF NOT EXISTS mood_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    epoch INTEGER NOT NULL DEFAULT 0,
    data TEXT NOT NULL DEFAULT '{}'
);
INSERT OR IGNORE INTO mood_state(singleton) VALUES(1);
PRAGMA user_version = 9;
