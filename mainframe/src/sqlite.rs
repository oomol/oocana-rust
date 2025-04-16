use rusqlite;
use serde::{Deserialize, Serialize};

pub struct SQLite {
    db: rusqlite::Connection,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub manifest_path: String,
    pub session_id: String,
    pub node_id: Option<String>,
    pub event_type: String,
}

const EVENT_TABLE: &str = "oocana_event";

impl SQLite {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let db = rusqlite::Connection::open(db_path)?;
        Ok(SQLite { db })
    }

    pub fn setup(&self) -> rusqlite::Result<()> {
        self._setup_oocana_event()?;
        self._setup_oocana_event_indexes()?;
        Ok(())
    }

    pub fn insert(&self, event: &Event) -> rusqlite::Result<()> {
        let full_msg = serde_json::to_string(event)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        // skip some full_msg >= 512 kB only some event will bigger than 512 kB
        if full_msg.len() > 512 * 1024 {
            return Ok(());
        }

        self.db.execute(
            &format!(
                "INSERT INTO {} (manifest_path, session_id, node_id, type, event) VALUES (?, ?, ?, ?, ?)",
                EVENT_TABLE
            ),
            rusqlite::params![event.manifest_path, event.session_id, event.node_id, event.event_type, full_msg],
        )?;
        Ok(())
    }

    fn _setup_oocana_event(&self) -> rusqlite::Result<()> {
        self.db.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    manifest_path TEXT NOT NULL,
                    session_id TEXT NOT NULL,
                    node_id TEXT,
                    type TEXT NOT NULL,
                    event TEXT NOT NULL,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )",
                EVENT_TABLE
            ),
            [],
        )?;

        Ok(())
    }

    fn _setup_oocana_event_indexes(&self) -> rusqlite::Result<()> {
        // Composite index: Query events by session and creation time
        self.db.execute(
            "CREATE INDEX IF NOT EXISTS oocana_event_idx_session_created ON oocana_event (session_id, created_at)",
            [],
        )?;

        // Composite index: Query flows by project
        self.db.execute(
            "CREATE INDEX IF NOT EXISTS oocana_event_idx_flow ON oocana_event (flow_path)",
            [],
        )?;

        // Type index: Filter by type
        self.db.execute(
            "CREATE INDEX IF NOT EXISTS oocana_event_idx_type ON oocana_event (type)",
            [],
        )?;

        // Composite index: Query node_id by session and manifest path
        self.db.execute(
            "CREATE INDEX IF NOT EXISTS oocana_event_idx_session_manifest ON oocana_event (session_id, manifest_path)",
            [],
        )?;

        Ok(())
    }
}
