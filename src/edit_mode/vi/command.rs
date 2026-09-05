use super::{motion::Motion, parser::ReedlineOption, ViMode};
use crate::enums::{
    TextObject, TextObjectBracket, TextObjectQuote, TextObjectScope, TextObjectType,
};
use crate::{Direction, EditCommand, Granularity, MotionTarget, ReedlineEvent, Vi};
use std::iter::Peekable;

pub fn parse_command<'iter, I>(mode: ViMode, input: &mut Peekable<I>) -> Option<Command>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('d') => {
            let _ = input.next();
            text_object_to_command(input, Command::Delete, |text_object| {
                Command::DeleteTextObject { text_object }
            })
        }
        // Checking for "yi(" or "yiw" etc.
        Some('y') => {
            let _ = input.next();
            text_object_to_command(input, Command::Yank, |text_object| {
                Command::YankTextObject { text_object }
            })
        }
        Some('p') => {
            let _ = input.next();
            Some(Command::PasteAfter)
        }
        Some('P') => {
            let _ = input.next();
            Some(Command::PasteBefore)
        }
        Some('i') => {
            let _ = input.next();
            Some(Command::EnterViInsert)
        }
        Some('a') => {
            let _ = input.next();
            Some(Command::EnterViAppend)
        }
        Some('u') if mode == ViMode::Normal => {
            let _ = input.next();
            Some(Command::Undo)
        }
        // Checking for "ci(" or "ciw" etc.
        Some('c') => {
            let _ = input.next();
            text_object_to_command(input, Command::Change, |text_object| {
                Command::ChangeTextObject { text_object }
            })
        }
        Some('x') => {
            let _ = input.next();
            Some(Command::DeleteChar)
        }
        Some('X') => {
            let _ = input.next();
            Some(Command::DeleteCharBackward)
        }
        Some('r') => {
            let _ = input.next();
            input
                .next()
                .map(|c| Command::ReplaceChar(*c))
                .or(Some(Command::Incomplete))
        }
        Some('s') => {
            let _ = input.next();
            Some(Command::SubstituteCharWithInsert)
        }
        Some('?') => {
            let _ = input.next();
            Some(Command::HistorySearch)
        }
        Some('C') => {
            let _ = input.next();
            Some(Command::ChangeToLineEnd)
        }
        Some('D') => {
            let _ = input.next();
            Some(Command::DeleteToEnd)
        }
        Some('I') => {
            let _ = input.next();
            Some(Command::PrependToStart)
        }
        Some('A') => {
            let _ = input.next();
            Some(Command::AppendToEnd)
        }
        Some('S') => {
            let _ = input.next();
            Some(Command::RewriteCurrentLine)
        }
        Some('~') => {
            let _ = input.next();
            Some(Command::Switchcase)
        }
        Some('.') => {
            let _ = input.next();
            Some(Command::RepeatLastAction)
        }
        Some(&&o @ ('o' | 'O')) => match mode {
            ViMode::Normal => {
                let _ = input.next();
                if o.is_ascii_lowercase() {
                    Some(Command::NewlineBelow)
                } else {
                    Some(Command::NewlineAbove)
                }
            }
            ViMode::Visual => {
                let _ = input.next();
                Some(Command::SwapCursorAndAnchor)
            }
            // This arm should be unreachable
            ViMode::Insert => None,
        },
        Some(&&u @ ('u' | 'U')) if mode == ViMode::Visual => {
            let _ = input.next();
            if u.is_ascii_lowercase() {
                Some(Command::Lowercase)
            } else {
                Some(Command::Uppercase)
            }
        }
        _ => None,
    }
}

pub fn text_object_to_command<'iter, I, F>(
    input: &mut Peekable<I>,
    incomplete_command: Command,
    command_generator: F,
) -> Option<Command>
where
    I: Iterator<Item = &'iter char>,
    F: FnOnce(TextObject) -> Command,
{
    let scope = match input.peek() {
        Some('i') => TextObjectScope::Inner,
        Some('a') => TextObjectScope::Around,
        _ => return Some(incomplete_command),
    };
    let _ = input.next();
    input
        .next()
        .and_then(|c| char_to_text_object(*c, scope))
        .map(command_generator)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Incomplete,
    Delete,
    DeleteChar,
    DeleteCharBackward,
    ReplaceChar(char),
    SubstituteCharWithInsert,
    NewlineAbove,
    NewlineBelow,
    PasteAfter,
    PasteBefore,
    EnterViAppend,
    EnterViInsert,
    Undo,
    ChangeToLineEnd,
    DeleteToEnd,
    AppendToEnd,
    PrependToStart,
    RewriteCurrentLine,
    Change,
    HistorySearch,
    Lowercase,
    Uppercase,
    Switchcase,
    RepeatLastAction,
    Yank,
    ChangeTextObject { text_object: TextObject },
    YankTextObject { text_object: TextObject },
    DeleteTextObject { text_object: TextObject },
    SwapCursorAndAnchor,
}

