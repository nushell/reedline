use std::io::{Stdout, Write};

use crossterm::{
    cursor::{position, MoveToColumn, RestorePosition, SavePosition},
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
    ExecutableCommand, QueueableCommand, Result,
};

use std::collections::VecDeque;

use crate::line_buffer::LineBuffer;

const HISTORY_SIZE: usize = 100;

pub enum EditCommand {
    MoveToStart,
    MoveToEnd,
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    InsertChar(char),
    Backspace,
    Delete,
    AppendToHistory,
    PreviousHistory,
    NextHistory,
    Clear,
    CutFromStart,
    CutToEnd,
    CutWordLeft,
    CutWordRight,
    InsertCutBuffer,
    UppercaseWord,
    LowercaseWord,
    CapitalizeChar,
    SwapWords,
    SwapGraphemes,
}

pub struct Engine {
    line_buffer: LineBuffer,

    // Cut buffer
    cut_buffer: String,

    // History
    history: VecDeque<String>,
    history_cursor: i64,
    has_history: bool,
}

pub fn print_message(stdout: &mut Stdout, msg: &str) -> Result<()> {
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
    let new_index = engine.get_insertion_point();

    // Repaint logic:
    //
    // Start after the prompt
    // Draw the string slice from 0 to the grapheme start left of insertion point
    // Then, get the position on the screen
    // Then draw the remainer of the buffer from above
    // Finally, reset the cursor to the saved position

    stdout.queue(MoveToColumn(prompt_offset))?;
    stdout.queue(Print(&engine.line_buffer[0..new_index]))?;
    stdout.queue(SavePosition)?;
    stdout.queue(Print(&engine.line_buffer[new_index..]))?;
    stdout.queue(Clear(ClearType::UntilNewLine))?;
    stdout.queue(RestorePosition)?;

    stdout.flush()?;

    Ok(())
}

impl Engine {
    pub fn new() -> Engine {
        let history = VecDeque::with_capacity(HISTORY_SIZE);
        let history_cursor = -1i64;
        let has_history = false;
        let cut_buffer = String::new();

        Engine {
            line_buffer: LineBuffer::new(),
            cut_buffer,
            history,
            history_cursor,
            has_history,
        }
    }

