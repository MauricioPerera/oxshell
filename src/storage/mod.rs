use anyhow::{Context, Result};
use minimemory::{Config, Distance, VectorDB};
use std::path::{Path, PathBuf};

use crate::llm::embeddings::EMBEDDING_DIM;

/// Conversation history storage (separate from the memory system).
/// Stores raw messages for session persistence and search.
pub struct ConversationStore {
    db: VectorDB,
    data_dir: PathBuf,
}

fn make_config() -> Config {
    Config::new(EMBEDDING_DIM).with_distance(Distance::Cosine)
}

impl ConversationStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("conversations.mmdb");

        let db = if db_path.exists() {
            VectorDB::open(&db_path).unwrap_or_else(|e| {
                let backup = db_path.with_extension("mmdb.bak");
                tracing::warn!("Conversations DB corrupted: {e}. Backing up to {backup:?}");
                let _ = std::fs::rename(&db_path, &backup);
                VectorDB::new(make_config()).expect("Failed to create conversations DB")
            })
        } else {
            VectorDB::new(make_config()).context("Failed to create conversations DB")?
        };

        Ok(Self {
            db,
            data_dir: data_dir.to_path_buf(),
        })
    }

    pub fn flush(&self) -> Result<()> {
        self.db
            .save(&self.data_dir.join("conversations.mmdb"))?;
        Ok(())
    }
}
