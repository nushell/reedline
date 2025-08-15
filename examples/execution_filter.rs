// Example of using the execution_filter feature to delegate commands
// Run with: cargo run --example execution_filter --features "execution_filter suspend_control"

use reedline::{default_emacs_keybindings, DefaultPrompt, Emacs, Reedline, Signal};
use std::io;

#[cfg(feature = "execution_filter")]
use reedline::{ExecutionFilter, FilterDecision};
#[cfg(feature = "execution_filter")]
use std::sync::Arc;

/// Example filter that delegates interactive commands to external execution
#[cfg(feature = "execution_filter")]
#[derive(Debug)]
struct InteractiveCommandFilter {
    /// Commands that need special handling (e.g., PTY allocation)
    interactive_commands: Vec<String>,
}

#[cfg(feature = "execution_filter")]
impl InteractiveCommandFilter {
    fn new() -> Self {
        Self {
            interactive_commands: vec![
                "vim".to_string(),
                "vi".to_string(),
                "nano".to_string(),
                "emacs".to_string(),
                "less".to_string(),
                "more".to_string(),
                "top".to_string(),
                "htop".to_string(),
                "ssh".to_string(),
                "telnet".to_string(),
                "python".to_string(), // Interactive Python
                "ipython".to_string(),
                "node".to_string(), // Interactive Node.js
                "irb".to_string(),  // Interactive Ruby
            ],
        }
    }

    fn needs_special_handling(&self, command: &str) -> bool {
        let cmd = command.split_whitespace().next().unwrap_or("");

        // Check if it's an interactive command
        if self.interactive_commands.iter().any(|ic| cmd == ic) {
            return true;
        }

        // Check for docker/podman interactive flags
        if (cmd == "docker" || cmd == "podman") && command.contains("-it") {
            return true;
        }

        // Check if running Python/Node without arguments (interactive mode)
        if (cmd == "python" || cmd == "python3" || cmd == "node")
            && command.split_whitespace().count() == 1
        {
            return true;
        }

        false
    }
}

#[cfg(feature = "execution_filter")]
impl ExecutionFilter for InteractiveCommandFilter {
    fn filter(&self, command: &str) -> FilterDecision {
        if command.trim().is_empty() {
            return FilterDecision::Execute(command.to_string());
        }

        if self.needs_special_handling(command) {
            println!(
                "Delegating '{}' to external handler (would use PTY)",
                command
            );
            FilterDecision::Delegate(command.to_string())
        } else {
            FilterDecision::Execute(command.to_string())
        }
    }
}

fn main() -> io::Result<()> {
    println!("Reedline Execution Filter Example");
    println!("==================================");
    println!("This example demonstrates automatic command delegation.");
    println!("Interactive commands (vim, ssh, etc.) will be delegated.");
    println!("Regular commands will execute normally.");
    println!();

    let mut line_editor = Reedline::create();

    // Set up the execution filter
    #[cfg(feature = "execution_filter")]
    {
        let filter = Arc::new(InteractiveCommandFilter::new());
        line_editor.set_execution_filter(filter);
        println!("Execution filter installed");
    }

    #[cfg(not(feature = "execution_filter"))]
    {
        println!("WARNING: execution_filter feature not enabled");
        println!("Run with: --features execution_filter");
    }

    // Set up basic keybindings
    let edit_mode = Box::new(Emacs::new(default_emacs_keybindings()));
    line_editor = line_editor.with_edit_mode(edit_mode);

    let prompt = DefaultPrompt::default();

    loop {
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                if buffer.trim() == "exit" {
                    println!("Goodbye!");
                    break;
                }
                println!("Executing normally: {}", buffer);
                // In a real implementation, you would execute the command here
            }
            #[cfg(feature = "execution_filter")]
            Signal::ExecuteHostCommand(cmd) => {
                println!("External handler invoked for: {}", cmd);

                // In a real implementation, you would:
                // 1. Suspend the line editor
                #[cfg(feature = "suspend_control")]
                line_editor.suspend();

                // 2. Execute the command with PTY
                println!("   [Would execute '{}' in PTY]", cmd);

                // 3. Resume the line editor
                #[cfg(feature = "suspend_control")]
                {
                    line_editor.resume()?;
                    line_editor.force_repaint(&prompt)?;
                }

                println!("   [Command completed]");
            }
            Signal::CtrlD => {
                println!("\nExiting (Ctrl+D)");
                break;
            }
            Signal::CtrlC => {
                println!("\nInterrupted (Ctrl+C)");
                // Continue to next iteration
            }
            #[allow(unreachable_patterns)]
            _ => {
                // Handle any other signals if they exist
            }
        }
    }

    Ok(())
}
