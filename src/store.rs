use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    Endpoint, Envelope, Field, FieldLoc, GlobalSettings, ManualSource, Project, RequestLog, Scene,
    SceneKind,
};

const LOG_CAP: i64 = 500;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    port INTEGER NOT NULL,
    success_code TEXT NOT NULL,
    fail_code TEXT NOT NULL,
    default_headers TEXT NOT NULL,
    envelope_code TEXT NOT NULL DEFAULT 'code',
    envelope_msg TEXT NOT NULL DEFAULT 'msg',
    envelope_data TEXT NOT NULL DEFAULT 'data'
);

CREATE TABLE IF NOT EXISTS endpoints (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    notes TEXT NOT NULL,
    source_text TEXT NOT NULL,
    deprecated INTEGER NOT NULL,
    enabled INTEGER NOT NULL,
    current_scene TEXT NOT NULL,
    UNIQUE (project_id, method, path),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS fields (
    id TEXT PRIMARY KEY,
    endpoint_id TEXT NOT NULL,
    location TEXT NOT NULL,
    name TEXT NOT NULL,
    name_zh TEXT NOT NULL,
    type_name TEXT NOT NULL,
    required INTEGER NOT NULL,
    comment TEXT NOT NULL,
    enum_values TEXT NOT NULL,
    parent_id TEXT,
    FOREIGN KEY (endpoint_id) REFERENCES endpoints(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS scenes (
    endpoint_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    http_status INTEGER NOT NULL,
    body_json TEXT NOT NULL,
    PRIMARY KEY (endpoint_id, kind),
    FOREIGN KEY (endpoint_id) REFERENCES endpoints(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    selected_source_id TEXT NOT NULL,
    selected_model TEXT NOT NULL,
    manual_base_url TEXT NOT NULL,
    manual_api_key TEXT NOT NULL,
    manual_protocol TEXT NOT NULL,
    manual_model TEXT NOT NULL,
    manual_sources TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE IF NOT EXISTS request_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL,
    at TEXT NOT NULL,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    req_headers TEXT NOT NULL,
    req_body TEXT NOT NULL,
    hit INTEGER NOT NULL,
    endpoint_id TEXT,
    scene TEXT,
    status INTEGER NOT NULL,
    res_body TEXT NOT NULL,
    elapsed_ms INTEGER NOT NULL,
    missing_default_headers TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
);
"#;

pub struct Store {
    conn: Connection,
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                // Application Support/Mocker may not exist yet.
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("create sqlite dir {}", parent.display()))?;
            }
        }
        let conn =
            Connection::open(path).with_context(|| format!("open sqlite {}", path.display()))?;
        Self::from_conn(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_conn(Connection::open_in_memory().context("open sqlite memory")?)
    }

    fn from_conn(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        conn.execute_batch(SCHEMA).context("init sqlite schema")?;
        migrate_projects(&conn)?;
        migrate_settings(&conn)?;
        Ok(Self { conn })
    }

    pub fn upsert_project(&self, project: &Project) -> Result<()> {
        let headers = serde_json::to_string(&project.default_headers)?;
        let envelope = project.envelope.sanitized();
        self.conn.execute(
            "INSERT INTO projects (
                id, name, port, success_code, fail_code, default_headers,
                envelope_code, envelope_msg, envelope_data
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                port = excluded.port,
                success_code = excluded.success_code,
                fail_code = excluded.fail_code,
                default_headers = excluded.default_headers,
                envelope_code = excluded.envelope_code,
                envelope_msg = excluded.envelope_msg,
                envelope_data = excluded.envelope_data",
            params![
                project.id,
                project.name,
                project.port as i64,
                project.success_code,
                project.fail_code,
                headers,
                envelope.code_key,
                envelope.msg_key,
                envelope.data_key,
            ],
        )?;
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, port, success_code, fail_code, default_headers,
                    envelope_code, envelope_msg, envelope_data
             FROM projects ORDER BY name, id",
        )?;
        let rows = stmt.query_map([], map_project)?;
        collect_rows(rows)
    }

    pub fn get_project(&self, id: &str) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, port, success_code, fail_code, default_headers,
                    envelope_code, envelope_msg, envelope_data
             FROM projects WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], map_project).optional()?)
    }

    pub fn delete_project(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn upsert_endpoint(&self, endpoint: &Endpoint) -> Result<()> {
        self.conn.execute(
            "INSERT INTO endpoints (
                id, project_id, method, path, name, notes, source_text,
                deprecated, enabled, current_scene
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                method = excluded.method,
                path = excluded.path,
                name = excluded.name,
                notes = excluded.notes,
                source_text = excluded.source_text,
                deprecated = excluded.deprecated,
                enabled = excluded.enabled,
                current_scene = excluded.current_scene",
            params![
                endpoint.id,
                endpoint.project_id,
                endpoint.method,
                endpoint.path,
                endpoint.name,
                endpoint.notes,
                endpoint.source_text,
                endpoint.deprecated,
                endpoint.enabled,
                endpoint.current_scene.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn list_endpoints(&self, project_id: &str) -> Result<Vec<Endpoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, method, path, name, notes, source_text,
                    deprecated, enabled, current_scene
             FROM endpoints WHERE project_id = ?1 ORDER BY path, method, id",
        )?;
        let rows = stmt.query_map(params![project_id], map_endpoint)?;
        collect_rows(rows)
    }

    pub fn get_endpoint(&self, id: &str) -> Result<Option<Endpoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, method, path, name, notes, source_text,
                    deprecated, enabled, current_scene
             FROM endpoints WHERE id = ?1",
        )?;
        Ok(stmt.query_row(params![id], map_endpoint).optional()?)
    }

    pub fn delete_endpoint(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM endpoints WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn replace_fields(&self, endpoint_id: &str, fields: &[Field]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM fields WHERE endpoint_id = ?1",
            params![endpoint_id],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO fields (
                    id, endpoint_id, location, name, name_zh, type_name,
                    required, comment, enum_values, parent_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )?;
            for field in fields {
                let enum_values = serde_json::to_string(&field.enum_values)?;
                // Argument endpoint_id wins so a mixed slice cannot attach to another endpoint.
                stmt.execute(params![
                    field.id,
                    endpoint_id,
                    field_loc_as_str(field.location),
                    field.name,
                    field.name_zh,
                    field.type_name,
                    field.required,
                    field.comment,
                    enum_values,
                    field.parent_id,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_fields(&self, endpoint_id: &str) -> Result<Vec<Field>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, endpoint_id, location, name, name_zh, type_name,
                    required, comment, enum_values, parent_id
             FROM fields WHERE endpoint_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![endpoint_id], map_field)?;
        collect_rows(rows)
    }

    pub fn upsert_scene(&self, scene: &Scene) -> Result<()> {
        self.conn.execute(
            "INSERT INTO scenes (endpoint_id, kind, http_status, body_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(endpoint_id, kind) DO UPDATE SET
                http_status = excluded.http_status,
                body_json = excluded.body_json",
            params![
                scene.endpoint_id,
                scene.kind.as_str(),
                scene.http_status as i64,
                scene.body_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_scene(&self, endpoint_id: &str, kind: SceneKind) -> Result<Option<Scene>> {
        let mut stmt = self.conn.prepare(
            "SELECT endpoint_id, kind, http_status, body_json
             FROM scenes WHERE endpoint_id = ?1 AND kind = ?2",
        )?;
        Ok(stmt
            .query_row(params![endpoint_id, kind.as_str()], map_scene)
            .optional()?)
    }

    pub fn set_current_scene(&self, endpoint_id: &str, kind: SceneKind) -> Result<()> {
        self.conn.execute(
            "UPDATE endpoints SET current_scene = ?1 WHERE id = ?2",
            params![kind.as_str(), endpoint_id],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<GlobalSettings> {
        let mut stmt = self.conn.prepare(
            "SELECT selected_source_id, selected_model, manual_base_url,
                    manual_api_key, manual_protocol, manual_model, manual_sources
             FROM settings WHERE id = 1",
        )?;
        let row = stmt
            .query_row([], |row| {
                let selected_source_id: String = row.get(0)?;
                let selected_model: String = row.get(1)?;
                let legacy_base: String = row.get(2)?;
                let legacy_key: String = row.get(3)?;
                let legacy_protocol: String = row.get(4)?;
                let legacy_model: String = row.get(5)?;
                let sources_json: String = row.get(6)?;
                Ok((
                    selected_source_id,
                    selected_model,
                    legacy_base,
                    legacy_key,
                    legacy_protocol,
                    legacy_model,
                    sources_json,
                ))
            })
            .optional()?;
        let Some((
            selected_source_id,
            selected_model,
            legacy_base,
            legacy_key,
            legacy_protocol,
            legacy_model,
            sources_json,
        )) = row
        else {
            return Ok(GlobalSettings::default());
        };
        let mut manual_sources: Vec<ManualSource> =
            serde_json::from_str(&sources_json).unwrap_or_default();
        if manual_sources.is_empty()
            && (!legacy_base.trim().is_empty()
                || !legacy_key.is_empty()
                || !legacy_model.trim().is_empty())
        {
            manual_sources.push(ManualSource {
                id: "manual".into(),
                name: if legacy_model.trim().is_empty() {
                    "手动填写".into()
                } else {
                    legacy_model.trim().to_string()
                },
                base_url: legacy_base,
                api_key: legacy_key,
                protocol: if legacy_protocol.trim().is_empty() {
                    "openai-chat".into()
                } else {
                    legacy_protocol
                },
                model: legacy_model,
            });
        }
        Ok(GlobalSettings {
            selected_source_id,
            selected_model,
            manual_sources,
        })
    }

    pub fn save_settings(&self, settings: &GlobalSettings) -> Result<()> {
        let sources_json = serde_json::to_string(&settings.manual_sources)?;
        let (base, key, protocol, model) = settings
            .manual_sources
            .first()
            .map(|m| {
                (
                    m.base_url.clone(),
                    m.api_key.clone(),
                    m.protocol.clone(),
                    m.model.clone(),
                )
            })
            .unwrap_or_default();
        self.conn.execute(
            "INSERT INTO settings (
                id, selected_source_id, selected_model, manual_base_url,
                manual_api_key, manual_protocol, manual_model, manual_sources
             ) VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
                selected_source_id = excluded.selected_source_id,
                selected_model = excluded.selected_model,
                manual_base_url = excluded.manual_base_url,
                manual_api_key = excluded.manual_api_key,
                manual_protocol = excluded.manual_protocol,
                manual_model = excluded.manual_model,
                manual_sources = excluded.manual_sources",
            params![
                settings.selected_source_id,
                settings.selected_model,
                base,
                key,
                protocol,
                model,
                sources_json,
            ],
        )?;
        Ok(())
    }

    pub fn append_log(&self, log: &RequestLog) -> Result<()> {
        let missing = serde_json::to_string(&log.missing_default_headers)?;
        self.conn.execute(
            "INSERT INTO request_logs (
                project_id, at, method, url, req_headers, req_body, hit,
                endpoint_id, scene, status, res_body, elapsed_ms, missing_default_headers
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                log.project_id,
                log.at,
                log.method,
                log.url,
                log.req_headers,
                log.req_body,
                log.hit,
                log.endpoint_id,
                log.scene,
                log.status as i64,
                log.res_body,
                i64::try_from(log.elapsed_ms).unwrap_or(i64::MAX),
                missing,
            ],
        )?;
        self.trim_logs(&log.project_id)?;
        Ok(())
    }

    pub fn list_logs(&self, project_id: &str) -> Result<Vec<RequestLog>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, at, method, url, req_headers, req_body, hit,
                    endpoint_id, scene, status, res_body, elapsed_ms, missing_default_headers
             FROM request_logs WHERE project_id = ?1 ORDER BY id DESC",
        )?;
        let rows = stmt.query_map(params![project_id], map_log)?;
        collect_rows(rows)
    }

    pub fn clear_logs(&self, project_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM request_logs WHERE project_id = ?1",
            params![project_id],
        )?;
        Ok(())
    }

    pub fn trim_logs(&self, project_id: &str) -> Result<()> {
        // OFFSET LOG_CAP-1 is the oldest id among the newest LOG_CAP rows.
        let cutoff: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM request_logs
                 WHERE project_id = ?1
                 ORDER BY id DESC
                 LIMIT 1 OFFSET ?2",
                params![project_id, LOG_CAP - 1],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(min_keep) = cutoff {
            self.conn.execute(
                "DELETE FROM request_logs WHERE project_id = ?1 AND id < ?2",
                params![project_id, min_keep],
            )?;
        }
        Ok(())
    }
}

fn migrate_projects(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(projects)")?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let extras = [
        ("envelope_code", "TEXT NOT NULL DEFAULT 'code'"),
        ("envelope_msg", "TEXT NOT NULL DEFAULT 'msg'"),
        ("envelope_data", "TEXT NOT NULL DEFAULT 'data'"),
    ];
    for (name, decl) in extras {
        if !existing.iter().any(|col| col == name) {
            conn.execute(
                &format!("ALTER TABLE projects ADD COLUMN {name} {decl}"),
                [],
            )
            .with_context(|| format!("add column projects.{name}"))?;
        }
    }
    Ok(())
}

fn migrate_settings(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(settings)")?;
    let existing: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !existing.iter().any(|col| col == "manual_sources") {
        conn.execute(
            "ALTER TABLE settings ADD COLUMN manual_sources TEXT NOT NULL DEFAULT '[]'",
            [],
        )
        .context("add column settings.manual_sources")?;
    }
    Ok(())
}

fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn map_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<Project> {
    let headers: String = row.get(5)?;
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        port: row.get::<_, i64>(2)? as u16,
        success_code: row.get(3)?,
        fail_code: row.get(4)?,
        default_headers: json_from_sql(5, &headers)?,
        envelope: Envelope {
            code_key: row.get(6)?,
            msg_key: row.get(7)?,
            data_key: row.get(8)?,
        }
        .sanitized(),
    })
}

