use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub token: String,
}

#[derive(Deserialize)]
struct ConfigFile {
    server: Option<ServerConfig>,
}

#[derive(Deserialize)]
struct ServerConfig {
    url: Option<String>,
    /// Command to run to get the token (stdout is trimmed and used as the token value).
    token_command: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let env_url = std::env::var("ZAMMAD_URL").ok();
        let env_token_cmd = std::env::var("ZAMMAD_TOKEN_COMMAND").ok();

        if let (Some(url), Some(cmd)) = (env_url.clone(), env_token_cmd.clone()) {
            let token = run_token_command(&cmd)?;
            return Ok(Config {
                url: url.trim_end_matches('/').to_string(),
                token,
            });
        }

        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read config file")?;
            let file: ConfigFile =
                toml::from_str(&content).context("Failed to parse config file")?;

            if let Some(server) = file.server {
                let url = env_url.or(server.url).context("Missing 'url' in config")?;
                let cmd = env_token_cmd
                    .or(server.token_command)
                    .context("Missing 'token_command' in config")?;
                let token = run_token_command(&cmd)?;
                return Ok(Config {
                    url: url.trim_end_matches('/').to_string(),
                    token,
                });
            }
        }

        bail!(
            "No configuration found.\n\
             Set ZAMMAD_URL and ZAMMAD_TOKEN_COMMAND environment variables,\n\
             or create {}:\n\n\
             [server]\n\
             url = \"https://zammad.example.com\"\n\
             token_command = \"pass show zammad/api-token\"\n",
            Self::config_path()?.display()
        );
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().context("Cannot determine config directory")?;
        Ok(config_dir.join("zammad-tui").join("config.toml"))
    }
}

/// Run a shell command and return its stdout as the token.
fn run_token_command(cmd: &str) -> Result<String> {
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .with_context(|| format!("Failed to run token command: {cmd}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Token command failed (exit {}):\n  command: {cmd}\n  stderr: {stderr}",
            output.status.code().unwrap_or(-1)
        );
    }

    let token = String::from_utf8(output.stdout)
        .context("Token command output is not valid UTF-8")?
        .trim()
        .to_string();

    if token.is_empty() {
        bail!("Token command produced empty output: {cmd}");
    }

    Ok(token)
}
