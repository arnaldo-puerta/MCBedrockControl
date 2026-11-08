//! Everything that talks to Bedrock Dedicated Server (BDS).

pub mod allowlist;
pub mod archive;
pub mod backup;
pub mod installer;
pub mod level_dat;
pub mod packs;
pub mod parser;
pub mod properties;
pub mod settings;
pub mod supervisor;
pub mod updater;
pub mod worlds;

pub const EXECUTABLE: &str = "bedrock_server.exe";
