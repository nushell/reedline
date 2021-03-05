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
    insertion_point: usize,
}

impl LineBuffer {
    pub fn new() -> LineBuffer {
        LineBuffer {
            buffer: String::new(),
            insertion_point: 0,
        }
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
        self.insertion_point = pos;
    }

    pub fn get_insertion_point(&self) -> usize {
        self.insertion_point
    }

    pub fn get_buffer(&self) -> &str {
        &self.buffer
    }

    pub fn inc_insertion_point(&mut self) {
        self.insertion_point += 1;
    }

    pub fn dec_insertion_point(&mut self) {
        self.insertion_point -= 1;
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

    pub fn move_word_left(&mut self) -> usize {
        match self
            .buffer
            .rmatch_indices(&[' ', '\t'][..])
            .find(|(index, _)| index < &(self.insertion_point - 1))
        {
            Some((index, _)) => {
                self.insertion_point = index + 1;
                self.insertion_point
            }
            None => {
                self.insertion_point = 0;
                self.insertion_point
            }
        }
    }

    pub fn move_word_right(&mut self) -> usize {
        match self
            .buffer
            .match_indices(&[' ', '\t'][..])
            .find(|(index, _)| index > &(self.insertion_point))
        {
            Some((index, _)) => {
                self.insertion_point = index + 1;
                self.insertion_point
            }
            None => {
                self.insertion_point = self.get_buffer_len();
                self.insertion_point
            }
        }
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
        let (mut prompt_offset, _) = position()?;
        prompt_offset += 1;

        'input: loop {
            match read()? {
                Event::Key(KeyEvent { code, modifiers }) => {
                    match code {
                        KeyCode::Char(c) => {
                            if modifiers == KeyModifiers::CONTROL && c == 'd' {
                                stdout.queue(MoveToNextLine(1))?.queue(Print("exit"))?;
                                break 'repl;
                            }
                            let insertion_point = buffer.get_insertion_point();
                            if insertion_point == buffer.get_buffer_len() {
                                stdout.queue(Print(c))?;
                            } else {
                                stdout
                                    .queue(Print(c))?
                                    .queue(Print(buffer.slice_buffer(insertion_point)))?
                                    .queue(MoveToColumn(
                                        insertion_point as u16 + prompt_offset + 1,
                                    ))?;
                            }
                            stdout.flush()?;
                            buffer.inc_insertion_point();
                            buffer.insert_char(insertion_point, c);
                        }
                        KeyCode::Backspace => {
                            let insertion_point = buffer.get_insertion_point();
                            if insertion_point == buffer.get_buffer_len() && !buffer.is_empty() {
                                buffer.pop();
                                stdout
                                    .queue(MoveLeft(1))?
                                    .queue(Print(' '))?
                                    .queue(MoveLeft(1))?;
                                stdout.flush()?;
                                buffer.dec_insertion_point();
                            } else if insertion_point < buffer.get_buffer_len()
                                && !buffer.is_empty()
                            {
                                buffer.remove_char(insertion_point - 1);
                                stdout
                                    .queue(MoveLeft(1))?
                                    .queue(Print(buffer.slice_buffer(insertion_point - 1)))?
                                    .queue(Print(' '))?
                                    .queue(MoveToColumn(
                                        insertion_point as u16 + prompt_offset - 1,
                                    ))?;
                                stdout.flush()?;
                                buffer.dec_insertion_point();
                            }
                        }
                        KeyCode::Delete => {
                            let insertion_point = buffer.get_insertion_point();
                            if insertion_point < buffer.get_buffer_len() && !buffer.is_empty() {
                                buffer.remove_char(insertion_point);
                                stdout
                                    .queue(Print(buffer.slice_buffer(insertion_point)))?
                                    .queue(Print(' '))?
                                    .queue(MoveToColumn(insertion_point as u16 + prompt_offset))?;
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
                                buffer.set_insertion_point(0);
                                break 'input;
                            }
                        }
                        KeyCode::Left => {
                            if buffer.get_insertion_point() > 0 {
                                // If the ALT modifier is set, we want to jump words for more
                                // natural editing. Jumping words basically means: move to next
                                // whitespace in the given direction.
                                if modifiers == KeyModifiers::ALT {
                                    let new_insertion_point = buffer.move_word_left();
                                    stdout.queue(MoveToColumn(
                                        new_insertion_point as u16 + prompt_offset,
                                    ))?;
                                } else {
                                    stdout.queue(MoveLeft(1))?;
                                    buffer.dec_insertion_point();
                                }
                                stdout.flush()?;
                            }
                        }
                        KeyCode::Right => {
                            if buffer.get_insertion_point() < buffer.get_buffer_len() {
                                if modifiers == KeyModifiers::ALT {
                                    let new_insertion_point = buffer.move_word_right();
                                    stdout.queue(MoveToColumn(
                                        new_insertion_point as u16 + prompt_offset,
                                    ))?;
                                } else {
                                    stdout.queue(MoveRight(1))?;
                                    buffer.inc_insertion_point();
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
