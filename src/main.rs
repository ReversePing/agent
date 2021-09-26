use std::{collections::HashMap, time::Duration};

use crate::transmit::Transmitter;

mod agent;
mod discovery;
mod transmit;

use agent::{Agent, AgentConfig};
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "reverseping", about = "ReversePing Agent")]
struct Opt {
    #[structopt(subcommand)]
    command: Command,
}
#[derive(Debug, StructOpt)]
pub enum Command {
    /// Install and Start the agent daemon in the background
    Up {
        agent_id: String,
        #[structopt(long)]
        agent_only: bool,
    },
    /// Run the agent daemon in a loop
    Start {
        agent_id: Option<String>,
        #[structopt(long)]
        agent_only: bool,
    },
    /// Run the agent daemon once
    Scan { agent: String },
    /// Uninstall the agent daemon
    Uninstall,
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(3 * 1024 * 1024)
        .enable_all()
        .build()
        .expect("build async runtime");

    runtime
        .block_on(async move { start().await })
        .expect("Agent errored");
}

async fn start() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    match opt.command {
        Command::Uninstall => {
            Agent::uninstall_daemon()?;
            Agent::remove_config()?;
            Ok(())
        }
        Command::Up {
            agent_id,
            agent_only,
        } => {
            let _ = Agent::save_agent_config(&agent_id, agent_only)?;
            Agent::install_daemon()?;
            Ok(())
        }
        Command::Start {
            agent_id,
            agent_only,
        } => {
            let agent = if let Some(agent) = agent_id {
                let conf = Agent::save_agent_config(&agent, agent_only)?;
                Agent::install_daemon()?;
                conf
            } else {
                Agent::get_agent_config()?
            };

            loop {
                if let Err(err) = run(&agent).await {
                    log_err(err);
                }
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        }
        Command::Scan { agent } => {
            let conf = AgentConfig {
                agent,
                agent_only: false,
            };
            run(&conf).await
        }
    }
}

fn log_err(e: Box<dyn std::error::Error>) {
    let _ = Agent::write_log(format!("\n[Error] {}: {:?}", chrono::Local::now(), e));
}

async fn run(agent: &AgentConfig) -> Result<(), Box<dyn std::error::Error>> {
    let devices = if agent.agent_only {
        let _ = Agent::write_log("running in agent-only mode (no local device scanning)");
        HashMap::default()
    } else {
        #[cfg(unix)]
        sudo::escalate_if_needed().expect("Root access needed to scan devices");

        let devices = discovery::discover_devices().await?;

        let log = format!(
            "\n[Log] {}: Discovered devices:\n\n{}",
            chrono::Local::now(),
            devices
                .iter()
                .map(|d| format!("{}", d.1))
                .collect::<Vec<String>>()
                .join("-\t\n")
        );
        let _ = Agent::write_log(log);
        devices
    };

    Ok(Transmitter::new(&agent.agent).send(devices).await?)
}
