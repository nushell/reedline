use std::io::{stdout, Write};

use crossterm::{
    cursor::{position, MoveLeft, MoveRight, MoveToColumn, MoveToNextLine},
    event::read,
    event::{Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal, ExecutableCommand, QueueableCommand, Result,
};

use std::io::Stdout;

struct LineBuffer {
    buffer: String,
    caret_pos: u16,
}

impl LineBuffer {
    pub fn new() -> LineBuffer {
        LineBuffer {
            buffer: String::new(),
            caret_pos: 0,
        }
    }

    pub fn set_caret_pos(&mut self, pos: u16) {
        self.caret_pos = pos;
    }

    pub fn get_caret_pos(&self) -> u16 {
        self.caret_pos
    }

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }

    pub fn inc_caret_pos(&mut self) {
        self.caret_pos += 1;
    }

    pub fn dec_caret_pos(&mut self) {
        self.caret_pos -= 1;
    }

    pub fn get_buffer_len(&self) -> usize {
        self.buffer.len()
    }

    pub fn slice_buffer(&self, pos: usize) -> &str {
        &self.buffer[pos..]
    }

    pub fn insert_char(&mut self, pos: usize, c: char) {
        self.buffer.insert(pos, c)
    }

    pub fn remove_char(&mut self, pos: usize) -> char {
        self.buffer.remove(pos)
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn pop(&mut self) -> Option<char> {
        self.buffer.pop()
    }

    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    pub fn calculate_word_left(&mut self, input_start_col: u16) -> Option<(usize, &str)> {
        self.buffer
            .rmatch_indices(&[' ', '\t'][..])
            .find(|(index, _)| index < &(self.caret_pos as usize - input_start_col as usize - 1))
    }

    pub fn calculate_word_right(&mut self, input_start_col: u16) -> Option<(usize, &str)> {
        self.buffer
            .match_indices(&[' ', '\t'][..])
            .find(|(index, _)| index > &(self.caret_pos as usize - input_start_col as usize))
    }
}

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

fn main() -> Result<()> {
    let mut stdout = stdout();

    terminal::enable_raw_mode()?;

    let mut buffer = LineBuffer::new();

    'repl: loop {
        // print our prompt
        stdout
            .execute(SetForegroundColor(Color::Blue))?
            .execute(Print("> "))?
            .execute(ResetColor)?;

        // set where the input begins
        let (mut input_start_col, _) = position()?;
        input_start_col += 1;
        buffer.set_caret_pos(input_start_col);

        'input: loop {
            match read()? {
                Event::Key(KeyEvent { code, modifiers }) => {
                    match code {
                        KeyCode::Char(c) => {
                            if modifiers == KeyModifiers::CONTROL && c == 'd' {
                                stdout.queue(MoveToNextLine(1))?.queue(Print("exit"))?;
                                break 'repl;
                            }
                            let insertion_point =
                                buffer.get_caret_pos() as usize - input_start_col as usize;
                            if insertion_point == buffer.get_buffer_len() {
                                stdout.queue(Print(c))?;
                            } else {
                                stdout
                                    .queue(Print(c))?
                                    .queue(Print(buffer.slice_buffer(insertion_point)))?
                                    .queue(MoveToColumn(buffer.get_caret_pos() + 1))?;
                            }
                            stdout.flush()?;
                            buffer.inc_caret_pos();
                            buffer.insert_char(insertion_point, c);
                        }
                        KeyCode::Backspace => {
                            let insertion_point =
                                buffer.get_caret_pos() as usize - input_start_col as usize;
                            if insertion_point == buffer.get_buffer_len() && !buffer.is_empty() {
                                buffer.pop();
                                stdout
                                    .queue(MoveLeft(1))?
                                    .queue(Print(' '))?
                                    .queue(MoveLeft(1))?;
                                stdout.flush()?;
                                buffer.dec_caret_pos();
                            } else if insertion_point < buffer.get_buffer_len()
                                && !buffer.is_empty()
                            {
                                buffer.remove_char(insertion_point - 1);
                                stdout
                                    .queue(MoveLeft(1))?
                                    .queue(Print(buffer.slice_buffer(insertion_point - 1)))?
                                    .queue(Print(' '))?
                                    .queue(MoveToColumn(buffer.get_caret_pos() - 1))?;
                                stdout.flush()?;
                                buffer.dec_caret_pos();
                            }
                        }
                        KeyCode::Delete => {
                            let insertion_point =
                                buffer.get_caret_pos() as usize - input_start_col as usize;
                            if insertion_point < buffer.get_buffer_len() && !buffer.is_empty() {
                                buffer.remove_char(insertion_point);
                                stdout
                                    .queue(Print(buffer.slice_buffer(insertion_point)))?
                                    .queue(Print(' '))?
                                    .queue(MoveToColumn(buffer.get_caret_pos()))?;
                                stdout.flush()?;
                            }
                        }
                        KeyCode::Enter => {
                            if buffer.get_buffer() == "exit" {
                                break 'repl;
                            } else {
                                print_message(
                                    &mut stdout,
                                    &format!("Our buffer: {}", buffer.get_buffer()),
                                )?;
                                buffer.clear();
                                break 'input;
                            }
                        }
                        KeyCode::Left => {
                            if buffer.get_caret_pos() > input_start_col {
                                // If the ALT modifier is set, we want to jump words for more
                                // natural editing. Jumping words basically means: move to next
                                // whitespace in the given direction.
                                if modifiers == KeyModifiers::ALT {
                                    let whitespace_index =
                                        buffer.calculate_word_left(input_start_col);
                                    match whitespace_index {
                                        Some((index, _)) => {
                                            stdout.queue(MoveToColumn(
                                                index as u16 + input_start_col + 1,
                                            ))?;
                                            buffer
                                                .set_caret_pos(input_start_col + index as u16 + 1);
                                        }
                                        None => {
                                            stdout.queue(MoveToColumn(input_start_col))?;
                                            buffer.set_caret_pos(input_start_col);
                                        }
                                    }
                                } else {
                                    stdout.queue(MoveLeft(1))?;
                                    buffer.dec_caret_pos();
                                }
                                stdout.flush()?;
                            }
                        }
                        KeyCode::Right => {
                            if (buffer.get_caret_pos() as usize)
                                < ((input_start_col as usize) + buffer.get_buffer_len())
                            {
                                if modifiers == KeyModifiers::ALT {
                                    let whitespace_index =
                                        buffer.calculate_word_right(input_start_col);
                                    match whitespace_index {
                                        Some((index, _)) => {
                                            stdout.queue(MoveToColumn(
                                                index as u16 + input_start_col + 1,
                                            ))?;
                                            buffer
                                                .set_caret_pos(input_start_col + index as u16 + 1);
                                        }
                                        None => {
                                            stdout.queue(MoveToColumn(
                                                buffer.get_buffer_len() as u16 + input_start_col,
                                            ))?;
                                            buffer.set_caret_pos(
                                                buffer.get_buffer_len() as u16 + input_start_col,
                                            );
                                        }
                                    }
                                } else {
                                    stdout.queue(MoveRight(1))?;
                                    buffer.inc_caret_pos();
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
