pub mod domain;
pub mod import;
pub mod llm;
pub mod runtime;
pub mod scenes;
pub mod sources;
pub mod store;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
