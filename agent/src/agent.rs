use std::{io::Write, path::PathBuf};

use askama::Template;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub struct Agent;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    IO(#[from] std::io::Error),
    #[error("System daemon directory not found")]
    SystemdDirectory,
    #[error("User directory not found")]
    UserDirectory,
    #[error("Agent not configured")]
    AgentNotConfigured,
    #[error("{0}")]
    TemplateFile(#[from] askama::Error),
    #[error("{0}")]
    Encoding(#[from] toml::ser::Error),
    #[error("{0}")]
    Decoding(#[from] toml::de::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub agent: String,
    #[serde(default = "agent_default")]
    pub agent_only: bool,
}

fn agent_default() -> bool {
    false
}

impl Agent {
    const BIN_NAME: &'static str = env!("CARGO_BIN_NAME");
    const NAME: &'static str = env!("CARGO_PKG_NAME");
    const CONFIG_FILE: &'static str = "config.toml";
    const LOG_FILE: &'static str = "debug.log";

    fn config_dir() -> Result<PathBuf, Error> {
        let path = directories::BaseDirs::new()
            .ok_or(Error::UserDirectory)?
            .config_dir()
            .join(Self::NAME);
        std::fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn config_file() -> Result<PathBuf, Error> {
        let path = Self::config_dir()?.join(Self::CONFIG_FILE);
        Ok(path)
    }

    pub fn remove_config() -> Result<(), Error> {
        std::fs::remove_dir_all(Self::config_dir()?)?;
        Ok(())
    }

    pub fn write_log<S: Into<String>>(data: S) -> Result<(), Error> {
        let data: String = data.into();
        eprintln!("{}", &data);
        let path = Self::config_dir()?.join(Self::LOG_FILE);
        let mut file = std::fs::OpenOptions::new().append(true).open(path)?;
        file.write_all(data.as_bytes())?;
        Ok(())
    }

    pub fn save_agent_config(agent_id: &str, agent_only: bool) -> Result<AgentConfig, Error> {
        let conf = AgentConfig {
            agent: agent_id.to_string(),
            agent_only,
        };
        let agent_file_contents = toml::to_string(&conf)?;
        std::fs::write(Self::config_file()?, agent_file_contents)?;
        Ok(conf)
    }

    pub fn get_agent_config() -> Result<AgentConfig, Error> {
        let path = Self::config_file()?;
        if !path.exists() {
            return Err(Error::AgentNotConfigured);
        }
        let config = std::fs::read_to_string(path)?;
        let agent_file: AgentConfig = toml::from_str(&config)?;
        Ok(agent_file)
    }

    #[cfg(target_os = "linux")]
    pub fn install_daemon() -> Result<(), Error> {
        sudo::escalate_if_needed().expect("Root access needed to scan devices");

        let systemd_file = SystemdServiceFile {
            bin_name: Self::BIN_NAME.to_string(),
            bin_path: std::env::current_exe()?.to_string_lossy().to_string(),
            description: env!("CARGO_PKG_DESCRIPTION").to_string(),
            user: whoami::username(),
        };

        let service_name = format!("{}.service", Self::NAME);
        let path = format!("/etc/systemd/system/{}", &service_name);

        let contents = systemd_file.render()?;
        std::fs::write(path, contents)?;

        let _ = std::process::Command::new("systemctl")
            .arg("--now")
            .arg("enable")
            .arg(&service_name)
            .output()?;

        let _ = std::process::Command::new("systemctl")
            .arg("start")
            .arg(service_name)
            .output()?;

        Ok(())
    }

    #[cfg(target_os = "linux")]
    pub fn uninstall_daemon() -> Result<(), Error> {
        sudo::escalate_if_needed().expect("Root access needed to scan devices");

        let service_name = format!("{}.service", Self::NAME);

        let _ = std::process::Command::new("systemctl")
            .arg("stop")
            .arg(&service_name)
            .output()?;

        let _ = std::process::Command::new("systemctl")
            .arg("disable")
            .arg(&service_name)
            .output()?;

        let path = format!("/etc/systemd/system/{}", &service_name);
        std::fs::remove_file(path)?;

        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn install_daemon() -> Result<(), Error> {
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn uninstall_daemon() -> Result<(), Error> {
        Ok(())
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Template)]
#[template(path = "systemd.service", escape = "none")]
struct SystemdServiceFile {
    description: String,
    bin_path: String,
    bin_name: String,
    user: String,
}
