use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderKv {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub port: u16,
    pub success_code: String,
    pub fail_code: String,
    pub default_headers: Vec<HeaderKv>,
    pub envelope: Envelope,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope {
    pub code_key: String,
    pub msg_key: String,
    pub data_key: String,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            code_key: "code".into(),
            msg_key: "msg".into(),
            data_key: "data".into(),
        }
    }
}

impl Envelope {
    pub fn sanitized(&self) -> Self {
        Self {
            code_key: nonempty_key(&self.code_key, "code"),
            msg_key: nonempty_key(&self.msg_key, "msg"),
            data_key: nonempty_key(&self.data_key, "data"),
        }
    }

    pub fn pack(
        &self,
        code: serde_json::Value,
        msg: &str,
        data: serde_json::Value,
    ) -> serde_json::Value {
        let env = self.sanitized();
        let mut map = serde_json::Map::new();
        map.insert(env.code_key, code);
        map.insert(env.msg_key, serde_json::Value::String(msg.to_string()));
        map.insert(env.data_key, data);
        serde_json::Value::Object(map)
    }
}

fn nonempty_key(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneKind {
    Success,
    Empty,
    ParamError,
    BusinessError,
}

impl SceneKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Empty => "empty",
            Self::ParamError => "param_error",
            Self::BusinessError => "business_error",
        }
    }

    pub fn all() -> [SceneKind; 4] {
        [
            Self::Success,
            Self::Empty,
            Self::ParamError,
            Self::BusinessError,
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Endpoint {
    pub id: String,
    pub project_id: String,
    pub method: String,
    pub path: String,
    pub name: String,
    pub notes: String,
    pub source_text: String,
    pub deprecated: bool,
    pub enabled: bool,
    pub current_scene: SceneKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldLoc {
    Header,
    Body,
    Response,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub endpoint_id: String,
    pub location: FieldLoc,
    pub name: String,
    pub name_zh: String,
    pub type_name: String,
    pub required: bool,
    pub comment: String,
    pub enum_values: Vec<String>,
    pub parent_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scene {
    pub endpoint_id: String,
    pub kind: SceneKind,
    pub http_status: u16,
    pub body_json: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct GlobalSettings {
    pub selected_source_id: String,
    pub selected_model: String,
    #[serde(default)]
    pub manual_sources: Vec<ManualSource>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManualSource {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub protocol: String,
    pub model: String,
}

impl GlobalSettings {
    pub fn find_manual(&self, id: &str) -> Option<&ManualSource> {
        self.manual_sources.iter().find(|m| m.id == id)
    }
}

#[derive(Clone, Debug)]
pub struct RequestLog {
    pub id: i64,
    pub project_id: String,
    pub at: String,
    pub method: String,
    pub url: String,
    pub req_headers: String,
    pub req_body: String,
    pub hit: bool,
    pub endpoint_id: Option<String>,
    pub scene: Option<String>,
    pub status: u16,
    pub res_body: String,
    pub elapsed_ms: u128,
    pub missing_default_headers: Vec<String>,
}
