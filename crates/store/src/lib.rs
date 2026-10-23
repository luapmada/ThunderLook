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
const SCHEMA_VERSION: i32 = 11;

/// Schéma v1. Les dates sont en millisecondes depuis l'époque Unix (UTC).
/// `props` : sac de propriétés MAPI brut (format décrit dans `thunderlook-import-pst::propbag`),
/// pour reprendre plus tard toute propriété non convertie sans réimporter.
const SCHEMA_V1: &str = r#"
CREATE TABLE source (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,              -- 'pst' ou 'imap' (J7)
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

/// v2 (J2) : index couvrant pour les compteurs de dossiers et l'ouverture d'une liste
/// (non-lus, date) sans lire les lignes, dont les sacs MAPI rendent le parcours coûteux.
const SCHEMA_V2: &str = r#"
CREATE INDEX IF NOT EXISTS message_folder_read ON message(folder_id, is_read, received_at);
"#;

/// v3 (J3) : favoris du volet des dossiers. Vide = favoris par défaut (réception et envoyés de la
/// première source).
const SCHEMA_V3: &str = r#"
CREATE TABLE IF NOT EXISTS favorite (
    folder_id   INTEGER PRIMARY KEY REFERENCES folder(id) ON DELETE CASCADE,
    position    INTEGER NOT NULL
);
"#;

/// v4 (J4) : recherche.
/// - `message_fts` : index plein texte sans contenu stocké (le texte reste dans les `.eml`), insensible aux
///   accents, avec index de préfixes ; `rowid` = `message.id`. Rempli en tâche de fond (`message.indexed`).
/// - Index sur l'adresse de l'expéditeur, pour retrouver l'historique d'un correspondant (ADI Facturation).
/// - `search_folder` : dossiers de recherche enregistrés (requête et portée).
const SCHEMA_V4: &str = r#"
ALTER TABLE message ADD COLUMN indexed INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS message_unindexed ON message(id) WHERE indexed = 0;
CREATE INDEX IF NOT EXISTS message_from_addr ON message(from_addr COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS message_flagged ON message(folder_id, received_at) WHERE flag_status = 2;
CREATE VIRTUAL TABLE IF NOT EXISTS message_fts USING fts5(
    subject, sender, recipients, body, attachments,
    content = '', contentless_delete = 1,
    tokenize = 'unicode61 remove_diacritics 2', prefix = '2 3'
);
CREATE TABLE IF NOT EXISTS search_folder (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    query       TEXT NOT NULL,
    scope       TEXT NOT NULL,              -- 'all' ou 'source:<id>'
    position    INTEGER NOT NULL
);
"#;

/// v5 (J3b) : conversations. `conversation_id` = `PidTagConversationId` calculé ([`conversation_id`]) ;
/// vide quand le message n'a pas de conversation, NULL tant qu'il n'a pas été calculé (bases antérieures,
/// complétées par `thunderlook-mailbox` à l'ouverture).
const SCHEMA_V5: &str = r#"
ALTER TABLE message ADD COLUMN conversation_id BLOB;
CREATE INDEX IF NOT EXISTS message_folder_conv ON message(folder_id, received_at, conversation_id, is_read);
CREATE INDEX IF NOT EXISTS message_conv ON message(conversation_id) WHERE length(conversation_id) = 16;
"#;

/// v6 (J3b) : objets importés avec le marqueur de préfixe du PST ([`strip_subject_marker`]).
const SCHEMA_V6: &str = r#"
UPDATE message SET subject = substr(subject, 3) WHERE substr(subject, 1, 1) = char(1) AND length(subject) >= 2;
UPDATE fai SET subject = substr(subject, 3) WHERE substr(subject, 1, 1) = char(1) AND length(subject) >= 2;
"#;

/// v7 (J6) : règles côté client, lues dans le flux `PR_RW_RULES_STREAM` d'un PST ou dans un export `.rwz`.
/// - `origin` : `pst:<fai.id>` ou chemin du `.rwz` ; une origine forme un jeu de règles, ordonné par `position`.
/// - `version` : version du flux (`RW_Magic`) ; `rule_json` : règle décodée (`thunderlook-rules`) ;
///   `raw` : octets de la règle dans le flux, pour la relire avec un décodeur plus récent.
/// - `active` fait foi : la case de la boîte « Règles et alertes » ne réécrit ni `rule_json` ni `raw`.
const SCHEMA_V7: &str = r#"
CREATE TABLE IF NOT EXISTS rule (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,              -- 'pst' ou 'rwz'
    source_id   INTEGER REFERENCES source(id) ON DELETE CASCADE,
    origin      TEXT NOT NULL,
    position    INTEGER NOT NULL,
    name        TEXT NOT NULL,
    active      INTEGER NOT NULL,
    version     INTEGER NOT NULL,
    rule_json   TEXT NOT NULL,
    raw         BLOB NOT NULL,
    imported_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS rule_origin ON rule(origin, position);
"#;

/// v8 (J7) : comptes IMAP + SMTP. Un compte est une source de type `imap` ; le mot de passe n'est jamais en
/// base (Gestionnaire d'identification de Windows).
/// - `folder.imap_name` : nom de la boîte aux lettres sur le serveur (NULL : dossier local, comme la boîte
///   d'envoi) ; `imap_uidvalidity`, `imap_uidnext`, `imap_modseq` : état de la dernière synchronisation.
/// - `message.imap_uid` : UID du message dans sa boîte (NULL : pas encore sur le serveur).
/// - `imap_pending` : changements faits hors connexion ou pas encore reportés sur le serveur, dans l'ordre.
/// - `outbox` : messages de la boîte d'envoi et état de leur envoi.
const SCHEMA_V8: &str = r#"
CREATE TABLE IF NOT EXISTS account (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL UNIQUE REFERENCES source(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,              -- nom de l'expéditeur
    email       TEXT NOT NULL,
    username    TEXT NOT NULL,
    imap_host   TEXT NOT NULL,
    imap_port   INTEGER NOT NULL,
    imap_security TEXT NOT NULL,            -- 'tls', 'starttls' ou 'none' (machine locale seulement)
    smtp_host   TEXT NOT NULL,
    smtp_port   INTEGER NOT NULL,
    smtp_security TEXT NOT NULL,
    save_sent   INTEGER NOT NULL,           -- dépôt d'une copie dans « Éléments envoyés » du serveur
    created_at  INTEGER NOT NULL,
    last_sync   INTEGER,
    last_error  TEXT                        -- dernière erreur de relève ou d'envoi (sans secret)
);
CREATE UNIQUE INDEX IF NOT EXISTS message_imap_uid ON message(folder_id, imap_uid) WHERE imap_uid IS NOT NULL;
CREATE TABLE IF NOT EXISTS imap_pending (
    id          INTEGER PRIMARY KEY,
    account_id  INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,              -- 'flags', 'move', 'copy', 'expunge', 'append', 'mkdir', 'rename', 'rmdir'
    mailbox     TEXT NOT NULL,              -- boîte concernée (nom IMAP)
    uid         INTEGER,
    message_id  INTEGER,                    -- message local concerné (append, move : reçoit le nouvel UID)
    arg         TEXT NOT NULL,              -- JSON propre à chaque type
    created_at  INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    last_error  TEXT
);
CREATE TABLE IF NOT EXISTS outbox (
    message_id  INTEGER PRIMARY KEY REFERENCES message(id) ON DELETE CASCADE,
    account_id  INTEGER NOT NULL REFERENCES account(id) ON DELETE CASCADE,
    queued_at   INTEGER NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 0,
    last_error  TEXT
);
"#;

/// Colonnes du v8, ajoutées avant `SCHEMA_V8` et seulement si elles manquent : la migration peut être rejouée
/// (bases de test ramenées à un schéma antérieur) sans « duplicate column ».
const V8_COLUMNS: &[(&str, &str, &str)] = &[
    ("folder", "imap_name", "TEXT"),
    ("folder", "imap_uidvalidity", "INTEGER"),
    ("folder", "imap_uidnext", "INTEGER"),
    ("folder", "imap_modseq", "INTEGER"),
    ("message", "imap_uid", "INTEGER"),
];

/// v9 : règles à l'arrivée par compte (désactivées par défaut : pendant la transition, Outlook et ThunderLook
/// relèvent les mêmes boîtes IMAP et les règles ne doivent tourner que d'un côté).
const V9_COLUMNS: &[(&str, &str, &str)] = &[("account", "rules_on_arrival", "INTEGER NOT NULL DEFAULT 0")];

/// v10 : dernière action sur un message reçu, comme Outlook (`PidTagLastVerbExecuted` : 102 répondu, 103 répondu à
/// tous, 104 transféré ; `PidTagLastVerbExecutionTime`, en millisecondes). Sur le serveur : `\Answered`, `$Forwarded`.
const V10_COLUMNS: &[(&str, &str, &str)] = &[("message", "last_verb", "INTEGER"), ("message", "last_verb_at", "INTEGER")];

/// v11 (lot F) : index des classes de message. `LIKE 'IPM.Appointment%'` (insensible à la casse) ne passe par un index
/// que s'il est en `COLLATE NOCASE` ; sans lui, calendrier, contacts, tâches et notes balayaient toute la table
/// `message` (courrier compris) à chaque ouverture.
const SCHEMA_V11: &str = r#"
CREATE INDEX IF NOT EXISTS message_class_nocase ON message(message_class COLLATE NOCASE, folder_id);
"#;

fn add_missing_columns(conn: &Connection, columns: &[(&str, &str, &str)]) -> rusqlite::Result<()> {
    for (table, column, decl) in columns {
        let exists: bool =
            conn.query_row(&format!("SELECT EXISTS (SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1)"), [column], |r| r.get(0))?;
        if !exists {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl};"))?;
        }
    }
    Ok(())
}

/// Objet sans le marqueur de préfixe que le PST place devant `PidTagSubject` ([MS-PST] §2.5.3.1.1.1) :
/// un premier caractère de valeur 1, puis un caractère donnant la longueur du préfixe plus un.
/// L'objet complet commence après ces deux caractères.
pub fn strip_subject_marker(subject: &str) -> &str {
    let mut chars = subject.chars();
    match (chars.next(), chars.next()) {
        (Some('\u{1}'), Some(_)) => chars.as_str(),
        _ => subject,
    }
}

/// `PidTagConversationId` ([MS-OXOMSG] §2.2.1.2) : la partie GUID de `PidTagConversationIndex` si le suivi
/// est actif (`PidTagConversationIndexTracking`) et l'en-tête valide (22 octets au moins, premier octet 0x01) ;
/// sinon le MD5 des 255 premiers caractères UTF-16LE de `PidTagConversationTopic` en majuscules ; sinon rien.
pub fn conversation_id(index: Option<&[u8]>, tracking: bool, topic: Option<&str>) -> Option<[u8; 16]> {
    use md5::{Digest, Md5};
    if let Some(index) = index.filter(|i| tracking && i.len() >= 22 && i[0] == 0x01) {
        // En-tête : 1 octet réservé, 5 octets d'horodatage, 16 octets de GUID.
        return index[6..22].try_into().ok();
    }
    let topic = topic?;
    let mut hasher = Md5::new();
    // « Premiers caractères non nuls » : la chaîne s'arrête au premier NUL.
    for unit in topic.encode_utf16().take_while(|u| *u != 0).take(255) {
        // Majuscules sans dépendre de la langue (« i » donne « I » partout) ; une unité UTF-16 reste une unité.
        let upper = char::from_u32(u32::from(unit))
            .map(|c| {
                let mut up = c.to_uppercase();
                match (up.next(), up.next()) {
                    (Some(u), None) if (u as u32) <= 0xFFFF => u as u16,
                    _ => unit,
                }
            })
            .unwrap_or(unit);
        hasher.update(upper.to_le_bytes());
    }
    Some(hasher.finalize().into())
}

/// Ouvre (ou crée) la base du dossier de données et applique les migrations.
pub fn open(data_dir: &Path) -> rusqlite::Result<Connection> {
    std::fs::create_dir_all(data_dir).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let conn = Connection::open(data_dir.join(DB_FILE))?;
    // Attente d'un verrou plutôt qu'un échec immédiat : l'indexation écrit en tâche de fond. WAL + `synchronous = NORMAL` :
    // les lecteurs ne bloquent pas l'écrivain, et la base reste cohérente après une coupure (seules les toutes dernières
    // transactions peuvent être perdues).
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 8000;")?;
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version == 0 {
        conn.execute_batch(&format!("BEGIN; {SCHEMA_V1} {SCHEMA_V2} {SCHEMA_V3} {SCHEMA_V4} {SCHEMA_V5} {SCHEMA_V6} {SCHEMA_V7}"))?;
        add_missing_columns(&conn, V8_COLUMNS)?;
        conn.execute_batch(SCHEMA_V8)?;
        add_missing_columns(&conn, V9_COLUMNS)?;
        add_missing_columns(&conn, V10_COLUMNS)?;
        conn.execute_batch(SCHEMA_V11)?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"))?;
    } else if version < SCHEMA_VERSION {
        // Migrations par ajout uniquement : la base est lue par d'autres outils (ADI Facturation).
        let steps = [SCHEMA_V2, SCHEMA_V3, SCHEMA_V4, SCHEMA_V5, SCHEMA_V6, SCHEMA_V7];
        let pending = steps[(version as usize - 1).min(steps.len())..].concat();
        conn.execute_batch(&format!("BEGIN; {pending}"))?;
        add_missing_columns(&conn, V8_COLUMNS)?;
        conn.execute_batch(SCHEMA_V8)?;
        add_missing_columns(&conn, V9_COLUMNS)?;
        add_missing_columns(&conn, V10_COLUMNS)?;
        conn.execute_batch(SCHEMA_V11)?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}; COMMIT;"))?;
    } else if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "base créée par une version plus récente (schéma {version})"
        )));
    }
    Ok(conn)
}

