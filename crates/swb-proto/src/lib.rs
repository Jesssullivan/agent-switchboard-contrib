//! Wire types for agent-switchboard (P1a stub).
//!
//! The full envelope lands in P1b. What is fixed now, by ruling (ADR-0001,
//! SWB-R14): the broker stamps `authority: peer` on every message and never
//! emits `operator`, so [`Authority`] has exactly one variant.

use serde::{Deserialize, Serialize};

/// The release version: `swb --version`, the broker's MCP `serverInfo`, the
/// channel's `serverInfo` and `/healthz` all report it. Bazel does not set
/// CARGO_PKG_VERSION from Cargo.toml, so this is kept equal to the workspace
/// version by hand; a test checks the two agree.
pub const VERSION: &str = "0.3.0";

/// Envelope version this crate speaks.
pub const ENVELOPE_VERSION: u32 = 3;

/// The envelope v3 JSON schema draft, embedded so tests pin it.
pub const ENVELOPE_V3_SCHEMA: &str = include_str!("../../../schemas/envelope-v3.schema.json");

/// Message authority. A message is teammate information; it can never carry
/// operator authority, whatever the sender claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Authority {
    /// The only authority the broker ever stamps.
    Peer,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_is_always_peer() {
        assert_eq!(serde_json::to_string(&Authority::Peer).unwrap(), "\"peer\"");
        let schema: serde_json::Value = serde_json::from_str(ENVELOPE_V3_SCHEMA).unwrap();
        assert_eq!(schema["properties"]["authority"]["const"], "peer");
        assert_eq!(schema["properties"]["v"]["const"], ENVELOPE_VERSION);
    }

    #[test]
    fn version_matches_the_workspace() {
        let workspace = include_str!("../../../Cargo.toml");
        assert!(
            workspace.contains(&format!("\nversion = \"{VERSION}\"\n")),
            "VERSION {VERSION} differs from the workspace version in Cargo.toml"
        );
        let module = include_str!("../../../MODULE.bazel");
        assert!(
            module.contains(&format!("version = \"{VERSION}\"")),
            "VERSION {VERSION} differs from MODULE.bazel"
        );
    }
}
