use alacritty_test::{extract_text, pty_spawn, PtyExt, Terminal};
use std::time::Duration;

/// Test if Reedline prints the prompt at startup.
#[test]
fn prints_prompt() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    #[cfg(not(windows))]
    assert_eq!(&text[0][..13], "~/reedline〉");
    #[cfg(windows)]
    assert_eq!(&text[0][..13], "~\\reedline〉");

    Ok(())
}

/// Test if Reedline echos back input when the user presses Enter.
#[test]
fn echos_input() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"Hello World!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());

    assert_eq!(&text[0][13..25], "Hello World!");
    assert_eq!(&text[1][0..26], "We processed: Hello World!");

    Ok(())
}

/// Test if Reedline handles backspace correctly.
#[test]
fn backspace() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"Hello World")?;
    pty.write_all(b"\x7f\x7f\x7f\x7f\x7f")?;
    pty.write_all(b"Bread!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "Hello Bread!");
    assert_eq!(&text[1][0..26], "We processed: Hello Bread!");

    Ok(())
}

/// Test if Reedline supports history via up/down arrow.
#[test]
fn history() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"Hello World!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    pty.write_all(b"Goodbye!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    // arrow up
    pty.write_all(b"\x1b[A")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[4][13..21], "Goodbye!");

    // press Enter to execute it
    pty.write_all(b"\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[5][..22], "We processed: Goodbye!");

    // arrow up twice
    pty.write_all(b"\x1b[A\x1b[A")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[6][13..25], "Hello World!");

    // arrow down twice
    pty.write_all(b"\x1b[B\x1b[B")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[6][13..25], "            ");

    // type "Hel" then arrow up
    pty.write_all(b"Hel\x1b[A")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[6][13..25], "Hello World!");

    // TODO: not sure how reverse search works in Reedline

    Ok(())
}

/// Test if Reedline supports ctrl-b/ctrl-f/ctrl-left/ctrl-right style movement.
#[test]
fn word_movement() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"foo bar baz")?;

    // Ctrl-left twice, Ctrl-right once, Ctrl-b twice, Ctrl-f once.
    pty.write_all(b"\x1b[1;5D\x1b[1;5D")?;
    pty.write_all(b"\x1b[1;5C")?;
    pty.write_all(b"\x02\x02")?;
    pty.write_all(b"\x06")?;

    // Insert some more text, then press enter.
    pty.write_all(b"za\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..26], "foo bazar baz");
    assert_eq!(&text[1][..27], "We processed: foo bazar baz");

    Ok(())
}

/// Test if Ctrl-l clears the screen while keeping current entry.
#[test]
fn clear_screen() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"Hello World!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"Hello again!\x0c\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "Hello again!");

    Ok(())
}

/// Test if Reedline supports common Emacs keybindings.
#[test]
fn emacs_keybinds() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.write_all(b"Hello World!")?;

    // undo with Ctrl-z
    pty.write_all(b"\x1a")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "Hello       ");

    // redo with Ctrl-g
    pty.write_all(b"\x07")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "Hello World!");

    // delete "World" with alt+left, alt+backspace
    pty.write_all(b"\x1b[1;3D\x1b\x7f")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "Hello !     ");

    // make "Hello" ALL CAPS with alt+b, alt+u
    pty.write_all(b"\x1bb\x1bu")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "HELLO !     ");

    Ok(())
}
