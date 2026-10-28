ALTER TABLE messages ADD COLUMN speaker_id TEXT;
ALTER TABLE messages ADD COLUMN speaker_name TEXT;
PRAGMA user_version = 10;