    pub fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        for command in commands {
            match command {
                EditCommand::MoveToStart => self.line_buffer.set_insertion_point(0),
                EditCommand::MoveToEnd => {
                    self.line_buffer.move_to_end();
                }
                EditCommand::MoveLeft => self.line_buffer.move_left(),
                EditCommand::MoveRight => self.line_buffer.move_right(),
                EditCommand::MoveWordLeft => {
                    self.line_buffer.move_word_left();
                }
                EditCommand::MoveWordRight => {
                    self.line_buffer.move_word_right();
                }
                EditCommand::InsertChar(c) => {
                    let insertion_point = self.line_buffer.get_insertion_point();
                    self.line_buffer.insert_char(insertion_point, *c)
                }
                EditCommand::Backspace => {
                    let left_index = self.line_buffer.grapheme_left_index();
                    if left_index < self.get_insertion_point() {
                        self.clear_range(left_index..self.get_insertion_point());
                        self.set_insertion_point(left_index);
                    }
                }
                EditCommand::Delete => {
                    let right_index = self.line_buffer.grapheme_right_index();
                    if right_index > self.get_insertion_point() {
                        self.clear_range(self.get_insertion_point()..right_index);
                    }
                }
                EditCommand::Clear => {
                    self.line_buffer.clear();
                    self.set_insertion_point(0);
                }
                EditCommand::AppendToHistory => {
                    if self.history.len() + 1 == HISTORY_SIZE {
                        // History is "full", so we delete the oldest entry first,
                        // before adding a new one.
                        self.history.pop_back();
                    }
                    self.history.push_front(self.line_buffer.to_owned());
                    self.has_history = true;
                    // reset the history cursor - we want to start at the bottom of the
                    // history again.
                    self.history_cursor = -1;
                }
                EditCommand::PreviousHistory => {
                    if self.has_history && self.history_cursor < (self.history.len() as i64 - 1) {
                        self.history_cursor += 1;
                        let history_entry = self
                            .history
                            .get(self.history_cursor as usize)
                            .unwrap()
                            .clone();
                        self.set_buffer(history_entry.clone());
                        self.move_to_end();
                    }
                }
                EditCommand::NextHistory => {
                    if self.history_cursor >= 0 {
                        self.history_cursor -= 1;
                    }
                    let new_buffer = if self.history_cursor < 0 {
                        String::new()
                    } else {
                        // We can be sure that we always have an entry on hand, that's why
                        // unwrap is fine.
                        self.history
                            .get(self.history_cursor as usize)
                            .unwrap()
                            .clone()
                    };

                    self.set_buffer(new_buffer.clone());
                    self.move_to_end();
                }
                EditCommand::CutFromStart => {
                    if self.get_insertion_point() > 0 {
                        self.cut_buffer
                            .replace_range(.., &self.line_buffer[..self.get_insertion_point()]);
                        self.clear_to_insertion_point();
                    }
                }
                EditCommand::CutToEnd => {
                    let cut_slice = &self.line_buffer[self.get_insertion_point()..];
                    if !cut_slice.is_empty() {
                        self.cut_buffer.replace_range(.., cut_slice);
                        self.clear_to_end();
                    }
                }
                EditCommand::CutWordLeft => {
                    let left_index = self.line_buffer.word_left_index();
                    if left_index < self.get_insertion_point() {
                        let cut_range = left_index..self.get_insertion_point();
                        self.cut_buffer
                            .replace_range(.., &self.line_buffer[cut_range.clone()]);
                        self.clear_range(cut_range);
                        self.set_insertion_point(left_index);
                    }
                }
                EditCommand::CutWordRight => {
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > self.get_insertion_point() {
                        let cut_range = self.get_insertion_point()..right_index;
                        self.cut_buffer
                            .replace_range(.., &self.line_buffer[cut_range.clone()]);
                        self.clear_range(cut_range);
                    }
                }
                EditCommand::InsertCutBuffer => {
                    self.line_buffer
                        .insert_str(self.get_insertion_point(), &self.cut_buffer);
                    self.set_insertion_point(self.get_insertion_point() + self.cut_buffer.len());
                }
                EditCommand::UppercaseWord => {
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > self.get_insertion_point() {
                        let change_range = self.get_insertion_point()..right_index;
                        let uppercased = self.line_buffer[change_range.clone()].to_uppercase();
                        self.line_buffer.replace_range(change_range, &uppercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::LowercaseWord => {
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > self.get_insertion_point() {
                        let change_range = self.get_insertion_point()..right_index;
                        let lowercased = self.line_buffer[change_range.clone()].to_lowercase();
                        self.line_buffer.replace_range(change_range, &lowercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::CapitalizeChar => {
                    if self.line_buffer.on_whitespace() {
                        self.line_buffer.move_word_right();
                        self.line_buffer.move_word_left();
                    }
                    let right_index = self.line_buffer.grapheme_right_index();
                    if right_index > self.get_insertion_point() {
                        let change_range = self.get_insertion_point()..right_index;
                        let uppercased = self.line_buffer[change_range.clone()].to_uppercase();
                        self.line_buffer.replace_range(change_range, &uppercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::SwapWords => {
                    let old_insertion_point = self.get_insertion_point();
                    self.line_buffer.move_word_right();
                    let word_2_end = self.get_insertion_point();
                    self.line_buffer.move_word_left();
                    let word_2_start = self.get_insertion_point();
                    self.line_buffer.move_word_left();
                    let word_1_start = self.get_insertion_point();
                    let word_1_end = self.line_buffer.word_right_index();

                    if word_1_start < word_1_end
                        && word_1_end < word_2_start
                        && word_2_start < word_2_end
                    {
                        let word_1 = self.line_buffer[word_1_start..word_1_end].to_string();
                        let word_2 = self.line_buffer[word_2_start..word_2_end].to_string();
                        self.line_buffer
                            .replace_range(word_2_start..word_2_end, &word_1);
                        self.line_buffer
                            .replace_range(word_1_start..word_1_end, &word_2);
                        self.set_insertion_point(word_2_end);
                    } else {
                        self.set_insertion_point(old_insertion_point);
                    }
                }
                EditCommand::SwapGraphemes => {
                    if self.get_insertion_point() == 0 {
                        self.line_buffer.move_right()
                    } else if self.get_insertion_point() == self.line_buffer.len() {
                        self.line_buffer.move_left()
                    }
                    let insertion_point = self.get_insertion_point();
                    let grapheme_1_start = self.line_buffer.grapheme_left_index();
                    let grapheme_2_end = self.line_buffer.grapheme_right_index();

                    if grapheme_1_start < insertion_point && grapheme_2_end > insertion_point {
                        let grapheme_1 =
                            self.line_buffer[grapheme_1_start..insertion_point].to_string();
                        let grapheme_2 =
                            self.line_buffer[insertion_point..grapheme_2_end].to_string();
                        self.line_buffer
                            .replace_range(insertion_point..grapheme_2_end, &grapheme_1);
                        self.line_buffer
                            .replace_range(grapheme_1_start..insertion_point, &grapheme_2);
                        self.set_insertion_point(grapheme_2_end);
                    } else {
                        self.set_insertion_point(insertion_point);
                    }
                }
            }
        }
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
        self.line_buffer.set_insertion_point(pos)
    }

    pub fn get_insertion_point(&self) -> usize {
        self.line_buffer.get_insertion_point()
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    pub fn move_to_end(&mut self) -> usize {
        self.line_buffer.move_to_end()
    }

    pub fn clear_to_end(&mut self) {
        self.line_buffer.clear_to_end()
    }

    pub fn clear_to_insertion_point(&mut self) {
        self.line_buffer.clear_to_insertion_point()
    }

    pub fn clear_range<R>(&mut self, range: R)
    where
        R: std::ops::RangeBounds<usize>,
    {
        self.line_buffer.clear_range(range)
    }

    pub fn read_line(&mut self, stdout: &mut Stdout) -> Result<String> {
        // print our prompt
        stdout
            .execute(SetForegroundColor(Color::Blue))?
            .execute(Print("〉"))?
            .execute(ResetColor)?;

        // set where the input begins
        let (mut prompt_offset, _) = position()?;
        prompt_offset += 1;

        loop {
            match read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::CONTROL,
                }) => match code {
                    KeyCode::Char('d') => {
                        if self.line_buffer.is_empty() {
                            return Ok("exit".to_string());
                        } else {
                            self.run_edit_commands(&[EditCommand::Delete]);
                        }
                    }
                    KeyCode::Char('a') => {
                        self.run_edit_commands(&[EditCommand::MoveToStart]);
                    }
                    KeyCode::Char('e') => {
                        self.run_edit_commands(&[EditCommand::MoveToEnd]);
                    }
                    KeyCode::Char('k') => {
                        self.run_edit_commands(&[EditCommand::CutToEnd]);
                    }
                    KeyCode::Char('u') => {
                        self.run_edit_commands(&[EditCommand::CutFromStart]);
                    }
                    KeyCode::Char('y') => {
                        self.run_edit_commands(&[EditCommand::InsertCutBuffer]);
                    }
                    KeyCode::Char('b') => {
                        self.run_edit_commands(&[EditCommand::MoveLeft]);
                    }
                    KeyCode::Char('f') => {
                        self.run_edit_commands(&[EditCommand::MoveRight]);
                    }
                    KeyCode::Char('c') => {
                        return Ok("".to_string());
                    }
                    KeyCode::Char('h') => {
                        self.run_edit_commands(&[EditCommand::Backspace]);
                    }
                    KeyCode::Char('w') => {
                        self.run_edit_commands(&[EditCommand::CutWordLeft]);
                    }
                    KeyCode::Left => {
                        self.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Right => {
                        self.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('p') => {
                        self.run_edit_commands(&[EditCommand::PreviousHistory]);
                    }
                    KeyCode::Char('n') => {
                        self.run_edit_commands(&[EditCommand::NextHistory]);
                    }
                    KeyCode::Char('t') => {
                        self.run_edit_commands(&[EditCommand::SwapGraphemes]);
                    }
                    _ => {}
                },
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::ALT,
                }) => match code {
                    KeyCode::Char('b') => {
                        self.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Char('f') => {
                        self.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('d') => {
                        self.run_edit_commands(&[EditCommand::CutWordRight]);
                    }
                    KeyCode::Left => {
                        self.run_edit_commands(&[EditCommand::MoveWordLeft]);
                    }
                    KeyCode::Right => {
                        self.run_edit_commands(&[EditCommand::MoveWordRight]);
                    }
                    KeyCode::Char('u') => {
                        self.run_edit_commands(&[EditCommand::UppercaseWord]);
                    }
                    KeyCode::Char('l') => {
                        self.run_edit_commands(&[EditCommand::LowercaseWord]);
                    }
                    KeyCode::Char('c') => {
                        self.run_edit_commands(&[EditCommand::CapitalizeChar]);
                    }
                    KeyCode::Char('t') => {
                        self.run_edit_commands(&[EditCommand::SwapWords]);
                    }
                    _ => {}
                },
                Event::Key(KeyEvent { code, modifiers: _ }) => {
                    match code {
                        KeyCode::Char(c) => {
                            self.run_edit_commands(&[
                                EditCommand::InsertChar(c),
                                EditCommand::MoveRight,
                            ]);
                        }
                        KeyCode::Backspace => {
                            self.run_edit_commands(&[EditCommand::Backspace]);
                        }
                        KeyCode::Delete => {
                            self.run_edit_commands(&[EditCommand::Delete]);
                        }
                        KeyCode::Home => {
                            self.run_edit_commands(&[EditCommand::MoveToStart]);
                        }
                        KeyCode::End => {
                            self.run_edit_commands(&[EditCommand::MoveToEnd]);
                        }
                        KeyCode::Enter => {
                            let buffer = self.line_buffer.to_owned();

                            self.run_edit_commands(&[
                                EditCommand::AppendToHistory,
                                EditCommand::Clear,
                            ]);

                            return Ok(buffer);
                        }
                        KeyCode::Up => {
                            self.run_edit_commands(&[EditCommand::PreviousHistory]);
                        }
                        KeyCode::Down => {
                            // Down means: navigate forward through the history. If we reached the
                            // bottom of the history, we clear the buffer, to make it feel like
                            // zsh/bash/whatever
                            self.run_edit_commands(&[EditCommand::NextHistory]);
                        }
                        KeyCode::Left => {
                            self.run_edit_commands(&[EditCommand::MoveLeft]);
                        }
                        KeyCode::Right => {
                            self.run_edit_commands(&[EditCommand::MoveRight]);
                        }
                        _ => {}
                    };
                }
                Event::Mouse(event) => {
                    print_message(stdout, &format!("{:?}", event))?;
                }
                Event::Resize(width, height) => {
                    print_message(stdout, &format!("width: {} and height: {}", width, height))?;
                }
            }
            buffer_repaint(stdout, &self, prompt_offset)?;
        }
    }
}
