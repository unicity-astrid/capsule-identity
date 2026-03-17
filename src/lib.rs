#![deny(unsafe_code)]
#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![warn(missing_docs)]

//! Identity capsule for Astrid OS.
//!
//! Owns the agent's identity (spark config) in its KV store. Builds
//! the system prompt on `identity.v1.request.build` requests. Provides
//! `/identity-export` and `/identity-import` CLI commands.

use astrid_sdk::prelude::*;
use astrid_sdk::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

/// KV key for the spark identity config.
const SPARK_KEY: &str = "spark";

/// Default agent name when no spark config exists.
const DEFAULT_CALLSIGN: &str = "Astrid";
/// Default agent class/role.
const DEFAULT_CLASS: &str = "a secure coding assistant";

/// Agent identity configuration.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SparkConfig {
    /// Agent name/identifier.
    #[serde(default)]
    pub callsign: String,
    /// Agent role description.
    #[serde(default)]
    pub class: String,
    /// Personality traits.
    #[serde(default)]
    pub aura: String,
    /// Communication style preferences.
    #[serde(default)]
    pub signal: String,
    /// Core directives and constraints.
    #[serde(default)]
    pub core: String,
}

impl Default for SparkConfig {
    fn default() -> Self {
        Self {
            callsign: DEFAULT_CALLSIGN.into(),
            class: DEFAULT_CLASS.into(),
            aura: String::new(),
            signal: String::new(),
            core: String::new(),
        }
    }
}

impl SparkConfig {
    /// Build the identity preamble from spark fields.
    fn build_preamble(&self) -> String {
        let callsign = if self.callsign.is_empty() {
            DEFAULT_CALLSIGN
        } else {
            &self.callsign
        };

        let mut parts = vec![];
        if !self.class.is_empty() {
            parts.push(format!("You are {callsign}, {class}.", class = self.class));
        } else {
            parts.push(format!("You are {callsign}."));
        }

        if !self.aura.is_empty() {
            parts.push(format!("# Personality\n{}", self.aura));
        }
        if !self.signal.is_empty() {
            parts.push(format!("# Communication Style\n{}", self.signal));
        }
        if !self.core.is_empty() {
            parts.push(format!("# Core Directives\n{}", self.core));
        }

        parts.join("\n\n")
    }

    /// Serialize to TOML for export.
    fn to_toml(&self) -> String {
        let mut lines = vec![
            format!("callsign = \"{}\"", self.callsign),
            format!("class = \"{}\"", self.class),
        ];
        if !self.aura.is_empty() {
            lines.push(format!("aura = \"{}\"", self.aura));
        }
        if !self.signal.is_empty() {
            lines.push(format!("signal = \"{}\"", self.signal));
        }
        if !self.core.is_empty() {
            lines.push(format!("core = \"{}\"", self.core));
        }
        lines.join("\n")
    }
}

/// Load spark config from KV. If missing, store and return defaults.
fn load_or_init_spark() -> SparkConfig {
    match kv::get_json::<SparkConfig>(SPARK_KEY) {
        Ok(spark) => spark,
        Err(_) => {
            let spark = SparkConfig::default();
            let _ = kv::set_json(SPARK_KEY, &spark);
            spark
        }
    }
}

/// Request payload for building the system prompt.
#[derive(Debug, Deserialize)]
pub struct BuildRequest {
    /// Absolute path to the workspace root directory.
    pub workspace_root: String,
    /// Session ID for correlation.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Response payload containing the assembled system prompt.
#[derive(Debug, Serialize)]
struct BuildResponse {
    /// The fully assembled system prompt string.
    prompt: String,
    /// Session ID echoed from the request for correlation.
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

/// Identity builder capsule. Reads spark config from its own KV store.
#[derive(Default)]
pub struct IdentityBuilder;

#[capsule]
impl IdentityBuilder {
    /// Builds the system prompt from the spark identity in KV.
    #[astrid::interceptor("handle_build_request")]
    pub fn build_system_prompt(&self, req: BuildRequest) -> Result<(), SysError> {
        let workspace_root = req.workspace_root.trim_end_matches('/');
        let spark = load_or_init_spark();

        let opening = spark.build_preamble();

        let prompt = format!(
            "{opening}\n\n\
             # Environment\n\
             - Current working directory: {workspace_root}\n\
             - Platform: astrid-os"
        );

        let response = BuildResponse {
            prompt,
            session_id: req.session_id,
        };
        ipc::publish_json("identity.v1.response.ready", &response)?;

        Ok(())
    }

    /// Handles `/identity-export` — writes spark config to `.astrid/spark.toml`.
    #[astrid::interceptor("handle_command")]
    pub fn handle_command(&self, payload: serde_json::Value) -> Result<(), SysError> {
        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let session_id = payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        match text.trim() {
            "identity-export" => {
                let spark = load_or_init_spark();
                let toml = spark.to_toml();
                fs::write(".astrid/spark.toml", &toml)?;

                ipc::publish_json(
                    "agent.v1.response",
                    &serde_json::json!({
                        "type": "agent_response",
                        "text": format!("Identity exported to .astrid/spark.toml ({} bytes)", toml.len()),
                        "is_final": true,
                        "session_id": session_id,
                    }),
                )?;
            }
            "identity-import" => {
                let content = fs::read_to_string(".astrid/spark.toml")?;
                // Simple TOML key = "value" parser
                let spark = parse_spark_toml(&content);
                kv::set_json(SPARK_KEY, &spark)?;

                ipc::publish_json(
                    "agent.v1.response",
                    &serde_json::json!({
                        "type": "agent_response",
                        "text": format!("Identity imported from .astrid/spark.toml (callsign: {})", spark.callsign),
                        "is_final": true,
                        "session_id": session_id,
                    }),
                )?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Set the agent identity. Updates the spark config in KV.
    #[astrid::tool]
    pub fn set_identity(&self, input: SparkConfig) -> Result<serde_json::Value, SysError> {
        kv::set_json(SPARK_KEY, &input)?;
        Ok(serde_json::json!({
            "status": "ok",
            "callsign": input.callsign,
        }))
    }
}

/// Simple TOML parser for spark.toml (key = "value" pairs only).
fn parse_spark_toml(content: &str) -> SparkConfig {
    let mut spark = SparkConfig::default();
    for line in content.lines() {
        let line = line.trim();
        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');
            match key {
                "callsign" => spark.callsign = val.to_string(),
                "class" => spark.class = val.to_string(),
                "aura" => spark.aura = val.to_string(),
                "signal" => spark.signal = val.to_string(),
                "core" => spark.core = val.to_string(),
                _ => {}
            }
        }
    }
    spark
}
