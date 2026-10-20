//! Règles côté client (J6) : import du flux `PR_RW_RULES_STREAM` des PST et des exports `.rwz` dans la table
//! `rule` (schéma v7), liste pour la boîte « Règles et alertes ».

use std::path::Path;

use thunderlook_model::RuleSummary;
use thunderlook_rules::{Rule, RuleStream};
use thunderlook_store::rusqlite::{Connection, OptionalExtension, params};

use crate::{Mailbox, Result, conv::bag_value, err, parse_id};

/// Classe du contenu associé de la Boîte de réception qui porte le flux.
const RULE_ORGANIZER: &str = "IPM.RuleOrganizer";
/// `PR_RW_RULES_STREAM`.
const RULES_STREAM: u16 = 0x6802;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// Remplace le jeu de règles `origin` par celles du flux.
fn store_stream(conn: &Connection, kind: &str, source_id: Option<i64>, origin: &str, stream: &RuleStream) -> Result<usize> {
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute("DELETE FROM rule WHERE origin = ?1", params![origin]).map_err(err)?;
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO rule (kind, source_id, origin, position, name, active, version, rule_json, raw, imported_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .map_err(err)?;
        let at = now_ms();
        for (k, rule) in stream.rules.iter().enumerate() {
            let json = serde_json::to_string(rule).map_err(err)?;
            insert
                .execute(params![kind, source_id, origin, k as i64, rule.name, rule.active, stream.version, json, rule.raw, at])
                .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(stream.rules.len())
}

/// Importe les flux de règles des PST qui ne l'ont pas encore été (ouverture du dossier de données).
/// Un flux déjà importé n'est pas relu : l'état des cases « active » reste celui choisi dans ThunderLook.
/// Renvoie le nombre de règles importées ; un flux illisible est ignoré (il reste dans `fai.props`).
pub(crate) fn import_pst_rules(conn: &Connection) -> Result<usize> {
    let pending: Vec<(i64, i64, Vec<u8>)> = conn
        .prepare(
            "SELECT x.id, f.source_id, x.props FROM fai x JOIN folder f ON f.id = x.folder_id
             WHERE x.message_class = ?1 AND NOT EXISTS (SELECT 1 FROM rule r WHERE r.origin = 'pst:' || x.id)",
        )
        .map_err(err)?
        .query_map(params![RULE_ORGANIZER], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(err)?
        .collect::<std::result::Result<_, _>>()
        .map_err(err)?;
    let mut n = 0;
    for (fai, source, props) in pending {
        let Some(bytes) = bag_value(&props, RULES_STREAM, 0x0102) else { continue };
        if let Ok(stream) = thunderlook_rules::parse(bytes) {
            n += store_stream(conn, "pst", Some(source), &format!("pst:{fai}"), &stream)?;
        }
    }
    Ok(n)
}

/// Libellé d'un jeu de règles : nom du fichier de données pour un PST, nom du fichier pour un `.rwz`.
fn origin_label(conn: &Connection, origin: &str, source_id: Option<i64>) -> String {
    let source_name = || {
        let id = source_id?;
        conn.query_row("SELECT display_name FROM source WHERE id = ?1", params![id], |r| r.get::<_, String>(0)).optional().ok().flatten()
    };
    match origin.starts_with("pst:").then(source_name).flatten() {
        Some(name) => name,
        None => Path::new(origin).file_name().map_or_else(|| origin.to_string(), |n| n.to_string_lossy().into_owned()),
    }
}

impl Mailbox {
    /// Règles de tous les jeux : ceux des PST d'abord, puis les `.rwz`, chacun dans son ordre.
    pub fn rules(&self) -> Result<Vec<RuleSummary>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, origin, source_id, position, name, active, rule_json FROM rule ORDER BY kind, origin, position")
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, bool>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })
            .map_err(err)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, origin, source, position, name, active, json) = row.map_err(err)?;
            let (description, error) = match serde_json::from_str::<Rule>(&json) {
                Ok(rule) => (thunderlook_rules::describe(&rule), rule.error),
                Err(e) => (Vec::new(), Some(format!("règle illisible : {e}"))),
            };
            out.push(RuleSummary {
                id: format!("r{id}"),
                origin_label: origin_label(&self.conn, &origin, source),
                origin,
                position: position as u32,
                name,
                active,
                description,
                error,
            });
        }
        Ok(out)
    }

    /// Importe un export `.rwz` ; un nouvel import du même fichier remplace ses règles.
    pub fn import_rwz(&mut self, path: &Path) -> Result<usize> {
        let bytes = std::fs::read(path).map_err(|e| format!("{} : {e}", path.display()))?;
        let stream = thunderlook_rules::parse(&bytes).map_err(|e| format!("{} : {e}", path.display()))?;
        let origin = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned();
        store_stream(&self.conn, "rwz", None, &origin, &stream)
    }

    pub fn set_rule_active(&mut self, id: &str, active: bool) -> Result<()> {
        let n = self.conn.execute("UPDATE rule SET active = ?2 WHERE id = ?1", params![parse_id(id, 'r')?, active]).map_err(err)?;
        if n == 0 { Err(format!("règle introuvable : {id}")) } else { Ok(()) }
    }
}
