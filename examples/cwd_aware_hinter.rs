// Create a reedline object with in-line hint support.
// cargo run --example cwd_aware_hinter
//
// Fish-style cwd history based hinting
// assuming history ["abc", "ade"]
// pressing "a" hints to abc.
// Up/Down or Ctrl p/n, to select next/previous match

use std::io;

fn create_item(cwd: &str, cmd: &str, exit_status: i64) -> reedline::HistoryItem {
    use std::time::Duration;

    use reedline::HistoryItem;
    HistoryItem {
        id: None,
        start_timestamp: None,
        command_line: cmd.to_string(),
        session_id: None,
        hostname: Some("foohost".to_string()),
        cwd: Some(cwd.to_string()),
        duration: Some(Duration::from_millis(1000)),
        exit_status: Some(exit_status),
        more_info: None,
    }
}

fn create_filled_example_history(home_dir: &str, orig_dir: &str) -> Box<dyn reedline::History> {
    use reedline::History;
    #[cfg(not(any(feature = "sqlite", feature = "sqlite-dynlib")))]
    let mut history = Box::new(reedline::FileBackedHistory::new(100));
    #[cfg(any(feature = "sqlite", feature = "sqlite-dynlib"))]
    let mut history = Box::new(reedline::SqliteBackedHistory::in_memory().unwrap());

    history.save(create_item(orig_dir, "dummy", 0)).unwrap(); // add dummy item so ids start with 1
    history.save(create_item(orig_dir, "ls /usr", 0)).unwrap();
    history.save(create_item(orig_dir, "pwd", 0)).unwrap();

    history.save(create_item(home_dir, "cat foo", 0)).unwrap();
    history.save(create_item(home_dir, "ls bar", 0)).unwrap();
    history.save(create_item(home_dir, "rm baz", 0)).unwrap();

    history
}

fn main() -> io::Result<()> {
    use nu_ansi_term::{Color, Style};
    use reedline::{CwdAwareHinter, DefaultPrompt, Reedline, Signal};

    let orig_dir = std::env::current_dir().unwrap();
    #[allow(deprecated)]
    let home_dir = std::env::home_dir().unwrap();

    let history = create_filled_example_history(
        home_dir.to_string_lossy().as_ref(),
        orig_dir.to_string_lossy().as_ref(),
    );

    let mut line_editor = Reedline::create()
        .with_hinter(Box::new(
            CwdAwareHinter::default().with_style(Style::new().bold().italic().fg(Color::Yellow)),
        ))
        .with_history(history);

    let prompt = DefaultPrompt::default();

    let mut iterations = 0;
    loop {
        if iterations % 2 == 0 {
            std::env::set_current_dir(&orig_dir).unwrap();
        } else {
            std::env::set_current_dir(&home_dir).unwrap();
        }
        let sig = line_editor.read_line(&prompt)?;
        match sig {
            Signal::Success(buffer) => {
                println!("We processed: {buffer}");
            }
            Signal::CtrlD | Signal::CtrlC => {
                println!("\nAborted!");
                break Ok(());
            }
        }
        iterations += 1;
    }
}