/// Ouvre une base existante en lecture seule (interface de lecture, outils tiers).
/// Les migrations ne sont pas appliquées : ouvrir d'abord avec [`open`].
pub fn open_read_only(data_dir: &Path) -> rusqlite::Result<Connection> {
    open_readonly(data_dir)
}

/// Connexion en lecture seule pour les lectures longues (sauvegarde…) : en WAL elle lit un instantané cohérent sans bloquer
/// l'écrivain, et attend au plus 8 s un verrou au lieu d'échouer (`database is locked`). À lire par paquets (transactions courtes).
pub fn open_readonly(data_dir: &Path) -> rusqlite::Result<Connection> {
    use rusqlite::OpenFlags;
    let conn = Connection::open_with_flags(
        data_dir.join(DB_FILE),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.execute_batch("PRAGMA query_only = ON; PRAGMA busy_timeout = 8000;")?;
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

/// Supprime une source et tout ce qui en dépend, avec les `.eml` des messages qui se trouvent dans
/// ses dossiers. Un message déplacé depuis cette source vers une autre garde son fichier.
pub fn delete_source(conn: &Connection, data_dir: &Path, source_id: i64) -> rusqlite::Result<()> {
    let paths: Vec<String> = conn
        .prepare("SELECT m.eml_path FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1")?
        .query_map(params![source_id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    // L'index plein texte n'a pas de clé étrangère : on retire d'abord les lignes des messages de la source.
    conn.execute(
        "DELETE FROM message_fts WHERE rowid IN (SELECT m.id FROM message m JOIN folder f ON f.id = m.folder_id WHERE f.source_id = ?1)",
        params![source_id],
    )?;
    conn.execute("DELETE FROM source WHERE id = ?1", params![source_id])?;
    for rel in paths {
        let _ = std::fs::remove_file(resolve(data_dir, &rel));
    }
    remove_empty_dirs(&data_dir.join("eml").join(source_id.to_string()));
    Ok(())
}

/// Retire les dossiers vides d'une arborescence (les autres restent).
pub fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                remove_empty_dirs(&e.path());
            }
        }
    }
    let _ = std::fs::remove_dir(dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pragmas_and_long_read_does_not_block_writer() {
        let dir = std::env::temp_dir().join(format!("tl-store-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = open(&dir).unwrap();
        let pragma = |c: &Connection, n: &str| c.query_row(&format!("PRAGMA {n}"), [], |r| r.get::<_, rusqlite::types::Value>(0)).unwrap();
        assert_eq!(pragma(&w, "journal_mode"), rusqlite::types::Value::Text("wal".into()));
        assert_eq!(pragma(&w, "synchronous"), rusqlite::types::Value::Integer(1), "NORMAL");
        assert_eq!(pragma(&w, "busy_timeout"), rusqlite::types::Value::Integer(8000));
        w.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (1, 'pst', 'a', 'a', 'A', 0, 0)", []).unwrap();
        let r = open_readonly(&dir).unwrap();
        assert_eq!(pragma(&r, "busy_timeout"), rusqlite::types::Value::Integer(8000));
        assert!(r.execute("DELETE FROM source", []).is_err(), "lecture seule");

        // Le lecteur garde une lecture ouverte pendant que l'écrivain enchaîne les transactions.
        let reader = std::thread::spawn(move || {
            r.execute_batch("BEGIN").unwrap();
            let first = r.query_row("SELECT COUNT(*) FROM source", [], |x| x.get::<_, i64>(0)).unwrap();
            for _ in 0..20 {
                let n = r.query_row("SELECT COUNT(*) FROM source", [], |x| x.get::<_, i64>(0)).unwrap();
                assert_eq!(n, first, "instantané stable pendant les écritures");
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            r.execute_batch("COMMIT").unwrap();
        });
        for i in 2..60 {
            w.execute("INSERT INTO source (id, kind, origin_path, work_path, display_name, size, imported_at) VALUES (?1, 'pst', 'a', 'a', 'A', 0, 0)", [i]).unwrap();
        }
        reader.join().unwrap();
        assert_eq!(w.query_row("SELECT COUNT(*) FROM source", [], |x| x.get::<_, i64>(0)).unwrap(), 59);
    }

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

    #[test]
    fn subject_marker() {
        assert_eq!(strip_subject_marker("\u{1}\u{5}RE: Suite"), "RE: Suite");
        assert_eq!(strip_subject_marker("\u{1}\u{1}Devis"), "Devis");
        assert_eq!(strip_subject_marker("Devis"), "Devis");
        assert_eq!(strip_subject_marker("\u{1}"), "\u{1}");
        assert_eq!(strip_subject_marker(""), "");
    }

    #[test]
    fn conversation_ids() {
        let mut index = vec![0x01, 1, 2, 3, 4, 5];
        index.extend(0x10u8..0x20);
        index.extend([0xAA; 5]); // un bloc de réponse
        let guid: [u8; 16] = std::array::from_fn(|i| 0x10 + i as u8);
        assert_eq!(conversation_id(Some(&index), true, Some("x")), Some(guid));
        // Sans suivi, ou en-tête invalide : le sujet décide, sans tenir compte de la casse.
        let by_topic = conversation_id(None, false, Some("Devis 2026"));
        assert_eq!(conversation_id(Some(&index), false, Some("DEVIS 2026")), by_topic);
        assert_eq!(conversation_id(Some(&index[..21]), true, Some("devis 2026")), by_topic);
        index[0] = 0x02;
        assert_eq!(conversation_id(Some(&index), true, Some("Devis 2026")), by_topic);
        assert_ne!(conversation_id(None, false, Some("Devis 2027")), by_topic);
        // MD5 de « ABC » en UTF-16LE (41 00 42 00 43 00).
        let abc = conversation_id(None, false, Some("abc")).unwrap();
        assert_eq!(abc[..4], [0x71, 0x67, 0x74, 0xb5]);
        assert_eq!(conversation_id(None, true, None), None);
    }
}
