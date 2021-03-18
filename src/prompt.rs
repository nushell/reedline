use chrono::Local;
use crossterm::terminal;
use std::env;

pub struct Prompt {
    // The prompt symbol like >
    prompt_indicator: String,
    // The string for the left side of the prompt
    left_prompt: String,
    // The length of the left side of the prompt without ansi color
    left_prompt_width: usize,
    // The string for the right side of the prompt
    right_prompt: String,
    // The length of the right side of the prompt without ansi color
    right_print_width: usize,
    // The size of the terminal in columns, rows
    terminal_size: (u16, u16),
    // The number of line buffer character space between the
    // the left prompt and the right prompt. When this encroaches
    // into the right side prompt, we should not show the right
    // prompt. For future use. Not used currently.
    max_length: i32,
}

impl Prompt {
    pub fn new() -> Prompt {
        let term_size = get_terminal_size();
        // create the initial prompt
        let p_indicator = "〉".to_string();
        let l_prompt = "".to_string();
        let r_prompt = get_now();

        Prompt {
            // a clone for you, and a clone for you, and a clone for you...
            prompt_indicator: p_indicator.clone(),
            left_prompt: l_prompt.clone(),
            left_prompt_width: p_indicator.chars().count() + l_prompt.chars().count(),
            right_prompt: r_prompt.clone(),
            right_print_width: r_prompt.chars().count(),
            max_length: -1,
            terminal_size: term_size,
        }
    }

    pub fn set_left_prompt(&mut self, lprompt: String) {
        self.left_prompt = lprompt;
    }

    pub fn set_right_prompt(&mut self, rprompt: String) {
        self.right_prompt = rprompt;
    }

    pub fn set_prompt_indicator(&mut self, indicator: String) {
        self.prompt_indicator = indicator;
    }

    pub fn print_prompt(&mut self) -> String {
        // This is not really a left and right prompt at all. It's faking it.
        // It's really just a string that fits within the width of your screen.

        let mut prompt_str = String::new();

        self.terminal_size = get_terminal_size();
        let working_dir = match get_working_dir() {
            Ok(cwd) => cwd,
            _ => "no path".to_string(),
        };

        self.set_left_prompt(working_dir);
        self.left_prompt_width = self.left_prompt.chars().count();
        prompt_str.push_str(&self.left_prompt);

        // Figure out the right side padding width
        let padding_width: usize = if usize::from(self.terminal_size.0) < self.left_prompt_width {
            0
        } else {
            usize::from(self.terminal_size.0) - self.left_prompt_width
        };

        let right = format!("{:>width$}", get_now(), width = padding_width);
        self.set_right_prompt(right);
        self.right_print_width = self.right_prompt.chars().count();

        // At some point check the buffer length, assuming an actual left & right propmt functionality
        self.max_length = -1;

        prompt_str.push_str(&self.right_prompt);
        prompt_str.push_str(&self.prompt_indicator);

        prompt_str
    }
}

fn get_terminal_size() -> (u16, u16) {
    let ts = terminal::size();
    match ts {
        Ok((columns, rows)) => (columns, rows),
        Err(_) => (0, 0),
    }
}

fn get_working_dir() -> Result<String, std::io::Error> {
    let path = env::current_dir()?;
    Ok(path.display().to_string())
}

fn get_now() -> String {
    let now = Local::now();
    format!("{}", now.format("%m/%d/%Y %I:%M:%S %p"))
}
