//! Everything that talks to Bedrock Dedicated Server (BDS).

pub mod allowlist;
pub mod installer;
pub mod parser;
pub mod properties;
pub mod supervisor;

pub const EXECUTABLE: &str = "bedrock_server.exe";
