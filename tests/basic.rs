use alacritty_test::{extract_text, pty_spawn, EventedReadWrite, Terminal};
use std::{io::Write, time::Duration};

/// Test if Reedline prints the prompt at startup.
#[test]
fn prints_prompt() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..13], "~/reedline〉");

    Ok(())
}

/// Test if Reedline echos back input when the user presses Enter.
#[test]
fn echos_input() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"Hello World!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());

    assert_eq!(&text[0][13..25], "Hello World!");
    assert_eq!(&text[1][0..26], "We processed: Hello World!");
    assert_eq!(&text[2][0..13], "~/reedline〉");

    Ok(())
}

/// Test if Reedline handles backspace correctly.
#[test]
fn backspace() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"Hello World")?;
    pty.writer().write_all(b"\x7f\x7f\x7f\x7f\x7f")?;
    pty.writer().write_all(b"Bread!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][13..25], "Hello Bread!");
    assert_eq!(&text[1][0..26], "We processed: Hello Bread!");
    assert_eq!(&text[2][0..13], "~/reedline〉");

    Ok(())
}

/// Test if Reedline supports history via up/down arrow.
#[test]
fn history() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"Hello World!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    pty.writer().write_all(b"Goodbye!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    // arrow up
    pty.writer().write_all(b"\x1b[A")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[4][..21], "~/reedline〉Goodbye!");

    // press Enter to execute it
    pty.writer().write_all(b"\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[5][..22], "We processed: Goodbye!");

    // arrow up twice
    pty.writer().write_all(b"\x1b[A\x1b[A")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[6][..25], "~/reedline〉Hello World!");

    // arrow down twice
    pty.writer().write_all(b"\x1b[B\x1b[B")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[6][..25], "~/reedline〉            ");

    // type "Hel" then arrow up
    pty.writer().write_all(b"Hel\x1b[A")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[6][..25], "~/reedline〉Hello World!");

    // TODO: not sure how reverse search works in Reedline

    Ok(())
}

/// Test if Reedline supports ctrl-b/ctrl-f/ctrl-left/ctrl-right style movement.
#[test]
fn word_movement() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"foo bar baz")?;

    // Ctrl-left twice, Ctrl-right once, Ctrl-b twice, Ctrl-f once.
    pty.writer().write_all(b"\x1b[1;5D\x1b[1;5D")?;
    pty.writer().write_all(b"\x1b[1;5C")?;
    pty.writer().write_all(b"\x02\x02")?;
    pty.writer().write_all(b"\x06")?;

    // Insert some more text, then press enter.
    pty.writer().write_all(b"za\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..26], "~/reedline〉foo bazar baz");
    assert_eq!(&text[1][..27], "We processed: foo bazar baz");

    Ok(())
}

/// Test if Ctrl-l clears the screen while keeping current entry.
#[test]
fn clear_screen() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"Hello World!\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"Hello again!\x0c\r")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..25], "~/reedline〉Hello again!");

    Ok(())
}

/// Test if Reedline supports common Emacs keybindings.
#[test]
fn emacs_keybinds() -> std::io::Result<()> {
    let mut pty = pty_spawn("target/debug/examples/basic", vec![], None)?;
    let mut terminal = Terminal::new();
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;

    pty.writer().write_all(b"Hello World!")?;

    // undo with Ctrl-z
    pty.writer().write_all(b"\x1a")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..25], "~/reedline〉Hello       ");

    // redo with Ctrl-g
    pty.writer().write_all(b"\x07")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..25], "~/reedline〉Hello World!");

    // delete "World" with alt+left, alt+backspace
    pty.writer().write_all(b"\x1b[1;3D\x1b\x7f")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..25], "~/reedline〉Hello !     ");

    // make "Hello" ALL CAPS with alt+b, alt+u
    pty.writer().write_all(b"\x1bb\x1bu")?;
    terminal.read_from_pty(&mut pty, Some(Duration::from_millis(50)))?;
    let text = extract_text(terminal.inner());
    assert_eq!(&text[0][..25], "~/reedline〉HELLO !     ");

    Ok(())
}
