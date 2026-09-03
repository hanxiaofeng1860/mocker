pub mod domain;
pub mod scenes;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
