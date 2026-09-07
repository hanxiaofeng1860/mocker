use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use arc_swap::ArcSwap;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::{
    ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
    CONTENT_TYPE,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::Router;
use serde_json::Value;
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::domain::{Envelope, Project, RequestLog};
use crate::scenes::encode_code;
use crate::store::Store;

const CORS_METHODS: &str = "GET, POST, PUT, PATCH, DELETE, OPTIONS";
const BODY_LIMIT: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct RouteSnapshot {
    pub fail_code: String,
    pub envelope: Envelope,
    pub default_header_keys: Vec<String>,
    pub routes: HashMap<(String, String), Route>,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub status: u16,
    pub body: String,
    pub scene: String,
    pub endpoint_id: String,
    pub enabled: bool,
}

enum TokioRt {
    Current(Handle),
    Owned(tokio::runtime::Runtime),
}

impl TokioRt {
    fn handle(&self) -> Handle {
        match self {
            Self::Current(handle) => handle.clone(),
            Self::Owned(rt) => rt.handle().clone(),
        }
    }
}

struct Running {
    snapshot: Arc<ArcSwap<RouteSnapshot>>,
    shutdown: watch::Sender<bool>,
}

pub struct RuntimeHub {
    inner: Mutex<HashMap<String, Running>>,
    tokio: TokioRt,
    store: Arc<Mutex<Store>>,
}

impl RuntimeHub {
    pub fn new(store: Arc<Mutex<Store>>) -> Self {
        // Prefer the caller's runtime so #[tokio::test] can poll the server.
        let tokio = match Handle::try_current() {
            Ok(handle) => TokioRt::Current(handle),
            Err(_) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .enable_io()
                    .build()
                    .expect("tokio runtime");
                TokioRt::Owned(rt)
            }
        };
        Self {
            inner: Mutex::new(HashMap::new()),
            tokio,
            store,
        }
    }

    pub fn start(&self, project_id: &str) -> Result<()> {
        let (port, snapshot) = {
            let store = lock(&self.store)?;
            let project = store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("project not found: {project_id}"))?;
            let snapshot = build_snapshot(&store, &project)?;
            (project.port, snapshot)
        };

        let mut inner = lock(&self.inner)?;
        if inner.contains_key(project_id) {
            anyhow::bail!("project {project_id} already running");
        }

        // Bind this project's port only; never fall back to another port.
        let std_listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .with_context(|| format!("bind 127.0.0.1:{port}"))?;
        std_listener
            .set_nonblocking(true)
            .context("set nonblocking")?;

        let snapshot = Arc::new(ArcSwap::from_pointee(snapshot));
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let state = AppState {
            project_id: project_id.to_string(),
            snapshot: snapshot.clone(),
            store: self.store.clone(),
        };
        let app = Router::new().fallback(mock_handler).with_state(state);

        let handle = self.tokio.handle();
        let _enter = handle.enter();
        let listener =
            tokio::net::TcpListener::from_std(std_listener).context("tokio listener from std")?;
        handle.spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.wait_for(|stop| *stop).await;
                })
                .await;
        });

        inner.insert(
            project_id.to_string(),
            Running {
                snapshot,
                shutdown: shutdown_tx,
            },
        );
        Ok(())
    }

    pub fn stop(&self, project_id: &str) -> Result<()> {
        let mut inner = lock(&self.inner)?;
        if let Some(running) = inner.remove(project_id) {
            let _ = running.shutdown.send(true);
        }
        Ok(())
    }

    pub fn refresh(&self, project_id: &str) -> Result<()> {
        let cell = {
            let inner = lock(&self.inner)?;
            match inner.get(project_id) {
                Some(running) => running.snapshot.clone(),
                None => return Ok(()),
            }
        };
        let snapshot = {
            let store = lock(&self.store)?;
            let project = store
                .get_project(project_id)?
                .ok_or_else(|| anyhow!("project not found: {project_id}"))?;
            build_snapshot(&store, &project)?
        };
        cell.store(Arc::new(snapshot));
        Ok(())
    }

    pub fn is_running(&self, project_id: &str) -> bool {
        lock(&self.inner)
            .map(|inner| inner.contains_key(project_id))
            .unwrap_or(false)
    }
}

