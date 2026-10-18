//! Stockage local de ThunderLook (décision Q2) : une base SQLite pour l'index et les métadonnées,
//! plus un fichier `.eml` par message sous `eml/`. Aucun PST n'est jamais écrit.
//!
//! Disposition du dossier de données :
//! - `thunderlook.db` : base (schéma ci-dessous, version dans `PRAGMA user_version`) ;
//! - `eml/<source>/<lot>/<message>.eml` : messages, `lot` = identifiant / 1000 ;
//! - `sources/` : copies de travail des PST importés ;
//! - `reports/` : rapports d'import en JSON.

use std::path::{Path, PathBuf};

pub use rusqlite;
use rusqlite::{Connection, OptionalExtension, params};

pub const DB_FILE: &str = "thunderlook.db";
const SCHEMA_VERSION: i32 = 1;

/// Schéma v1. Les dates sont en millisecondes depuis l'époque Unix (UTC).
/// `props` : sac de propriétés MAPI brut (format décrit dans `thunderlook-import-pst::propbag`),
/// pour reprendre plus tard toute propriété non convertie sans réimporter.
const SCHEMA_V1: &str = r#"
CREATE TABLE source (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,              -- 'pst'
    origin_path TEXT NOT NULL,              -- fichier choisi par l'utilisateur
    work_path   TEXT NOT NULL,              -- copie de travail réellement lue
    display_name TEXT NOT NULL,
    size        INTEGER NOT NULL,
    imported_at INTEGER NOT NULL,
    props       BLOB
);
CREATE TABLE folder (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES source(id) ON DELETE CASCADE,
    parent_id   INTEGER REFERENCES folder(id) ON DELETE CASCADE,
    nid         INTEGER NOT NULL,           -- identifiant de nœud dans le PST
    name        TEXT NOT NULL,
    role        TEXT NOT NULL,              -- inbox, drafts, sent, deleted, junk, outbox, ipm, root, folder…
    container_class TEXT,
    hidden      INTEGER NOT NULL,           -- hors de la sous-arborescence IPM (vues, recherche…)
    position    INTEGER NOT NULL,
    pst_content_count INTEGER,
    pst_unread_count  INTEGER,
    props       BLOB
);
CREATE INDEX folder_parent ON folder(parent_id);
CREATE TABLE message (
    id          INTEGER PRIMARY KEY,
    folder_id   INTEGER NOT NULL REFERENCES folder(id) ON DELETE CASCADE,
    nid         INTEGER NOT NULL,
    message_class TEXT NOT NULL,
    subject     TEXT NOT NULL,
    from_name   TEXT NOT NULL,
    from_addr   TEXT NOT NULL,
    to_line     TEXT NOT NULL,
    cc_line     TEXT NOT NULL,
    sent_at     INTEGER,
    received_at INTEGER,
    created_at  INTEGER,
    modified_at INTEGER,
    message_flags INTEGER NOT NULL,
    is_read     INTEGER NOT NULL,
    flag_status INTEGER NOT NULL,           -- 0 aucun, 1 terminé, 2 marqué ([MS-OXOFLAG])
    flag_request TEXT,
    flag_due    INTEGER,
    flag_completed INTEGER,
    importance  INTEGER NOT NULL,           -- 0 basse, 1 normale, 2 haute
    sensitivity INTEGER NOT NULL,
    size        INTEGER NOT NULL,
    attachment_count INTEGER NOT NULL,
    body_kind   TEXT NOT NULL,              -- html, text, rtf-html, rtf-text, rtf, none
    preview     TEXT NOT NULL,
    internet_message_id TEXT,
    in_reply_to TEXT,
    conversation_topic TEXT,
    conversation_index BLOB,
    eml_path    TEXT NOT NULL,              -- relatif au dossier de données
    props       BLOB NOT NULL
);
CREATE INDEX message_folder_date ON message(folder_id, received_at DESC);
CREATE TABLE attachment (
    id          INTEGER PRIMARY KEY,
    message_id  INTEGER NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    filename    TEXT NOT NULL,
    mime_type   TEXT NOT NULL,
    size        INTEGER NOT NULL,
    method      INTEGER NOT NULL,           -- PidTagAttachMethod
    content_id  TEXT,
    inline      INTEGER NOT NULL,
    hidden      INTEGER NOT NULL,
    props       BLOB NOT NULL
);
CREATE INDEX attachment_message ON attachment(message_id);
CREATE TABLE message_category (
    message_id  INTEGER NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    PRIMARY KEY (message_id, name)
) WITHOUT ROWID;
CREATE TABLE category (
    source_id   INTEGER NOT NULL REFERENCES source(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       INTEGER NOT NULL,           -- index de couleur 0-24, -1 = aucune
    shortcut    INTEGER,
    guid        TEXT,
    PRIMARY KEY (source_id, name)
) WITHOUT ROWID;
-- Contenus associés (FAI) : règles, vues, catégories, favoris… conservés bruts pour J3/J6.
CREATE TABLE fai (
    id          INTEGER PRIMARY KEY,
    folder_id   INTEGER NOT NULL REFERENCES folder(id) ON DELETE CASCADE,
    nid         INTEGER NOT NULL,
    message_class TEXT NOT NULL,
    subject     TEXT NOT NULL,
    props       BLOB NOT NULL
);
CREATE INDEX fai_class ON fai(message_class);
-- Tout ce qui n'a pas pu être importé ou converti : rien ne se perd en silence.
CREATE TABLE import_issue (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES source(id) ON DELETE CASCADE,
    folder_path TEXT NOT NULL,
    nid         INTEGER,
    severity    TEXT NOT NULL,              -- 'lost' : élément absent ; 'degraded' : importé incomplet
    kind        TEXT NOT NULL,
    detail      TEXT NOT NULL
);
CREATE TABLE import_run (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL REFERENCES source(id) ON DELETE CASCADE,
    started_at  INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    report_json TEXT NOT NULL
);
"#;

/// Ouvre (ou crée) la base du dossier de données et applique les migrations.
pub fn open(data_dir: &Path) -> rusqlite::Result<Connection> {
    std::fs::create_dir_all(data_dir).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let conn = Connection::open(data_dir.join(DB_FILE))?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 0 {
        conn.execute_batch(&format!("BEGIN; {SCHEMA_V1} PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"))?;
    } else if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "base créée par une version plus récente (schéma {version})"
        )));
    }
    Ok(conn)
}

/// Chemin relatif du `.eml` d'un message (séparateur `/`, indépendant de la plateforme).
pub fn eml_relative_path(source_id: i64, message_id: i64) -> String {
    format!("eml/{source_id}/{:04}/{message_id}.eml", message_id / 1000)
}

pub fn resolve(data_dir: &Path, relative: &str) -> PathBuf {
    data_dir.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR))
}

/// Source déjà importée depuis ce fichier d'origine, s'il y en a une.
pub fn find_source(conn: &Connection, origin_path: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT id FROM source WHERE origin_path = ?1", params![origin_path], |r| r.get(0))
        .optional()
}

/// Supprime une source et tout ce qui en dépend (les `.eml` sont à effacer par l'appelant).
pub fn delete_source(conn: &Connection, source_id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM source WHERE id = ?1", params![source_id]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_applies_once() {
        let dir = std::env::temp_dir().join(format!("tl-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conn = open(&dir).unwrap();
        drop(conn);
        let conn = open(&dir).unwrap();
        let v: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, SCHEMA_VERSION);
        assert_eq!(eml_relative_path(3, 12345), "eml/3/0012/12345.eml");
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
