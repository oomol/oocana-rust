use std::{collections::HashMap, path::Path};

use crate::reporter::ReporterMessage;
use rusqlite;
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Debug)]
pub struct SQLite {
    db: rusqlite::Connection,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Event {
    pub manifest_path: String,
    pub session_id: String,
    pub node_id: Option<String>,
    pub event_type: String,
    pub event: String,
}

impl From<ReporterMessage<'_>> for Event {
    fn from(msg: ReporterMessage) -> Self {
        match msg {
            ReporterMessage::SessionStarted(msg) => Event {
                manifest_path: msg.path.clone().to_owned(),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("SessionStarted"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::SessionFinished(msg) => Event {
                manifest_path: msg.path.clone().to_owned(),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("SessionFinished"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::FlowStarted(msg) => Event {
                manifest_path: msg
                    .flow_path
                    .clone()
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("FlowStarted"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::FlowFinished(msg) => Event {
                manifest_path: msg
                    .flow_path
                    .clone()
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("FlowFinished"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::BlockStarted(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("BlockStarted"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::BlockFinished(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("BlockFinished"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::BlockOutput(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("BlockOutput"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::BlockError(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("BlockError"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::BlockLog(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("BlockLog"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::SubflowBlockStarted(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("SubflowBlockStarted"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::SubflowBlockFinished(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("SubflowBlockFinished"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::SubflowBlockOutput(msg) => Event {
                manifest_path: msg
                    .stacks
                    .get(0)
                    .map(|s| s.flow.clone())
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("SubflowBlockOutput"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
            ReporterMessage::FlowNodesWillRun(msg) => Event {
                manifest_path: msg
                    .flow_path
                    .clone()
                    .unwrap_or_else(|| String::from("missing path")),
                session_id: msg.session_id.clone().to_owned(),
                node_id: None,
                event_type: String::from("FlowNodesWillRun"),
                event: serde_json::to_string(&msg).unwrap_or_else(|_| String::from("{}")),
            },
        }
    }
}

const EVENT_TABLE: &str = "oocana_event";

impl SQLite {
    pub fn new<P: AsRef<Path>>(db_path: P) -> rusqlite::Result<Self> {
        let db = rusqlite::Connection::open(db_path)?;
        Ok(SQLite { db })
    }

    pub fn setup(&self) -> rusqlite::Result<()> {
        self._setup_oocana_event()?;
        self._setup_oocana_event_indexes()?;
        Ok(())
    }

    pub fn insert(&self, msg: HashMap<String, serde_json::Value>) -> rusqlite::Result<()> {
        let event: Event = msg.clone().try_into()?;

        let full_msg = serde_json::to_string(&msg)
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
