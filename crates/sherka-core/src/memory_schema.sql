ALTER TABLE memories ADD COLUMN kind TEXT NOT NULL DEFAULT 'detail';
ALTER TABLE memories ADD COLUMN approved INTEGER NOT NULL DEFAULT 1;
ALTER TABLE memories ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual';
CREATE TABLE memory_sources (
    memory_id INTEGER NOT NULL REFERENCES memories(id),
    message_id TEXT NOT NULL REFERENCES messages(id),
    PRIMARY KEY(memory_id,message_id)
);
INSERT INTO memory_sources SELECT id,source_message_id FROM memories WHERE source_message_id IS NOT NULL;
CREATE TABLE memory_meta (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    epoch INTEGER NOT NULL DEFAULT 0,
    cursor INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);
INSERT INTO memory_meta(singleton) VALUES(1);
CREATE TABLE summary_sources (
    summary_id TEXT NOT NULL REFERENCES summaries(id) ON DELETE CASCADE,
    message_id TEXT NOT NULL REFERENCES messages(id),
    PRIMARY KEY(summary_id,message_id)
);
CREATE VIRTUAL TABLE messages_fts USING fts5(content, content='messages', content_rowid='seq');
INSERT INTO messages_fts(messages_fts) VALUES('rebuild');
CREATE TRIGGER message_insert AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid,content) VALUES(new.seq,new.content);
END;
CREATE TRIGGER message_delete AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts,rowid,content) VALUES('delete',old.seq,old.content);
END;
CREATE TRIGGER message_update AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts,rowid,content) VALUES('delete',old.seq,old.content);
    INSERT INTO messages_fts(rowid,content) VALUES(new.seq,new.content);
END;
PRAGMA user_version = 3;
