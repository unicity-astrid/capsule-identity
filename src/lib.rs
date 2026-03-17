#![deny(unsafe_code)]
#![deny(clippy::all)]
#![deny(unreachable_pub)]
#![warn(missing_docs)]

//! Identity capsule for Astrid OS.
//!
//! Owns the agent's identity (spark config) as persistent state. Builds
//! the system prompt on `identity.v1.request.build` requests. On first
//! boot, injects an onboarding instruction so the agent walks the user
//! through identity setup. Provides `/identity-export` and
//! `/identity-import` CLI commands.

use astrid_sdk::prelude::*;
use astrid_sdk::schemars::{self, JsonSchema};
use serde::{Deserialize, Serialize};

/// Default agent name when no spark config exists.
const DEFAULT_CALLSIGN: &str = "Astrid";
/// Default agent class/role.
const DEFAULT_CLASS: &str = "a secure coding assistant";

/// Onboarding instruction appended to the system prompt when the user
/// hasn't configured their agent identity yet.
const ONBOARDING_PROMPT: &str = "\
# Important: Identity Setup Required

This is your first session. Before doing anything else, introduce yourself
briefly and ask the user if they'd like to personalize you. Ask for:
1. A name (callsign) they'd like to call you
2. What role/specialty they want you to focus on (class)
3. Any personality traits they'd like (aura) — optional
4. Communication style preferences (signal) — optional
5. Core directives or constraints (core) — optional

Once they provide answers, call the `set_identity` tool to save the configuration.
If they decline, that's fine — continue with the defaults.";

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

/// Identity capsule state — persisted to KV via `#[capsule(state)]`.
#[derive(Default, Debug, Serialize, Deserialize)]
pub struct IdentityBuilder {
    /// The spark identity configuration.
    spark: SparkConfig,
    /// Whether the user has completed identity onboarding.
    onboarded: bool,
}

#[capsule]
impl IdentityBuilder {
    /// Builds the system prompt from the spark identity.
    #[astrid::interceptor("handle_build_request")]
    pub fn build_system_prompt(&mut self, req: BuildRequest) -> Result<(), SysError> {
        let workspace_root = req.workspace_root.trim_end_matches('/');

        let opening = self.spark.build_preamble();

        // TODO: Move to a new capsule which handles env details. Time would be good too.
        let mut prompt = format!(
            "{opening}\n\n\
             # Environment\n\
             - Current working directory: {workspace_root}\n\
             - Platform: astrid-os"
        );

        if !self.onboarded {
            prompt.push_str("\n\n");
            prompt.push_str(ONBOARDING_PROMPT);
        }

        let response = BuildResponse {
            prompt,
            session_id: req.session_id,
        };
        ipc::publish_json("identity.v1.response.ready", &response)?;

        Ok(())
    }

    /// Handles `/identity-export` and `/identity-import` CLI commands.
    #[astrid::interceptor("handle_command")]
    pub fn handle_command(&mut self, payload: serde_json::Value) -> Result<(), SysError> {
        let text = payload.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let session_id = payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        match text.trim() {
            "identity-export" => {
                let toml = self.spark.to_toml();
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
                self.spark = parse_spark_toml(&content);
                self.onboarded = true;

                ipc::publish_json(
                    "agent.v1.response",
                    &serde_json::json!({
                        "type": "agent_response",
                        "text": format!("Identity imported from .astrid/spark.toml (callsign: {})", self.spark.callsign),
                        "is_final": true,
                        "session_id": session_id,
                    }),
                )?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Set the agent identity. Updates the spark config and marks
    /// onboarding as complete.
    #[astrid::tool]
    pub fn set_identity(&mut self, input: SparkConfig) -> Result<serde_json::Value, SysError> {
        self.spark = input;
        self.onboarded = true;
        Ok(serde_json::json!({
            "status": "ok",
            "callsign": self.spark.callsign,
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
