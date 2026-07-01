use std::{
    io::{self, Write},
    process::Command,
};

use color_eyre::{Result, eyre::eyre};
use crossterm::{
    cursor::Show,
    execute,
    terminal::{LeaveAlternateScreen, disable_raw_mode},
};

use super::Server;

#[derive(Debug, Clone)]
pub(super) struct SshLaunch {
    pub(super) command: String,
    pub(super) args: Vec<String>,
}

pub(super) fn ssh_command(server: &Server) -> String {
    let launch = ssh_launch(server);
    std::iter::once(launch.command)
        .chain(launch.args)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn ssh_launch(server: &Server) -> SshLaunch {
    let mut args = Vec::new();
    if server.auth_type == "SSH Key" {
        if let Some(key) = &server.key_path {
            if !key.trim().is_empty() {
                args.push("-i".to_string());
                args.push(key.clone());
            }
        }
    }
    args.push(format!("{}@{}", server.username, server.host));
    args.push("-p".to_string());
    args.push(server.port.to_string());
    SshLaunch {
        command: "ssh".to_string(),
        args,
    }
}

pub(super) fn launch_ssh(launch: SshLaunch) -> Result<()> {
    restore_terminal_for_process()?;
    let status = Command::new(&launch.command).args(&launch.args).status()?;
    if !status.success() {
        return Err(eyre!("ssh exited with status {status}"));
    }
    Ok(())
}

fn restore_terminal_for_process() -> Result<()> {
    let _ = disable_raw_mode();
    let mut stdout = io::stdout();
    execute!(stdout, Show, LeaveAlternateScreen)?;
    stdout.flush()?;
    Ok(())
}
