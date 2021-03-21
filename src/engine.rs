use crate::line_buffer::LineBuffer;
use crate::Prompt;
use crate::{
    history_search::{BasicSearch, BasicSearchCommand},
    line_buffer::InsertionPoint,
};
use crossterm::{
    cursor::{position, MoveTo, MoveToColumn, RestorePosition, SavePosition},
    event::{read, Event, KeyCode, KeyEvent, KeyModifiers},
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    QueueableCommand, Result,
};
use std::collections::VecDeque;
use std::io::{Stdout, Write};

const HISTORY_SIZE: usize = 100;
static PROMPT_INDICATOR: &str = "〉";
const PROMPT_COLOR: Color = Color::Blue;

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
    SearchHistory,
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
    history_search: Option<BasicSearch>, // This could be have more features in the future (fzf, configurable?)
}

pub enum Signal {
    Success(String),
    CtrlC, // Interrupt current editing
    CtrlD, // End terminal session
    CtrlL, // FormFeed/Clear current screen
}

/// First jumps to new line then prints message with following newline.
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

/// Same behavior as std::println!
pub fn print_line(stdout: &mut Stdout, msg: &str) -> Result<()> {
    stdout
        .queue(Print(msg))?
        .queue(Print("\n"))?
        .queue(MoveToColumn(1))?;
    stdout.flush()?;

    Ok(())
}

/// Goes to the beginning of the next line
pub fn print_crlf(stdout: &mut Stdout) -> Result<()> {
    stdout.queue(Print("\n"))?.queue(MoveToColumn(1))?;
    stdout.flush()?;

    Ok(())
}

pub fn clear_screen(stdout: &mut Stdout) -> Result<()> {
    let (_, num_lines) = terminal::size()?;
    for _ in 0..2 * num_lines {
        stdout.queue(Print("\n"))?;
    }
    stdout.queue(MoveTo(0, 0))?;
    stdout.flush()?;
    Ok(())
}

fn queue_prompt(stdout: &mut Stdout) -> Result<()> {
    let mut prompt = Prompt::new(PROMPT_INDICATOR, 1);

    // print our prompt
    stdout
        .queue(MoveToColumn(0))?
        .queue(SetForegroundColor(PROMPT_COLOR))?
        .queue(Print(prompt.print_prompt()))?
        .queue(ResetColor)?;

    Ok(())
}

fn queue_prompt_indicator(stdout: &mut Stdout) -> Result<()> {
    // print our prompt
    stdout
        .queue(MoveToColumn(0))?
        .queue(SetForegroundColor(PROMPT_COLOR))?
        .queue(Print(PROMPT_INDICATOR))?
        .queue(ResetColor)?;

    Ok(())
}

fn buffer_paint(stdout: &mut Stdout, engine: &Engine, prompt_offset: (u16, u16)) -> Result<()> {
    let new_index = engine.insertion_point().offset;

    // Repaint logic:
    //
    // Start after the prompt
    // Draw the string slice from 0 to the grapheme start left of insertion point
    // Then, get the position on the screen
    // Then draw the remainer of the buffer from above
    // Finally, reset the cursor to the saved position

    // stdout.queue(Print(&engine.line_buffer[..new_index]))?;
    let insertion_line = engine.insertion_line();
    stdout.queue(MoveTo(prompt_offset.0, prompt_offset.1))?;
    stdout.queue(Print(&insertion_line[0..new_index]))?;
    stdout.queue(SavePosition)?;
    stdout.queue(Print(&insertion_line[new_index..]))?;
    stdout.queue(Clear(ClearType::FromCursorDown))?;
    stdout.queue(RestorePosition)?;

    stdout.flush()?;

    Ok(())
}