fn map_endpoint(row: &rusqlite::Row<'_>) -> rusqlite::Result<Endpoint> {
    let kind: String = row.get(9)?;
    Ok(Endpoint {
        id: row.get(0)?,
        project_id: row.get(1)?,
        method: row.get(2)?,
        path: row.get(3)?,
        name: row.get(4)?,
        notes: row.get(5)?,
        source_text: row.get(6)?,
        deprecated: row.get(7)?,
        enabled: row.get(8)?,
        current_scene: parse_scene_kind(9, &kind)?,
    })
}

fn map_field(row: &rusqlite::Row<'_>) -> rusqlite::Result<Field> {
    let location: String = row.get(2)?;
    let enum_values: String = row.get(8)?;
    Ok(Field {
        id: row.get(0)?,
        endpoint_id: row.get(1)?,
        location: parse_field_loc(2, &location)?,
        name: row.get(3)?,
        name_zh: row.get(4)?,
        type_name: row.get(5)?,
        required: row.get(6)?,
        comment: row.get(7)?,
        enum_values: json_from_sql(8, &enum_values)?,
        parent_id: row.get(9)?,
    })
}

fn map_scene(row: &rusqlite::Row<'_>) -> rusqlite::Result<Scene> {
    let kind: String = row.get(1)?;
    Ok(Scene {
        endpoint_id: row.get(0)?,
        kind: parse_scene_kind(1, &kind)?,
        http_status: row.get::<_, i64>(2)? as u16,
        body_json: row.get(3)?,
    })
}

