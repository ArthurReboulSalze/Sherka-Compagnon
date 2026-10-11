CREATE TABLE profile (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    id TEXT NOT NULL UNIQUE,
    revision INTEGER NOT NULL CHECK(revision > 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    details TEXT NOT NULL,
    avatar BLOB
);
CREATE TABLE messages (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id TEXT NOT NULL UNIQUE,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    generation_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    delivery TEXT NOT NULL,
    heard_content TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    extraction_allowed INTEGER NOT NULL DEFAULT 1 CHECK(extraction_allowed IN (0,1))
);
CREATE INDEX messages_session ON messages(session_id, seq);
CREATE TABLE memories (
    id INTEGER PRIMARY KEY,
    content TEXT NOT NULL,
    category TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active','obsolete','deleted')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    source_message_id TEXT REFERENCES messages(id),
    revision INTEGER NOT NULL DEFAULT 1,
    operation_key TEXT NOT NULL UNIQUE
);
CREATE VIRTUAL TABLE memories_fts USING fts5(content, category, content='memories', content_rowid='id');
CREATE TRIGGER memory_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts(rowid,content,category) VALUES(new.id,new.content,new.category);
END;
CREATE TRIGGER memory_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts,rowid,content,category) VALUES('delete',old.id,old.content,old.category);
END;
CREATE TRIGGER memory_update AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts(memories_fts,rowid,content,category) VALUES('delete',old.id,old.content,old.category);
    INSERT INTO memories_fts(rowid,content,category) VALUES(new.id,new.content,new.category);
END;
CREATE TABLE summaries (
    id TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    version INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE extraction_exclusions (
    source_message_id TEXT PRIMARY KEY REFERENCES messages(id),
    created_at INTEGER NOT NULL
);
PRAGMA user_version = 2;

