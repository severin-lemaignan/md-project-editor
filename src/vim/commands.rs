/// Ex command-line mode handler.
/// Parses and executes commands like :w, :q, :wq, :{n}

pub enum CommandResult {
    /// No action needed
    None,
    /// Quit the application
    Quit,
    /// Go to a specific line
    GotoLine(i32),
    /// Unknown / invalid command
    Error(String),
}

/// Parse and execute an ex command string (without the leading ':').
pub fn execute_command(cmd: &str) -> CommandResult {
    let cmd = cmd.trim();

    if cmd.is_empty() {
        return CommandResult::None;
    }

    // Check for a line number
    if let Ok(n) = cmd.parse::<i32>() {
        return CommandResult::GotoLine(n);
    }

    match cmd {
        "q" | "quit" => CommandResult::Quit,
        "q!" => CommandResult::Quit,
        "w" | "write" => {
            // TODO: implement file saving
            CommandResult::None
        }
        "wq" | "x" => {
            // TODO: save then quit
            CommandResult::Quit
        }
        _ => CommandResult::Error(format!("Unknown command: {cmd}")),
    }
}
