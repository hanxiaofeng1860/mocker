use mocker::domain::{
    Endpoint, Envelope, GlobalSettings, HeaderKv, ManualSource, Project, RequestLog, Scene,
    SceneKind,
};
use mocker::store::{self, Store};
use tempfile::TempDir;

fn open_tmp() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(dir.path().join("mocker.db")).unwrap();
    (dir, store)
}

fn sample_project() -> Project {
    Project {
        id: store::new_id(),
        name: "phone".into(),
        port: 18080,
        success_code: "0000".into(),
        fail_code: "9999".into(),
        default_headers: vec![HeaderKv {
            key: "sn".into(),
            value: "device".into(),
        }],
        envelope: Envelope::default(),
    }
}

fn sample_endpoint(project_id: &str) -> Endpoint {
    Endpoint {
        id: store::new_id(),
        project_id: project_id.into(),
        method: "POST".into(),
        path: "/api/queryPhoneHomeData".into(),
        name: "queryPhoneHomeData".into(),
        notes: String::new(),
        source_text: String::new(),
        deprecated: false,
        enabled: true,
        current_scene: SceneKind::Success,
    }
}

fn sample_log(project_id: &str, n: usize) -> RequestLog {
    RequestLog {
        id: 0,
        project_id: project_id.into(),
        at: format!("t{n}"),
        method: "POST".into(),
        url: format!("/n/{n}"),
        req_headers: "{}".into(),
        req_body: String::new(),
        hit: true,
        endpoint_id: None,
        scene: None,
        status: 200,
        res_body: format!("body-{n}"),
        elapsed_ms: 1,
        missing_default_headers: Vec::new(),
    }
}

#[test]
fn insert_project_endpoint_and_scenes_then_switch_current() {
    let (_dir, store) = open_tmp();
    let project = sample_project();
    store.upsert_project(&project).unwrap();

    let endpoint = sample_endpoint(&project.id);
    store.upsert_endpoint(&endpoint).unwrap();

    for kind in SceneKind::all() {
        store
            .upsert_scene(&Scene {
                endpoint_id: endpoint.id.clone(),
                kind,
                http_status: match kind {
                    SceneKind::ParamError => 400,
                    _ => 200,
                },
                body_json: format!(r#"{{"kind":"{}"}}"#, kind.as_str()),
            })
            .unwrap();
    }

    let projects = store.list_projects().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].id, project.id);
    assert_eq!(projects[0].default_headers[0].key, "sn");
    assert_eq!(projects[0].envelope, Envelope::default());

    let endpoints = store.list_endpoints(&project.id).unwrap();
    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].current_scene, SceneKind::Success);

    for kind in SceneKind::all() {
        let scene = store.get_scene(&endpoint.id, kind).unwrap().expect("scene");
        assert_eq!(scene.kind, kind);
    }

    store
        .set_current_scene(&endpoint.id, SceneKind::Empty)
        .unwrap();
    let got = store.get_endpoint(&endpoint.id).unwrap().unwrap();
    assert_eq!(got.current_scene, SceneKind::Empty);
}

#[test]
fn settings_round_trip_persists_manual_key_not_scan_token() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mocker.db");
    let settings = GlobalSettings {
        selected_source_id: "claude-code".into(),
        selected_model: "claude-sonnet".into(),
        manual_sources: vec![ManualSource {
            id: "manual-1".into(),
            name: "本地".into(),
            base_url: "http://127.0.0.1:9".into(),
            api_key: "sk-manual-allowed".into(),
            protocol: "anthropic-messages".into(),
            model: "claude-sonnet".into(),
        }],
    };

    {
        let store = Store::open(&path).unwrap();
        store.save_settings(&settings).unwrap();
        let loaded = store.load_settings().unwrap();
        assert_eq!(loaded.selected_source_id, "claude-code");
        assert_eq!(loaded.manual_sources.len(), 1);
        assert_eq!(loaded.manual_sources[0].api_key, "sk-manual-allowed");
        assert_eq!(loaded.selected_model, settings.selected_model);
        assert_eq!(loaded.manual_sources[0].base_url, "http://127.0.0.1:9");
        assert_eq!(loaded.manual_sources[0].protocol, "anthropic-messages");
        assert_eq!(loaded.manual_sources[0].model, "claude-sonnet");
    }

    let conn = rusqlite::Connection::open(&path).unwrap();
    let mut stmt = conn.prepare("PRAGMA table_info(settings)").unwrap();
    let columns: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(columns.contains(&"selected_source_id".into()));
    assert!(columns.contains(&"manual_api_key".into()));
    assert!(
        !columns
            .iter()
            .any(|name| name.to_ascii_lowercase().contains("token")),
        "scan secrets must not have a token column: {columns:?}"
    );
    assert_eq!(
        columns,
        vec![
            "id",
            "selected_source_id",
            "selected_model",
            "manual_base_url",
            "manual_api_key",
            "manual_protocol",
            "manual_model",
            "manual_sources",
        ]
    );
}

