use std::io::{stdout, Write};
use std::time::Duration;

use crossterm::{
    cursor::{position, MoveToColumn, MoveToNextLine, RestorePosition, SavePosition},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    ExecutableCommand, QueueableCommand, Result,
};

use std::io::Stdout;

mod line_buffer;

mod engine;
use engine::{EditCommand, Engine};

fn print_message(stdout: &mut Stdout, msg: &str) -> Result<()> {
    stdout
        .queue(Print("\n"))?
        .queue(MoveToColumn(1))?
        .queue(Print(msg))?
        .queue(Print("\n"))?
        .queue(MoveToColumn(1))?;
    stdout.flush()?;

    Ok(())
}

fn buffer_repaint(stdout: &mut Stdout, engine: &Engine, prompt_offset: u16) -> Result<()> {
    let raw_buffer = engine.get_buffer();
    let new_index = engine.get_insertion_point();

    // Repaint logic:
    //
    // Start after the prompt
    // Draw the string slice from 0 to the grapheme start left of insertion point
    // Then, get the position on the screen
    // Then draw the remainer of the buffer from above
    // Finally, reset the cursor to the saved position

    stdout.queue(MoveToColumn(prompt_offset))?;
    stdout.queue(Print(&raw_buffer[0..new_index]))?;
    stdout.queue(SavePosition)?;
    stdout.queue(Print(&raw_buffer[new_index..]))?;
    stdout.queue(Clear(ClearType::UntilNewLine))?;
    stdout.queue(RestorePosition)?;

    stdout.flush()?;

    Ok(())
}

// this fn is totally ripped off from crossterm's examples
// it's really a diagnostic routine to see if crossterm is
// even seeing the events. if you press a key and no events
// are printed, it's a good chance your terminal is eating
// those events.
fn print_events(stdout: &mut Stdout) -> Result<()> {
    loop {
        // Wait up to 5s for another event
        if poll(Duration::from_millis(5_000))? {
            // It's guaranteed that read() wont block if `poll` returns `Ok(true)`
            let event = read()?;

            // just reuse the print_message fn to show events
            print_message(stdout, &format!("Event::{:?}", event))?;

            // hit the esc key to git out
            if event == Event::Key(KeyCode::Esc.into()) {
                break;
            }
        } else {
            // Timeout expired, no event for 5s
            print_message(stdout, "Waiting for you to type...")?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let mut stdout = stdout();

    terminal::enable_raw_mode()?;
    // quick command like parameter handling
    let args: Vec<String> = std::env::args().collect();
    // if -k is passed, show the events
    if args.len() > 1 && args[1] == "-k" {
        print_message(&mut stdout, "Ready to print events:")?;
        print_events(&mut stdout)?;
        terminal::disable_raw_mode()?;
        println!();
        return Ok(());
    };

    let mut engine = Engine::new();

    'repl: loop {
        // print our prompt
        stdout
            .execute(SetForegroundColor(Color::Blue))?
            .execute(Print("> "))?
            .execute(ResetColor)?;

        // set where the input begins
        let (mut prompt_offset, _) = position()?;
        prompt_offset += 1;

        'input: loop {
            match read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::CONTROL,
                }) => match code {
                    KeyCode::Char('d') => {
                        if engine.get_buffer().is_empty() {
                            stdout.queue(MoveToNextLine(1))?.queue(Print("exit"))?;
                            break 'repl;
                        } else {
                            engine.run_edit_commands(&[EditCommand::Delete]);
                        }
                    }
                    KeyCode::Char('a') => {
                        engine.run_edit_commands(&[EditCommand::MoveToStart]);
                    }
                    KeyCode::Char('e') => {
                        engine.run_edit_commands(&[EditCommand::MoveToEnd]);
                    }
                    KeyCode::Char('k') => {
                        engine.run_edit_commands(&[EditCommand::CutToEnd]);
                    }
                    KeyCode::Char('u') => {
                        engine.run_edit_commands(&[EditCommand::CutFromStart]);
                    }
                    KeyCode::Char('y') => {
                        engine.run_edit_commands(&[EditCommand::InsertCutBuffer]);
                    }
                    KeyCode::Char('b') => {
                        engine.run_edit_commands(&[EditCommand::MoveLeft]);
                    }
                    KeyCode::Char('f') => {
                        engine.run_edit_commands(&[EditCommand::MoveRight]);
                    }
                    KeyCode::Char('c') => {
                        engine.run_edit_commands(&[EditCommand::Clear]);
                        stdout.queue(Print('\n'))?.queue(MoveToColumn(1))?.flush()?;
                        break 'input;
                    }
                    KeyCode::Char('h') => {
                        engine.run_edit_commands(&[EditCommand::Backspace]);
                    }
                    KeyCode::Char('w') => {
                        engine.run_edit_commands(&[EditCommand::CutWordLeft]);
                    }
                    KeyCode::Left => {
                        engine.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Right => {
                        engine.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    _ => {}
                },
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::ALT,
                }) => match code {
                    KeyCode::Char('b') => {
                        engine.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Char('f') => {
                        engine.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('d') => {
                        engine.run_edit_commands(&[EditCommand::CutWordRight]);
                    }
                    KeyCode::Left => {
                        engine.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Right => {
                        engine.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    _ => {}
                },
                Event::Key(KeyEvent { code, modifiers: _ }) => {
                    match code {
                        KeyCode::Char(c) => {
                            engine.run_edit_commands(&[
                                EditCommand::InsertChar(c),
                                EditCommand::MoveRight,
                            ]);
                        }
                        KeyCode::Backspace => {
                            engine.run_edit_commands(&[EditCommand::Backspace]);
                        }
                        KeyCode::Delete => {
                            engine.run_edit_commands(&[EditCommand::Delete]);
                        }
                        KeyCode::Home => {
                            engine.run_edit_commands(&[EditCommand::MoveToStart]);
                        }
                        KeyCode::End => {
                            engine.run_edit_commands(&[EditCommand::MoveToEnd]);
                        }
                        KeyCode::Enter => {
                            if engine.get_buffer() == "exit" {
                                break 'repl;
                            } else {
                                let buffer = String::from(engine.get_buffer());

                                engine.run_edit_commands(&[
                                    EditCommand::AppendToHistory,
                                    EditCommand::Clear,
                                ]);
                                print_message(&mut stdout, &format!("Our buffer: {}", buffer))?;

                                break 'input;
                            }
                        }
                        KeyCode::Up => {
                            engine.run_edit_commands(&[EditCommand::PreviousHistory]);
                        }
                        KeyCode::Down => {
                            // Down means: navigate forward through the history. If we reached the
                            // bottom of the history, we clear the buffer, to make it feel like
                            // zsh/bash/whatever
                            engine.run_edit_commands(&[EditCommand::NextHistory]);
                        }
                        KeyCode::Left => {
                            engine.run_edit_commands(&[EditCommand::MoveLeft]);
                        }
                        KeyCode::Right => {
                            engine.run_edit_commands(&[EditCommand::MoveRight]);
                        }
                        _ => {}
                    };
                }
                Event::Mouse(event) => {
                    print_message(&mut stdout, &format!("{:?}", event))?;
                }
                Event::Resize(width, height) => {
                    print_message(
                        &mut stdout,
                        &format!("width: {} and height: {}", width, height),
                    )?;
                }
            }
            buffer_repaint(&mut stdout, &engine, prompt_offset)?;
        }
    }
    terminal::disable_raw_mode()?;

    println!();
    Ok(())
}