fn history_search_paint(stdout: &mut Stdout, engine: &Engine) -> Result<()> {
    // Assuming we are currently searching
    let search = engine
        .history_search
        .as_ref()
        .expect("couldn't get history_search reference");

    let status = if search.result.is_none() && !search.search_string.is_empty() {
        "failed "
    } else {
        ""
    };

    // print search prompt
    stdout
        .queue(MoveToColumn(0))?
        .queue(SetForegroundColor(Color::Blue))?
        .queue(Print(format!(
            "({}reverse-search)`{}':",
            status, search.search_string
        )))?
        .queue(ResetColor)?;

    match search.result {
        Some((history_index, offset)) => {
            let history_result = &engine.history[history_index];

            stdout.queue(Print(&history_result[..offset]))?;
            stdout.queue(SavePosition)?;
            stdout.queue(Print(&history_result[offset..]))?;
            stdout.queue(Clear(ClearType::UntilNewLine))?;
            stdout.queue(RestorePosition)?;
        }

        None => {
            stdout.queue(Clear(ClearType::UntilNewLine))?;
        }
    }

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
            history_search: None,
        }
    }

    pub fn run_edit_commands(&mut self, commands: &[EditCommand]) {
        // Handle command for history inputs
        if self.history_search.is_some() {
            for command in commands {
                match command {
                    EditCommand::InsertChar(c) => {
                        let search = self
                            .history_search
                            .as_mut()
                            .expect("couldn't get history_search as mutable"); // We checked it is some
                        search.step(BasicSearchCommand::InsertChar(*c), &self.history);
                    }
                    EditCommand::Backspace => {
                        let search = self
                            .history_search
                            .as_mut()
                            .expect("couldn't get history_search as mutable"); // We checked it is some
                        search.step(BasicSearchCommand::Backspace, &self.history);
                    }
                    EditCommand::SearchHistory => {
                        let search = self
                            .history_search
                            .as_mut()
                            .expect("couldn't get history_search as mutable"); // We checked it is some
                        search.step(BasicSearchCommand::Next, &self.history);
                    }
                    EditCommand::MoveRight => {
                        // Ignore move right, it is currently emited with InsertChar
                    }
                    // Leave history search otherwise
                    _ => self.history_search = None,
                }
            }
            return;
        }

        for command in commands {
            match command {
                EditCommand::MoveToStart => self.line_buffer.move_to_start(),
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
                    let insertion_point = self.line_buffer.insertion_point();
                    self.line_buffer.insert_char(insertion_point, *c);
                }
                EditCommand::Backspace => {
                    let left_index = self.line_buffer.grapheme_left_index();
                    let insertion_offset = self.insertion_point().offset;
                    if left_index < insertion_offset {
                        self.clear_range(left_index..insertion_offset);
                        self.set_insertion_point(left_index);
                    }
                }
                EditCommand::Delete => {
                    let right_index = self.line_buffer.grapheme_right_index();
                    let insertion_offset = self.insertion_point().offset;
                    if right_index > insertion_offset {
                        self.clear_range(insertion_offset..right_index);
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
                    // Don't append if the preceding value is identical
                    if self
                        .history
                        .front()
                        .map_or(true, |entry| entry.as_str() != self.insertion_line())
                    {
                        self.history.push_front(self.insertion_line().to_string());
                    }
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
                EditCommand::SearchHistory => {
                    self.history_search = Some(BasicSearch::new(self.insertion_line().to_string()));
                }
                EditCommand::CutFromStart => {
                    let insertion_offset = self.insertion_point().offset;
                    if insertion_offset > 0 {
                        self.cut_buffer.replace_range(
                            ..,
                            &self.line_buffer.insertion_line()[..insertion_offset],
                        );
                        self.clear_to_insertion_point();
                    }
                }
                EditCommand::CutToEnd => {
                    let cut_slice =
                        &self.line_buffer.insertion_line()[self.insertion_point().offset..];
                    if !cut_slice.is_empty() {
                        self.cut_buffer.replace_range(.., cut_slice);
                        self.clear_to_end();
                    }
                }
                EditCommand::CutWordLeft => {
                    let insertion_offset = self.insertion_point().offset;
                    let left_index = self.line_buffer.word_left_index();
                    if left_index < insertion_offset {
                        let cut_range = left_index..insertion_offset;
                        self.cut_buffer.replace_range(
                            ..,
                            &self.line_buffer.insertion_line()[cut_range.clone()],
                        );
                        self.clear_range(cut_range);
                        self.set_insertion_point(left_index);
                    }
                }
                EditCommand::CutWordRight => {
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > insertion_offset {
                        let cut_range = insertion_offset..right_index;
                        self.cut_buffer.replace_range(
                            ..,
                            &self.line_buffer.insertion_line()[cut_range.clone()],
                        );
                        self.clear_range(cut_range);
                    }
                }
                EditCommand::InsertCutBuffer => {
                    let insertion_offset = self.insertion_point().offset;
                    self.line_buffer
                        .insert_str(insertion_offset, &self.cut_buffer);
                    self.set_insertion_point(insertion_offset + self.cut_buffer.len());
                }
                EditCommand::UppercaseWord => {
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > insertion_offset {
                        let change_range = insertion_offset..right_index;
                        let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
                        self.line_buffer.replace_range(change_range, &uppercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::LowercaseWord => {
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.word_right_index();
                    if right_index > insertion_offset {
                        let change_range = insertion_offset..right_index;
                        let lowercased = self.insertion_line()[change_range.clone()].to_lowercase();
                        self.line_buffer.replace_range(change_range, &lowercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::CapitalizeChar => {
                    if self.line_buffer.on_whitespace() {
                        self.line_buffer.move_word_right();
                        self.line_buffer.move_word_left();
                    }
                    let insertion_offset = self.insertion_point().offset;
                    let right_index = self.line_buffer.grapheme_right_index();
                    if right_index > insertion_offset {
                        let change_range = insertion_offset..right_index;
                        let uppercased = self.insertion_line()[change_range.clone()].to_uppercase();
                        self.line_buffer.replace_range(change_range, &uppercased);
                        self.line_buffer.move_word_right();
                    }
                }
                EditCommand::SwapWords => {
                    let old_insertion_point = self.insertion_point().offset;
                    self.line_buffer.move_word_right();
                    let word_2_end = self.insertion_point().offset;
                    self.line_buffer.move_word_left();
                    let word_2_start = self.insertion_point().offset;
                    self.line_buffer.move_word_left();
                    let word_1_start = self.insertion_point().offset;
                    let word_1_end = self.line_buffer.word_right_index();

                    if word_1_start < word_1_end
                        && word_1_end < word_2_start
                        && word_2_start < word_2_end
                    {
                        let insertion_line = self.insertion_line();
                        let word_1 = insertion_line[word_1_start..word_1_end].to_string();
                        let word_2 = insertion_line[word_2_start..word_2_end].to_string();
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
                    let insertion_offset = self.insertion_point().offset;

                    if insertion_offset == 0 {
                        self.line_buffer.move_right()
                    } else if insertion_offset == self.line_buffer.insertion_line().len() {
                        self.line_buffer.move_left()
                    }
                    let grapheme_1_start = self.line_buffer.grapheme_left_index();
                    let grapheme_2_end = self.line_buffer.grapheme_right_index();

                    if grapheme_1_start < insertion_offset && grapheme_2_end > insertion_offset {
                        let grapheme_1 =
                            self.insertion_line()[grapheme_1_start..insertion_offset].to_string();
                        let grapheme_2 =
                            self.insertion_line()[insertion_offset..grapheme_2_end].to_string();
                        self.line_buffer
                            .replace_range(insertion_offset..grapheme_2_end, &grapheme_1);
                        self.line_buffer
                            .replace_range(grapheme_1_start..insertion_offset, &grapheme_2);
                        self.set_insertion_point(grapheme_2_end);
                    } else {
                        self.set_insertion_point(insertion_offset);
                    }
                }
            }
        }
    }

    pub fn insertion_point(&self) -> InsertionPoint {
        self.line_buffer.insertion_point()
    }

    pub fn set_insertion_point(&mut self, pos: usize) {
        let mut insertion_point = self.line_buffer.insertion_point();
        insertion_point.offset = pos;

        self.line_buffer.set_insertion_point(insertion_point)
    }

    pub fn insertion_line(&self) -> &str {
        self.line_buffer.insertion_line()
    }

    pub fn set_buffer(&mut self, buffer: String) {
        self.line_buffer.set_buffer(buffer)
    }

    pub fn move_to_end(&mut self) {
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

    pub fn print_history(&self, stdout: &mut Stdout) -> Result<()> {
        print_crlf(stdout)?;
        for (i, entry) in self.history.iter().rev().enumerate() {
            print_line(stdout, &format!("{}\t{}", i + 1, entry))?;
        }
        Ok(())
    }

    pub fn maybe_wrap(&self, terminal_width: u16, start_offset: u16, c: char) -> bool {
        use unicode_width::UnicodeWidthStr;

        let mut test_buffer = self.insertion_line().to_string();
        test_buffer.push(c);

        let display_width = UnicodeWidthStr::width(test_buffer.as_str()) + start_offset as usize;

        display_width >= terminal_width as usize
    }

    pub fn read_line(&mut self, stdout: &mut Stdout) -> Result<Signal> {
        queue_prompt(stdout)?;

        let mut terminal_size = terminal::size()?;

        // set where the input begins
        let mut prompt_offset = position()?;

        // our line count
        let mut line_count = 1;

        // Redraw if Ctrl-L was used
        if self.history_search.is_some() {
            history_search_paint(stdout, &self)?;
        } else {
            buffer_paint(stdout, &self, prompt_offset)?;
        }
        stdout.flush()?;

        loop {
            match read()? {
                Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::CONTROL,
                }) => match code {
                    KeyCode::Char('d') => {
                        if self.line_buffer.is_empty() {
                            return Ok(Signal::CtrlD);
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
                        self.run_edit_commands(&[EditCommand::Clear]);
                        return Ok(Signal::CtrlC);
                    }
                    KeyCode::Char('l') => {
                        return Ok(Signal::CtrlL);
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
                    KeyCode::Char('r') => {
                        self.run_edit_commands(&[EditCommand::SearchHistory]);
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
                            let line_start = if self.insertion_point().line == 0 {
                                prompt_offset.0
                            } else {
                                0
                            };
                            if self.maybe_wrap(terminal_size.0, line_start, c) {
                                let (original_column, original_row) = position()?;
                                self.run_edit_commands(&[
                                    EditCommand::InsertChar(c),
                                    EditCommand::MoveRight,
                                ]);
                                buffer_paint(stdout, &self, prompt_offset)?;

                                let (new_column, _) = position()?;

                                if new_column < original_column
                                    && original_row == (terminal_size.1 - 1)
                                    && line_count == 1
                                {
                                    // We have wrapped off bottom of screen, and prompt is on new row
                                    // We need to update the prompt location in this case
                                    prompt_offset.1 -= 1;
                                    line_count += 1;
                                }
                            } else {
                                self.run_edit_commands(&[
                                    EditCommand::InsertChar(c),
                                    EditCommand::MoveRight,
                                ]);
                            }
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
                        KeyCode::Enter => match &self.history_search {
                            Some(search) => {
                                queue_prompt_indicator(stdout)?;
                                if let Some((history_index, _)) = search.result {
                                    self.line_buffer
                                        .set_buffer(self.history[history_index].clone());
                                }
                                self.history_search = None;
                            }
                            None => {
                                let buffer = self.insertion_line().to_string();

                                self.run_edit_commands(&[
                                    EditCommand::AppendToHistory,
                                    EditCommand::Clear,
                                ]);

                                return Ok(Signal::Success(buffer));
                            }
                        },
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
                    terminal_size = (width, height);
                }
            }
            if self.history_search.is_some() {
                history_search_paint(stdout, &self)?;
            } else {
                buffer_paint(stdout, &self, prompt_offset)?;
            }
        }
    }
}
