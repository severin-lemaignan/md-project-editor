/// Ex command-line mode handler.
/// Parses and executes commands like :w, :q, :wq, :{n}, :s/foo/bar/, :%s/foo/bar/g

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubstituteCommand {
    pub pattern: String,
    pub replacement: String,
    pub global: bool,
    pub whole_file: bool,
}

pub enum CommandResult {
    /// No action needed
    None,
    /// Quit the application
    Quit,
    /// Go to a specific line
    GotoLine(i32),
    /// Perform a substitution
    Substitute(SubstituteCommand),
    /// Unknown / invalid command
    Error(String),
}

fn parse_delimited(input: &str, delimiter: char) -> Result<(String, String, String), String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in input.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '\\' {
            escaped = true;
            continue;
        }

        if ch == delimiter && parts.len() < 2 {
            parts.push(current);
            current = String::new();
            continue;
        }

        current.push(ch);
    }

    if escaped {
        current.push('\\');
    }
    parts.push(current);

    if parts.len() < 3 {
        Err("Invalid substitute command".to_string())
    } else {
        Ok((parts[0].clone(), parts[1].clone(), parts[2].clone()))
    }
}

fn parse_substitute(cmd: &str, whole_file: bool) -> Result<SubstituteCommand, String> {
    let body = if whole_file {
        cmd.strip_prefix("%s").ok_or_else(|| "Invalid substitute command".to_string())?
    } else {
        cmd.strip_prefix('s').ok_or_else(|| "Invalid substitute command".to_string())?
    };

    let mut chars = body.chars();
    let delimiter = chars
        .next()
        .ok_or_else(|| "Missing substitute delimiter".to_string())?;
    let rest: String = chars.collect();
    let (pattern, replacement, flags) = parse_delimited(&rest, delimiter)?;

    if pattern.is_empty() {
        return Err("Empty search pattern".to_string());
    }

    Ok(SubstituteCommand {
        pattern,
        replacement,
        global: flags.contains('g'),
        whole_file,
    })
}

/// Parse and execute an ex command string (without the leading ':').
pub fn execute_command(cmd: &str) -> CommandResult {
    let cmd = cmd.trim();

    if cmd.is_empty() {
        return CommandResult::None;
    }

    if let Ok(substitute) = parse_substitute(cmd, false) {
        return CommandResult::Substitute(substitute);
    }

    if let Ok(substitute) = parse_substitute(cmd, true) {
        return CommandResult::Substitute(substitute);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_line_substitute() {
        let result = execute_command("s/foo/bar/g");
        match result {
            CommandResult::Substitute(command) => {
                assert_eq!(command.pattern, "foo");
                assert_eq!(command.replacement, "bar");
                assert!(command.global);
                assert!(!command.whole_file);
            }
            _ => panic!("expected substitute command"),
        }
    }

    #[test]
    fn parses_whole_file_substitute() {
        let result = execute_command("%s/alpha/beta/");
        match result {
            CommandResult::Substitute(command) => {
                assert_eq!(command.pattern, "alpha");
                assert_eq!(command.replacement, "beta");
                assert!(!command.global);
                assert!(command.whole_file);
            }
            _ => panic!("expected substitute command"),
        }
    }
}
