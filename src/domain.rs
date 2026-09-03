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
    pub manual_base_url: String,
    pub manual_api_key: String,
    pub manual_protocol: String,
    pub manual_model: String,
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
