CREATE TABLE IF NOT EXISTS avatar_emotions (
    emotion TEXT PRIMARY KEY CHECK(emotion IN ('joy','sadness','anger','surprise','reflection','tenderness')),
    image BLOB
);
PRAGMA user_version = 6;
