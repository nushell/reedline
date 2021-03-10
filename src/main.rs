use std::io::{stdout, Write};

use crossterm::{
    cursor::{position, MoveToColumn, MoveToNextLine, RestorePosition, SavePosition},
    event::read,
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    ExecutableCommand, QueueableCommand, Result,
};

use std::collections::VecDeque;
use std::io::Stdout;

mod line_buffer;
use line_buffer::LineBuffer;

const HISTORY_SIZE: usize = 100;

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

fn buffer_repaint(stdout: &mut Stdout, buffer: &LineBuffer, prompt_offset: u16) -> Result<()> {
    let raw_buffer = buffer.get_buffer();
    let new_index = buffer.get_insertion_point();

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

fn main() -> Result<()> {
    let mut stdout = stdout();

    terminal::enable_raw_mode()?;

    let mut buffer = LineBuffer::new();
    let mut history = VecDeque::with_capacity(HISTORY_SIZE);
    let mut history_cursor = -1i64;
    let mut has_history = false;
    let mut cut_buffer = String::new();

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
                        if buffer.get_buffer().is_empty() {
                            stdout.queue(MoveToNextLine(1))?.queue(Print("exit"))?;
                            break 'repl;
                        } else {
                            let insertion_point = buffer.get_insertion_point();
                            if insertion_point < buffer.get_buffer_len() && !buffer.is_empty() {
                                buffer.remove_char(insertion_point);
                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                            }
                        }
                    }
                    KeyCode::Char('a') => {
                        buffer.set_insertion_point(0);
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('e') => {
                        let buffer_len = buffer.get_buffer_len();
                        buffer.set_insertion_point(buffer_len);
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('k') => {
                        let cut_slice = &buffer.get_buffer()[buffer.get_insertion_point()..];
                        if !cut_slice.is_empty() {
                            cut_buffer.replace_range(.., cut_slice);
                            buffer.clear_to_end();
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    KeyCode::Char('u') => {
                        if buffer.get_insertion_point() > 0 {
                            cut_buffer.replace_range(
                                ..,
                                &buffer.get_buffer()[..buffer.get_insertion_point()],
                            );
                            buffer.clear_to_insertion_point();
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    KeyCode::Char('y') => {
                        buffer.insert_str(buffer.get_insertion_point(), &cut_buffer);
                        buffer.set_insertion_point(buffer.get_insertion_point() + cut_buffer.len());
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('b') => {
                        buffer.dec_insertion_point();
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('f') => {
                        buffer.inc_insertion_point();
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('c') => {
                        buffer.clear();
                        stdout.queue(Print('\n'))?.queue(MoveToColumn(1))?.flush()?;
                        break 'input;
                    }
                    KeyCode::Char('h') => {
                        let insertion_point = buffer.get_insertion_point();
                        if insertion_point == buffer.get_buffer_len() && !buffer.is_empty() {
                            buffer.pop();
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        } else if insertion_point < buffer.get_buffer_len()
                            && insertion_point > 0
                            && !buffer.is_empty()
                        {
                            buffer.dec_insertion_point();
                            let insertion_point = buffer.get_insertion_point();
                            buffer.remove_char(insertion_point);
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    KeyCode::Char('w') => {
                        let old_insertion_point = buffer.get_insertion_point();
                        buffer.move_word_left();
                        if buffer.get_insertion_point() < old_insertion_point {
                            cut_buffer.replace_range(
                                ..,
                                &buffer.get_buffer()
                                    [buffer.get_insertion_point()..old_insertion_point],
                            );
                            buffer.clear_range(buffer.get_insertion_point()..old_insertion_point);
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    _ => {}
                },
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::ALT,
                }) => match code {
                    KeyCode::Char('b') => {
                        buffer.move_word_left();
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('f') => {
                        buffer.move_word_right();
                        buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                    }
                    KeyCode::Char('d') => {
                        let old_insertion_point = buffer.get_insertion_point();
                        buffer.move_word_right();
                        if buffer.get_insertion_point() > old_insertion_point {
                            cut_buffer.replace_range(
                                ..,
                                &buffer.get_buffer()
                                    [old_insertion_point..buffer.get_insertion_point()],
                            );
                            buffer.clear_range(old_insertion_point..buffer.get_insertion_point());
                            buffer.set_insertion_point(old_insertion_point);
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    KeyCode::Left => {
                        if buffer.get_insertion_point() > 0 {
                            // If the ALT modifier is set, we want to jump words for more
                            // natural editing. Jumping words basically means: move to next
                            // whitespace in the given direction.
                            buffer.move_word_left();
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    KeyCode::Right => {
                        if buffer.get_insertion_point() < buffer.get_buffer_len() {
                            buffer.move_word_right();
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                    }
                    _ => {}
                },
                Event::Key(KeyEvent { code, modifiers }) => {
                    match code {
                        KeyCode::Char(c) if modifiers == KeyModifiers::CONTROL && c == 'd' => {
                            stdout.queue(MoveToNextLine(1))?.queue(Print("exit"))?;
                            break 'repl;
                        }
                        KeyCode::Char(c) => {
                            buffer.insert_char(buffer.get_insertion_point(), c);
                            buffer.inc_insertion_point();

                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                        KeyCode::Backspace => {
                            let insertion_point = buffer.get_insertion_point();
                            if insertion_point == buffer.get_buffer_len() && !buffer.is_empty() {
                                // buffer.dec_insertion_point();
                                buffer.pop();
                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                            } else if insertion_point < buffer.get_buffer_len()
                                && insertion_point > 0
                                && !buffer.is_empty()
                            {
                                buffer.dec_insertion_point();
                                let insertion_point = buffer.get_insertion_point();
                                buffer.remove_char(insertion_point);

                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                            }
                        }
                        KeyCode::Delete => {
                            let insertion_point = buffer.get_insertion_point();
                            if insertion_point < buffer.get_buffer_len() && !buffer.is_empty() {
                                buffer.remove_char(insertion_point);
                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                            }
                        }
                        KeyCode::Home => {
                            buffer.set_insertion_point(0);
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                        KeyCode::End => {
                            let buffer_len = buffer.get_buffer_len();
                            buffer.set_insertion_point(buffer_len);
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                        KeyCode::Enter => {
                            if buffer.get_buffer() == "exit" {
                                break 'repl;
                            } else {
                                if history.len() + 1 == HISTORY_SIZE {
                                    // History is "full", so we delete the oldest entry first,
                                    // before adding a new one.
                                    history.pop_back();
                                }
                                history.push_front(String::from(buffer.get_buffer()));
                                has_history = true;
                                // reset the history cursor - we want to start at the bottom of the
                                // history again.
                                history_cursor = -1;
                                print_message(
                                    &mut stdout,
                                    &format!("Our buffer: {}", buffer.get_buffer()),
                                )?;
                                buffer.clear();
                                buffer.set_insertion_point(0);
                                break 'input;
                            }
                        }
                        KeyCode::Up => {
                            // Up means: navigate through the history.
                            if has_history && history_cursor < (history.len() as i64 - 1) {
                                history_cursor += 1;
                                let history_entry =
                                    history.get(history_cursor as usize).unwrap().clone();
                                buffer.set_buffer(history_entry.clone());
                                buffer.move_to_end();
                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                            }
                        }
                        KeyCode::Down => {
                            // Down means: navigate forward through the history. If we reached the
                            // bottom of the history, we clear the buffer, to make it feel like
                            // zsh/bash/whatever
                            if history_cursor >= 0 {
                                history_cursor -= 1;
                            }
                            let new_buffer = if history_cursor < 0 {
                                String::new()
                            } else {
                                // We can be sure that we always have an entry on hand, that's why
                                // unwrap is fine.
                                history.get(history_cursor as usize).unwrap().clone()
                            };

                            buffer.set_buffer(new_buffer.clone());
                            buffer.move_to_end();
                            buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                        }
                        KeyCode::Left => {
                            if buffer.get_insertion_point() > 0 {
                                // If the ALT modifier is set, we want to jump words for more
                                // natural editing. Jumping words basically means: move to next
                                // whitespace in the given direction.
                                buffer.dec_insertion_point();
                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
                            }
                        }
                        KeyCode::Right => {
                            if buffer.get_insertion_point() < buffer.get_buffer_len() {
                                buffer.inc_insertion_point();
                                buffer_repaint(&mut stdout, &buffer, prompt_offset)?;
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
