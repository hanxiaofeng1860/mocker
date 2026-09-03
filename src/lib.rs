pub mod domain;
pub mod runtime;
pub mod scenes;
pub mod store;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