impl Command {
    pub fn whole_line_char(&self) -> Option<char> {
        match self {
            Command::Delete => Some('d'),
            Command::Change => Some('c'),
            Command::Yank => Some('y'),
            _ => None,
        }
    }

    pub fn requires_motion(&self) -> bool {
        matches!(self, Command::Delete | Command::Change | Command::Yank)
    }

    pub fn to_reedline(&self, vi_state: &mut Vi) -> Vec<ReedlineOption> {
        match self {
            Self::EnterViInsert => vec![ReedlineOption::Event(ReedlineEvent::Repaint)],
            Self::EnterViAppend => vec![ReedlineOption::Edit(EditCommand::MoveRight {
                select: false,
            })],
            Self::NewlineAbove => vec![ReedlineOption::Edit(EditCommand::InsertNewlineAbove)],
            Self::NewlineBelow => vec![ReedlineOption::Edit(EditCommand::InsertNewlineBelow)],
            Self::PasteAfter => vec![ReedlineOption::Edit(EditCommand::PasteCutBufferAfter)],
            Self::PasteBefore => vec![ReedlineOption::Edit(EditCommand::PasteCutBufferBefore)],
            Self::Undo => vec![ReedlineOption::Edit(EditCommand::Undo)],
            Self::ChangeToLineEnd => vec![ReedlineOption::Edit(EditCommand::ClearToLineEnd)],
            Self::DeleteToEnd => vec![ReedlineOption::Edit(EditCommand::CutToLineEnd)],
            Self::AppendToEnd => vec![ReedlineOption::Edit(EditCommand::MoveToLineEnd {
                select: false,
            })],
            Self::PrependToStart => vec![ReedlineOption::Edit(EditCommand::MoveToLineStart {
                select: false,
            })],
            Self::DeleteCharBackward => {
                if vi_state.mode == ViMode::Visual {
                    vec![ReedlineOption::Edit(EditCommand::CutSelection {
                        granularity: Granularity::LineWise,
                    })]
                } else {
                    vec![ReedlineOption::Edit(EditCommand::CutCharLeft)]
                }
            }
            // `S` ≡ `cc` (vim): change the whole line, keeping the blank line
            // for insert mode and filling the register linewise.
            Self::RewriteCurrentLine => vec![ReedlineOption::Edit(EditCommand::Change {
                target: MotionTarget::LineEdge(Direction::Forward),
                granularity: Granularity::LineWise,
            })],
            Self::DeleteChar => {
                if vi_state.mode == ViMode::Visual {
                    vec![ReedlineOption::Edit(EditCommand::CutSelection {
                        granularity: Granularity::CharWise,
                    })]
                } else {
                    vec![ReedlineOption::Edit(EditCommand::CutChar)]
                }
            }
            Self::ReplaceChar(c) => {
                vec![ReedlineOption::Edit(EditCommand::ReplaceChar(*c))]
            }
            Self::SubstituteCharWithInsert => {
                if vi_state.mode == ViMode::Visual {
                    vec![ReedlineOption::Edit(EditCommand::CutSelection {
                        granularity: Granularity::CharWise,
                    })]
                } else {
                    vec![ReedlineOption::Edit(EditCommand::CutChar)]
                }
            }
            Self::HistorySearch => vec![ReedlineOption::Event(ReedlineEvent::SearchHistory)],
            Self::Lowercase => {
                vec![ReedlineOption::Edit(EditCommand::LowercaseSelection)]
            }
            Self::Uppercase => {
                vec![ReedlineOption::Edit(EditCommand::UppercaseSelection)]
            }
            Self::Switchcase => {
                if vi_state.mode == ViMode::Visual {
                    vec![ReedlineOption::Edit(EditCommand::SwitchcaseSelection)]
                } else {
                    vec![ReedlineOption::Edit(EditCommand::SwitchcaseChar)]
                }
            }
            // Whenever a motion is required to finish the command we must be in visual mode
            Self::Delete | Self::Change => vec![ReedlineOption::Edit(EditCommand::CutSelection {
                granularity: Granularity::CharWise,
            })],
            Self::Yank => vec![ReedlineOption::Edit(EditCommand::CopySelection)],
            Self::Incomplete => vec![ReedlineOption::Incomplete],
            Self::RepeatLastAction => match &vi_state.previous {
                Some(event) => vec![ReedlineOption::Event(event.clone())],
                None => vec![],
            },
            Self::ChangeTextObject { text_object } => {
                vec![ReedlineOption::Edit(EditCommand::CutTextObject {
                    text_object: *text_object,
                })]
            }
            Self::YankTextObject { text_object } => {
                vec![ReedlineOption::Edit(EditCommand::CopyTextObject {
                    text_object: *text_object,
                })]
            }
            Self::DeleteTextObject { text_object } => {
                vec![ReedlineOption::Edit(EditCommand::CutTextObject {
                    text_object: *text_object,
                })]
            }
            Self::SwapCursorAndAnchor => {
                vec![ReedlineOption::Edit(EditCommand::SwapCursorAndAnchor)]
            }
        }
    }