impl Drop for RuntimeHub {
    fn drop(&mut self) {
        let ids: Vec<String> = lock(&self.inner)
            .map(|inner| inner.keys().cloned().collect())
            .unwrap_or_default();
        for id in ids {
            let _ = self.stop(&id);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    mutex.lock().map_err(|_| anyhow!("mutex poisoned"))
}

fn build_snapshot(store: &Store, project: &Project) -> Result<RouteSnapshot> {
    let mut routes = HashMap::new();
    for endpoint in store.list_endpoints(&project.id)? {
        if !endpoint.enabled {
            continue;
        }
        let Some(scene) = store.get_scene(&endpoint.id, endpoint.current_scene)? else {
            continue;
        };
        routes.insert(
            (endpoint.method.to_ascii_uppercase(), endpoint.path.clone()),
            Route {
                status: scene.http_status,
                body: scene.body_json,
                scene: endpoint.current_scene.as_str().to_string(),
                endpoint_id: endpoint.id,
                enabled: true,
            },
        );
    }
    Ok(RouteSnapshot {
        fail_code: project.fail_code.clone(),
        envelope: project.envelope.clone(),
        default_header_keys: project
            .default_headers
            .iter()
            .map(|h| h.key.clone())
            .filter(|k| !k.is_empty())
            .collect(),
        routes,
    })
}

#[derive(Clone)]
struct AppState {
    project_id: String,
    snapshot: Arc<ArcSwap<RouteSnapshot>>,
    store: Arc<Mutex<Store>>,
}

async fn mock_handler(State(state): State<AppState>, req: Request) -> Response {
    let method = req.method().as_str().to_ascii_uppercase();
    // OPTIONS is CORS preflight only: no route match, no request log.
    if method == "OPTIONS" {
        return options_response();
    }

    let started = Instant::now();
    let fail_code = state.snapshot.load().fail_code.clone();
    let envelope = state.snapshot.load().envelope.clone();
    match handle_request(&state, req, method, started).await {
        Ok(response) => response,
        Err(_) => json_response(500, &envelope_body(&fail_code, "mock 内部错误", &envelope)),
    }
}

async fn handle_request(
    state: &AppState,
    req: Request,
    method: String,
    started: Instant,
) -> Result<Response> {
    let path = req.uri().path().to_string();
    let url = req.uri().to_string();
    let headers = req.headers().clone();
    let body_bytes = axum::body::to_bytes(req.into_body(), BODY_LIMIT)
        .await
        .unwrap_or_default();
    let req_body = String::from_utf8_lossy(&body_bytes).into_owned();

    let snap = state.snapshot.load();
    let missing = missing_default_headers(&snap.default_header_keys, &headers);
    let (status, res_body, hit, endpoint_id, scene) = match lookup(&snap, &method, &path) {
        Lookup::Hit(route) => (
            route.status,
            route.body.clone(),
            true,
            Some(route.endpoint_id.clone()),
            Some(route.scene.clone()),
        ),
        Lookup::MethodNotAllowed => (
            405,
            envelope_body(&snap.fail_code, "方法不允许", &snap.envelope),
            false,
            None,
            None,
        ),
        Lookup::NotFound => (
            404,
            envelope_body(&snap.fail_code, "未找到 mock 接口", &snap.envelope),
            false,
            None,
            None,
        ),
    };
    drop(snap);

    let log = RequestLog {
        id: 0,
        project_id: state.project_id.clone(),
        at: chrono::Utc::now().to_rfc3339(),
        method,
        url,
        req_headers: headers_json(&headers),
        req_body,
        hit,
        endpoint_id,
        scene,
        status,
        res_body: res_body.clone(),
        elapsed_ms: started.elapsed().as_millis(),
        missing_default_headers: missing,
    };
    if let Ok(store) = lock(&state.store) {
        let _ = store.append_log(&log);
    }

    Ok(json_response(status, &res_body))
}

enum Lookup<'a> {
    Hit(&'a Route),
    MethodNotAllowed,
    NotFound,
}

fn lookup<'a>(snap: &'a RouteSnapshot, method: &str, path: &str) -> Lookup<'a> {
    if let Some(route) = snap.routes.get(&(method.to_string(), path.to_string())) {
        if route.enabled {
            return Lookup::Hit(route);
        }
    }
    if snap.routes.keys().any(|(_, p)| p == path) {
        Lookup::MethodNotAllowed
    } else {
        Lookup::NotFound
    }
}

fn missing_default_headers(keys: &[String], headers: &HeaderMap) -> Vec<String> {
    keys.iter()
        .filter(|key| {
            !headers
                .iter()
                .any(|(name, _)| name.as_str().eq_ignore_ascii_case(key))
        })
        .cloned()
        .collect()
}

fn headers_json(headers: &HeaderMap) -> String {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        let text = String::from_utf8_lossy(value.as_bytes()).into_owned();
        map.entry(name.as_str().to_string())
            .and_modify(|existing| {
                if let Value::String(s) = existing {
                    s.push_str(", ");
                    s.push_str(&text);
                }
            })
            .or_insert(Value::String(text));
    }
    Value::Object(map).to_string()
}

fn envelope_body(fail_code: &str, msg: &str, envelope: &Envelope) -> String {
    envelope
        .pack(encode_code(fail_code), msg, Value::Null)
        .to_string()
}

fn apply_cors(headers: &mut HeaderMap) {
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static(CORS_METHODS),
    );
    headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("*"));
}

fn json_response(status: u16, body: &str) -> Response {
    let mut response = Response::new(Body::from(body.to_owned()));
    *response.status_mut() =
        StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = response.headers_mut();
    apply_cors(headers);
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn options_response() -> Response {
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::NO_CONTENT;
    apply_cors(response.headers_mut());
    response
}
