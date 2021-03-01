use std::io::{stdout, Write};

use crossterm::{
    cursor::{position, MoveLeft, MoveRight, MoveToColumn},
    event::read,
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal, ExecutableCommand, QueueableCommand, Result,
};

use std::io::Stdout;
use terminal::ScrollUp;

fn print_message(stdout: &mut Stdout, msg: &str) -> Result<()> {
    stdout
        .queue(ScrollUp(1))?
        .queue(MoveToColumn(1))?
        .queue(Print(msg))?
        .queue(ScrollUp(1))?
        .queue(MoveToColumn(1))?;
    stdout.flush()?;

    Ok(())
}

fn main() -> Result<()> {
    let mut stdout = stdout();

    let mut buffer = String::new();
    let mut caret_pos;

    terminal::enable_raw_mode()?;

    'repl: loop {
        // print our prompt
        stdout
            .execute(SetForegroundColor(Color::Blue))?
            .execute(Print("> "))?
            .execute(ResetColor)?;

        // set where the input begins
        let (mut input_start_col, _) = position()?;
        input_start_col += 1;
        caret_pos = input_start_col;

        'input: loop {
            match read()? {
                Event::Key(KeyEvent { code, modifiers }) => {
                    match code {
                        KeyCode::Char(c) => {
                            let insertion_point = caret_pos as usize - input_start_col as usize;
                            if insertion_point == buffer.len() {
                                stdout.queue(Print(c))?;
                            } else {
                                stdout
                                    .queue(Print(c))?
                                    .queue(Print(&buffer[insertion_point..]))?
                                    .queue(MoveToColumn(caret_pos + 1))?;
                            }
                            stdout.flush()?;
                            caret_pos += 1;
                            buffer.insert(insertion_point, c);
                        }
                        KeyCode::Backspace => {
                            let insertion_point = caret_pos as usize - input_start_col as usize;
                            if insertion_point == buffer.len() && !buffer.is_empty() {
                                buffer.pop();
                                stdout
                                    .queue(MoveLeft(1))?
                                    .queue(Print(' '))?
                                    .queue(MoveLeft(1))?;
                                stdout.flush()?;
                                caret_pos -= 1;
                            } else if insertion_point < buffer.len() && !buffer.is_empty() {
                                buffer.remove(insertion_point - 1);
                                stdout
                                    .queue(MoveLeft(1))?
                                    .queue(Print(&buffer[(insertion_point - 1)..]))?
                                    .queue(Print(' '))?
                                    .queue(MoveToColumn(caret_pos - 1))?;
                                stdout.flush()?;
                                caret_pos -= 1;
                            }
                        }
                        KeyCode::Delete => {
                            let insertion_point = caret_pos as usize - input_start_col as usize;
                            if insertion_point < buffer.len() && !buffer.is_empty() {
                                buffer.remove(insertion_point);
                                stdout
                                    .queue(Print(&buffer[insertion_point..]))?
                                    .queue(Print(' '))?
                                    .queue(MoveToColumn(caret_pos))?;
                                stdout.flush()?;
                            }
                        }
                        KeyCode::Enter => {
                            if buffer == "exit" {
                                break 'repl;
                            } else {
                                print_message(&mut stdout, &format!("Our buffer: {}", buffer))?;
                                buffer.clear();
                                break 'input;
                            }
                        }
                        KeyCode::Left => {
                            if caret_pos > input_start_col {
                                // If the ALT modifier is set, we want to jump words for more
                                // natural editing. Jumping words basically means: move to next
                                // whitespace in the given direction.
                                if modifiers == KeyModifiers::ALT {
                                    let whitespace_index = buffer
                                        .rmatch_indices(&[' ', '\t'][..])
                                        .find(|(index, _)| {
                                            index
                                                < &(caret_pos as usize
                                                    - input_start_col as usize
                                                    - 1)
                                        });

                                    match whitespace_index {
                                        Some((index, _)) => {
                                            stdout.queue(MoveToColumn(
                                                index as u16 + input_start_col + 1,
                                            ))?;
                                            caret_pos = input_start_col + index as u16 + 1;
                                        }
                                        None => {
                                            stdout.queue(MoveToColumn(input_start_col))?;
                                            caret_pos = input_start_col;
                                        }
                                    }
                                } else {
                                    stdout.queue(MoveLeft(1))?;
                                    caret_pos -= 1;
                                }
                                stdout.flush()?;
                            }
                        }
                        KeyCode::Right => {
                            if (caret_pos as usize) < ((input_start_col as usize) + buffer.len()) {
                                if modifiers == KeyModifiers::ALT {
                                    let whitespace_index = buffer
                                        .match_indices(&[' ', '\t'][..])
                                        .find(|(index, _)| {
                                            index > &(caret_pos as usize - input_start_col as usize)
                                        });

                                    match whitespace_index {
                                        Some((index, _)) => {
                                            stdout.queue(MoveToColumn(
                                                index as u16 + input_start_col + 1,
                                            ))?;
                                            caret_pos = input_start_col + index as u16 + 1;
                                        }
                                        None => {
                                            stdout.queue(MoveToColumn(
                                                buffer.len() as u16 + input_start_col,
                                            ))?;
                                            caret_pos = buffer.len() as u16 + input_start_col;
                                        }
                                    }
                                } else {
                                    stdout.queue(MoveRight(1))?;
                                    caret_pos += 1;
                                }
                                stdout.flush()?;
                            }
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
        }
    }

    terminal::disable_raw_mode()?;

    println!();
    Ok(())
}