    pub fn to_reedline_with_motion(
        &self,
        motion: &Motion,
        vi_state: &mut Vi,
    ) -> Option<Vec<ReedlineOption>> {
        match self {
            Self::Delete => match motion {
                // `dd` — the whole current line, linewise.
                Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::Cut {
                    target: MotionTarget::LineEdge(Direction::Forward),
                    granularity: Granularity::LineWise,
                })]),
                // Word and line-edge motions lower through one parameterized verb:
                // cut to the motion's target (`operator_span` makes `e`/`E` inclusive).
                Motion::NextWord
                | Motion::NextBigWord
                | Motion::NextWordEnd
                | Motion::NextBigWordEnd
                | Motion::PreviousWord
                | Motion::PreviousBigWord
                | Motion::Start
                | Motion::End => motion.target().map(|target| {
                    vec![ReedlineOption::Edit(EditCommand::Cut {
                        target,
                        granularity: Granularity::CharWise,
                    })]
                }),
                Motion::RightUntil(_)
                | Motion::RightBefore(_)
                | Motion::LeftUntil(_)
                | Motion::LeftBefore(_) => motion.target().map(|target| {
                    vi_state.last_char_search = Some(target);
                    vec![ReedlineOption::Edit(EditCommand::Cut {
                        target,
                        granularity: Granularity::CharWise,
                    })]
                }),
                Motion::NonBlankStart => Some(vec![ReedlineOption::Edit(
                    EditCommand::CutFromLineNonBlankStart,
                )]),
                Motion::Left => Some(vec![ReedlineOption::Edit(EditCommand::Backspace)]),
                Motion::Right => Some(vec![ReedlineOption::Edit(EditCommand::Delete)]),
                // `dj`/`dk`/`dgg`/`dG` — whole lines to the adjacent line or the
                // buffer edge, linewise. The targets + the LineWise snap (incl.
                // the buffer-end `\n` fixup) reproduce the dedicated commands.
                Motion::Down | Motion::Up | Motion::FirstLine | Motion::LastLine => {
                    motion.target().map(|target| {
                        vec![ReedlineOption::Edit(EditCommand::Cut {
                            target,
                            granularity: Granularity::LineWise,
                        })]
                    })
                }
                Motion::ReplayCharSearch => vi_state.last_char_search.map(|target| {
                    vec![ReedlineOption::Edit(EditCommand::Cut {
                        target,
                        granularity: Granularity::CharWise,
                    })]
                }),
                Motion::ReverseCharSearch => vi_state.last_char_search.map(|target| {
                    vec![ReedlineOption::Edit(EditCommand::Cut {
                        target: target.reversed(),
                        granularity: Granularity::CharWise,
                    })]
                }),
            },
            Self::Change => {
                let op = match motion {
                    // `cc` — change the whole line: its content is cut (the
                    // blank line remains for insert mode) and the register is
                    // filled linewise, so `p` after `cc` pastes as a line.
                    Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::Change {
                        target: MotionTarget::LineEdge(Direction::Forward),
                        granularity: Granularity::LineWise,
                    })]),
                    // `cw`/`cW` act like `ce`/`cE`: change to the word *end*, not the
                    // next word's start. Other word and line-edge motions (`c$`/`c0`)
                    // use their own target.
                    Motion::NextWord
                    | Motion::NextBigWord
                    | Motion::NextWordEnd
                    | Motion::NextBigWordEnd
                    | Motion::PreviousWord
                    | Motion::PreviousBigWord
                    | Motion::Start
                    | Motion::End => {
                        let target = match motion {
                            Motion::NextWord => Motion::NextWordEnd.target(),
                            Motion::NextBigWord => Motion::NextBigWordEnd.target(),
                            other => other.target(),
                        };
                        target.map(|target| {
                            vec![ReedlineOption::Edit(EditCommand::Cut {
                                target,
                                granularity: Granularity::CharWise,
                            })]
                        })
                    }
                    Motion::RightUntil(_)
                    | Motion::RightBefore(_)
                    | Motion::LeftUntil(_)
                    | Motion::LeftBefore(_) => motion.target().map(|target| {
                        vi_state.last_char_search = Some(target);
                        vec![ReedlineOption::Edit(EditCommand::Cut {
                            target,
                            granularity: Granularity::CharWise,
                        })]
                    }),
                    Motion::NonBlankStart => Some(vec![ReedlineOption::Edit(
                        EditCommand::CutFromLineNonBlankStart,
                    )]),
                    Motion::Left => Some(vec![ReedlineOption::Edit(EditCommand::Backspace)]),
                    Motion::Right => Some(vec![ReedlineOption::Edit(EditCommand::Delete)]),
                    // `cj`/`ck`/`cgg`/`cG` — linewise change: the spanned lines'
                    // content is cut, one blank line remains, and insert mode
                    // re-enters on it (`Change`'s LineWise snap keeps the
                    // terminators where `Cut`'s consumes them).
                    Motion::Down | Motion::Up | Motion::FirstLine | Motion::LastLine => {
                        motion.target().map(|target| {
                            vec![ReedlineOption::Edit(EditCommand::Change {
                                target,
                                granularity: Granularity::LineWise,
                            })]
                        })
                    }
                    Motion::ReplayCharSearch => vi_state.last_char_search.map(|target| {
                        vec![ReedlineOption::Edit(EditCommand::Cut {
                            target,
                            granularity: Granularity::CharWise,
                        })]
                    }),
                    Motion::ReverseCharSearch => vi_state.last_char_search.map(|target| {
                        vec![ReedlineOption::Edit(EditCommand::Cut {
                            target: target.reversed(),
                            granularity: Granularity::CharWise,
                        })]
                    }),
                };
                // Semihack: Append `Repaint` to ensure the mode change gets displayed
                op.map(|mut vec| {
                    vec.push(ReedlineOption::Event(ReedlineEvent::Repaint));
                    vec
                })
            }
            Self::Yank => match motion {
                // `yy` — the whole current line, linewise.
                Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::Copy {
                    target: MotionTarget::LineEdge(Direction::Forward),
                    granularity: Granularity::LineWise,
                })]),
                Motion::NextWord
                | Motion::NextBigWord
                | Motion::NextWordEnd
                | Motion::NextBigWordEnd
                | Motion::PreviousWord
                | Motion::PreviousBigWord
                | Motion::Start
                | Motion::End => motion.target().map(|target| {
                    vec![ReedlineOption::Edit(EditCommand::Copy {
                        target,
                        granularity: Granularity::CharWise,
                    })]
                }),
                Motion::RightUntil(_)
                | Motion::RightBefore(_)
                | Motion::LeftUntil(_)
                | Motion::LeftBefore(_) => motion.target().map(|target| {
                    vi_state.last_char_search = Some(target);
                    vec![ReedlineOption::Edit(EditCommand::Copy {
                        target,
                        granularity: Granularity::CharWise,
                    })]
                }),
                Motion::NonBlankStart => Some(vec![ReedlineOption::Edit(
                    EditCommand::CopyFromLineNonBlankStart,
                )]),
                Motion::Left => Some(vec![ReedlineOption::Edit(EditCommand::CopyLeft)]),
                Motion::Right => Some(vec![ReedlineOption::Edit(EditCommand::CopyRight)]),
                // `yj`/`yk`/`ygg`/`yG` — whole lines to the adjacent line or
                // the buffer edge, linewise.
                Motion::Down | Motion::Up | Motion::FirstLine | Motion::LastLine => {
                    motion.target().map(|target| {
                        vec![ReedlineOption::Edit(EditCommand::Copy {
                            target,
                            granularity: Granularity::LineWise,
                        })]
                    })
                }
                Motion::ReplayCharSearch => vi_state.last_char_search.map(|target| {
                    vec![ReedlineOption::Edit(EditCommand::Copy {
                        target,
                        granularity: Granularity::CharWise,
                    })]
                }),
                Motion::ReverseCharSearch => vi_state.last_char_search.map(|target| {
                    vec![ReedlineOption::Edit(EditCommand::Copy {
                        target: target.reversed(),
                        granularity: Granularity::CharWise,
                    })]
                }),
            },
            _ => None,
        }
    }
}

fn char_to_text_object(c: char, scope: TextObjectScope) -> Option<TextObject> {
    match c {
        'b' => Some(TextObject {
            scope,
            object_type: TextObjectType::Brackets(TextObjectBracket::All),
        }),
        'q' => Some(TextObject {
            scope,
            object_type: TextObjectType::Quotes(TextObjectQuote::All),
        }),
        _ => TextObjectType::from_char(c).map(|tot| TextObject {
            scope,
            object_type: tot,
        }),
    }
}