fn map_log(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestLog> {
    let missing: String = row.get(13)?;
    Ok(RequestLog {
        id: row.get(0)?,
        project_id: row.get(1)?,
        at: row.get(2)?,
        method: row.get(3)?,
        url: row.get(4)?,
        req_headers: row.get(5)?,
        req_body: row.get(6)?,
        hit: row.get(7)?,
        endpoint_id: row.get(8)?,
        scene: row.get(9)?,
        status: row.get::<_, i64>(10)? as u16,
        res_body: row.get(11)?,
        elapsed_ms: row.get::<_, i64>(12)? as u128,
        missing_default_headers: json_from_sql(13, &missing)?,
    })
}

fn json_from_sql<T: serde::de::DeserializeOwned>(idx: usize, s: &str) -> rusqlite::Result<T> {
    serde_json::from_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_scene_kind(idx: usize, s: &str) -> rusqlite::Result<SceneKind> {
    match s {
        "success" => Ok(SceneKind::Success),
        "empty" => Ok(SceneKind::Empty),
        "param_error" => Ok(SceneKind::ParamError),
        "business_error" => Ok(SceneKind::BusinessError),
        other => Err(invalid_text(idx, format!("unknown scene kind: {other}"))),
    }
}

fn parse_field_loc(idx: usize, s: &str) -> rusqlite::Result<FieldLoc> {
    match s {
        "header" => Ok(FieldLoc::Header),
        "body" => Ok(FieldLoc::Body),
        "response" => Ok(FieldLoc::Response),
        other => Err(invalid_text(
            idx,
            format!("unknown field location: {other}"),
        )),
    }
}

fn field_loc_as_str(loc: FieldLoc) -> &'static str {
    match loc {
        FieldLoc::Header => "header",
        FieldLoc::Body => "body",
        FieldLoc::Response => "response",
    }
}

fn invalid_text(idx: usize, msg: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, msg.into())
}