#[test]
fn trim_logs_keeps_newest_500_and_drops_oldest() {
    let (_dir, store) = open_tmp();
    let project = sample_project();
    store.upsert_project(&project).unwrap();

    for n in 0..505 {
        store.append_log(&sample_log(&project.id, n)).unwrap();
    }
    store.trim_logs(&project.id).unwrap();

    let logs = store.list_logs(&project.id).unwrap();
    assert_eq!(logs.len(), 500);
    assert!(!logs.iter().any(|log| log.url == "/n/0"));
    assert!(!logs.iter().any(|log| log.at == "t0"));
    assert_eq!(logs[0].url, "/n/504");
    assert_eq!(logs.last().unwrap().url, "/n/5");
}

#[test]
fn clear_logs_deletes_only_that_project() {
    let (_dir, store) = open_tmp();
    let a = sample_project();
    let mut b = sample_project();
    b.id = store::new_id();
    b.port = 18081;
    store.upsert_project(&a).unwrap();
    store.upsert_project(&b).unwrap();
    store.append_log(&sample_log(&a.id, 1)).unwrap();
    store.append_log(&sample_log(&b.id, 2)).unwrap();
    store.clear_logs(&a.id).unwrap();
    assert!(store.list_logs(&a.id).unwrap().is_empty());
    assert_eq!(store.list_logs(&b.id).unwrap().len(), 1);
}

#[test]
fn project_envelope_round_trip() {
    let (_dir, store) = open_tmp();
    let mut project = sample_project();
    project.envelope = Envelope {
        code_key: "errno".into(),
        msg_key: "message".into(),
        data_key: "result".into(),
    };
    store.upsert_project(&project).unwrap();
    let got = store.get_project(&project.id).unwrap().unwrap();
    assert_eq!(got.envelope.code_key, "errno");
    assert_eq!(got.envelope.msg_key, "message");
    assert_eq!(got.envelope.data_key, "result");
}

#[test]
fn open_migrates_old_projects_table_with_envelope_defaults() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mocker.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE projects (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                port INTEGER NOT NULL,
                success_code TEXT NOT NULL,
                fail_code TEXT NOT NULL,
                default_headers TEXT NOT NULL
            );
            INSERT INTO projects VALUES ('p1','old',18080,'0000','9999','[]');
            "#,
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let project = store.get_project("p1").unwrap().unwrap();
    assert_eq!(project.envelope, Envelope::default());
}

#[test]
fn open_lifts_legacy_manual_columns_into_manual_sources() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("mocker.db");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                selected_source_id TEXT NOT NULL,
                selected_model TEXT NOT NULL,
                manual_base_url TEXT NOT NULL,
                manual_api_key TEXT NOT NULL,
                manual_protocol TEXT NOT NULL,
                manual_model TEXT NOT NULL
            );
            INSERT INTO settings VALUES (
                1,'manual','deepseek','http://127.0.0.1:1','sk-old','openai-chat','deepseek'
            );
            "#,
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let loaded = store.load_settings().unwrap();
    assert_eq!(loaded.selected_source_id, "manual");
    assert_eq!(loaded.manual_sources.len(), 1);
    assert_eq!(loaded.manual_sources[0].id, "manual");
    assert_eq!(loaded.manual_sources[0].name, "deepseek");
    assert_eq!(loaded.manual_sources[0].api_key, "sk-old");
    assert_eq!(loaded.manual_sources[0].base_url, "http://127.0.0.1:1");
}
