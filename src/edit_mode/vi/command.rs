use super::{motion::Motion, motion::ViCharSearch, parser::ReedlineOption, ViMode};
use crate::{EditCommand, ReedlineEvent, Vi};
use std::iter::Peekable;

pub fn parse_command<'iter, I>(input: &mut Peekable<I>) -> Option<Command>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('d') => {
            let _ = input.next();
            // Checking for "di(" or "di)" etc.
            if let Some('i') = input.peek() {
                let _ = input.next();
                input
                    .next()
                    .and_then(|c| bracket_pair_for(*c))
                    .map(|(left, right)| Command::DeleteInsidePair { left, right })
            } else {
                Some(Command::Delete)
            }
        }
        // Checking for "yi(" or "yi)" etc.
        Some('y') => {
            let _ = input.next();
            if let Some('i') = input.peek() {
                let _ = input.next();
                input
                    .next()
                    .and_then(|c| bracket_pair_for(*c))
                    .map(|(left, right)| Command::YankInsidePair { left, right })
            } else {
                Some(Command::Yank)
            }
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
        Some('u') => {
            let _ = input.next();
            Some(Command::Undo)
        }
        // Checking for "ci(" or "ci)" etc.
        Some('c') => {
            let _ = input.next();
            if let Some('i') = input.peek() {
                let _ = input.next();
                input
                    .next()
                    .and_then(|c| bracket_pair_for(*c))
                    .map(|(left, right)| Command::ChangeInsidePair { left, right })
            } else {
                Some(Command::Change)
            }
        }
        Some('x') => {
            let _ = input.next();
            Some(Command::DeleteChar)
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
        Some('o') => {
            let _ = input.next();
            Some(Command::SwapCursorAndAnchor)
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Incomplete,
    Delete,
    DeleteChar,
    ReplaceChar(char),
    SubstituteCharWithInsert,
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
    Switchcase,
    RepeatLastAction,
    Yank,
    // These DoSthInsidePair commands are agnostic to whether user pressed the left char or right char
    ChangeInsidePair { left: char, right: char },
    DeleteInsidePair { left: char, right: char },
    YankInsidePair { left: char, right: char },
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
            Self::RewriteCurrentLine => vec![ReedlineOption::Edit(EditCommand::CutCurrentLine)],
            Self::DeleteChar => {
                if vi_state.mode == ViMode::Visual {
                    vec![ReedlineOption::Edit(EditCommand::CutSelection)]
                } else {
                    vec![ReedlineOption::Edit(EditCommand::CutChar)]
                }
            }
            Self::ReplaceChar(c) => {
                vec![ReedlineOption::Edit(EditCommand::ReplaceChar(*c))]
            }
            Self::SubstituteCharWithInsert => {
                if vi_state.mode == ViMode::Visual {
                    vec![ReedlineOption::Edit(EditCommand::CutSelection)]
                } else {
                    vec![ReedlineOption::Edit(EditCommand::CutChar)]
                }
            }
            Self::HistorySearch => vec![ReedlineOption::Event(ReedlineEvent::SearchHistory)],
            Self::Switchcase => vec![ReedlineOption::Edit(EditCommand::SwitchcaseChar)],
            // Whenever a motion is required to finish the command we must be in visual mode
            Self::Delete | Self::Change => vec![ReedlineOption::Edit(EditCommand::CutSelection)],
            Self::Yank => vec![ReedlineOption::Edit(EditCommand::CopySelection)],
            Self::Incomplete => vec![ReedlineOption::Incomplete],
            Self::RepeatLastAction => match &vi_state.previous {
                Some(event) => vec![ReedlineOption::Event(event.clone())],
                None => vec![],
            },
            Self::ChangeInsidePair { left, right } => {
                vec![ReedlineOption::Edit(EditCommand::CutInside {
                    left: *left,
                    right: *right,
                })]
            }
            Self::DeleteInsidePair { left, right } => {
                vec![ReedlineOption::Edit(EditCommand::CutInside {
                    left: *left,
                    right: *right,
                })]
            }
            Self::YankInsidePair { left, right } => {
                vec![ReedlineOption::Edit(EditCommand::YankInside {
                    left: *left,
                    right: *right,
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
                Motion::End => Some(vec![ReedlineOption::Edit(EditCommand::CutToLineEnd)]),
                Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::CutCurrentLine)]),
                Motion::NextWord => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutWordRightToNext)])
                }
                Motion::NextBigWord => Some(vec![ReedlineOption::Edit(
                    EditCommand::CutBigWordRightToNext,
                )]),
                Motion::NextWordEnd => Some(vec![ReedlineOption::Edit(EditCommand::CutWordRight)]),
                Motion::NextBigWordEnd => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordRight)])
                }
                Motion::PreviousWord => Some(vec![ReedlineOption::Edit(EditCommand::CutWordLeft)]),
                Motion::PreviousBigWord => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordLeft)])
                }
                Motion::RightUntil(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::ToRight(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CutRightUntil(*c))])
                }
                Motion::RightBefore(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::TillRight(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CutRightBefore(*c))])
                }
                Motion::LeftUntil(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::ToLeft(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CutLeftUntil(*c))])
                }
                Motion::LeftBefore(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::TillLeft(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CutLeftBefore(*c))])
                }
                Motion::Start => Some(vec![ReedlineOption::Edit(EditCommand::CutFromLineStart)]),
                Motion::Left => Some(vec![ReedlineOption::Edit(EditCommand::Backspace)]),
                Motion::Right => Some(vec![ReedlineOption::Edit(EditCommand::Delete)]),
                Motion::Up => None,
                Motion::Down => None,
                Motion::ReplayCharSearch => vi_state
                    .last_char_search
                    .as_ref()
                    .map(|char_search| vec![ReedlineOption::Edit(char_search.to_cut())]),
                Motion::ReverseCharSearch => vi_state
                    .last_char_search
                    .as_ref()
                    .map(|char_search| vec![ReedlineOption::Edit(char_search.reverse().to_cut())]),
            },
            Self::Change => {
                let op = match motion {
                    Motion::End => Some(vec![ReedlineOption::Edit(EditCommand::CutToLineEnd)]),
                    Motion::Line => Some(vec![
                        ReedlineOption::Edit(EditCommand::MoveToLineStart { select: false }),
                        ReedlineOption::Edit(EditCommand::CutToLineEnd),
                    ]),
                    Motion::NextWord => Some(vec![ReedlineOption::Edit(EditCommand::CutWordRight)]),
                    Motion::NextBigWord => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordRight)])
                    }
                    Motion::NextWordEnd => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutWordRight)])
                    }
                    Motion::NextBigWordEnd => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordRight)])
                    }
                    Motion::PreviousWord => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutWordLeft)])
                    }
                    Motion::PreviousBigWord => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutBigWordLeft)])
                    }
                    Motion::RightUntil(c) => {
                        vi_state.last_char_search = Some(ViCharSearch::ToRight(*c));
                        Some(vec![ReedlineOption::Edit(EditCommand::CutRightUntil(*c))])
                    }
                    Motion::RightBefore(c) => {
                        vi_state.last_char_search = Some(ViCharSearch::TillRight(*c));
                        Some(vec![ReedlineOption::Edit(EditCommand::CutRightBefore(*c))])
                    }
                    Motion::LeftUntil(c) => {
                        vi_state.last_char_search = Some(ViCharSearch::ToLeft(*c));
                        Some(vec![ReedlineOption::Edit(EditCommand::CutLeftUntil(*c))])
                    }
                    Motion::LeftBefore(c) => {
                        vi_state.last_char_search = Some(ViCharSearch::TillLeft(*c));
                        Some(vec![ReedlineOption::Edit(EditCommand::CutLeftBefore(*c))])
                    }
                    Motion::Start => {
                        Some(vec![ReedlineOption::Edit(EditCommand::CutFromLineStart)])
                    }
                    Motion::Left => Some(vec![ReedlineOption::Edit(EditCommand::Backspace)]),
                    Motion::Right => Some(vec![ReedlineOption::Edit(EditCommand::Delete)]),
                    Motion::Up => None,
                    Motion::Down => None,
                    Motion::ReplayCharSearch => vi_state
                        .last_char_search
                        .as_ref()
                        .map(|char_search| vec![ReedlineOption::Edit(char_search.to_cut())]),
                    Motion::ReverseCharSearch => {
                        vi_state.last_char_search.as_ref().map(|char_search| {
                            vec![ReedlineOption::Edit(char_search.reverse().to_cut())]
                        })
                    }
                };
                // Semihack: Append `Repaint` to ensure the mode change gets displayed
                op.map(|mut vec| {
                    vec.push(ReedlineOption::Event(ReedlineEvent::Repaint));
                    vec
                })
            }
            Self::Yank => match motion {
                Motion::End => Some(vec![ReedlineOption::Edit(EditCommand::CopyToLineEnd)]),
                Motion::Line => Some(vec![ReedlineOption::Edit(EditCommand::CopyCurrentLine)]),
                Motion::NextWord => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyWordRightToNext)])
                }
                Motion::NextBigWord => Some(vec![ReedlineOption::Edit(
                    EditCommand::CopyBigWordRightToNext,
                )]),
                Motion::NextWordEnd => Some(vec![ReedlineOption::Edit(EditCommand::CopyWordRight)]),
                Motion::NextBigWordEnd => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyBigWordRight)])
                }
                Motion::PreviousWord => Some(vec![ReedlineOption::Edit(EditCommand::CopyWordLeft)]),
                Motion::PreviousBigWord => {
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyBigWordLeft)])
                }
                Motion::RightUntil(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::ToRight(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyRightUntil(*c))])
                }
                Motion::RightBefore(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::TillRight(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyRightBefore(*c))])
                }
                Motion::LeftUntil(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::ToLeft(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyLeftUntil(*c))])
                }
                Motion::LeftBefore(c) => {
                    vi_state.last_char_search = Some(ViCharSearch::TillLeft(*c));
                    Some(vec![ReedlineOption::Edit(EditCommand::CopyLeftBefore(*c))])
                }
                Motion::Start => Some(vec![ReedlineOption::Edit(EditCommand::CopyFromLineStart)]),
                Motion::Left => Some(vec![ReedlineOption::Edit(EditCommand::CopyLeft)]),
                Motion::Right => Some(vec![ReedlineOption::Edit(EditCommand::CopyRight)]),
                Motion::Up => None,
                Motion::Down => None,
                Motion::ReplayCharSearch => vi_state
                    .last_char_search
                    .as_ref()
                    .map(|char_search| vec![ReedlineOption::Edit(char_search.to_copy())]),
                Motion::ReverseCharSearch => vi_state
                    .last_char_search
                    .as_ref()
                    .map(|char_search| vec![ReedlineOption::Edit(char_search.reverse().to_copy())]),
            },
            _ => None,
        }
    }
}

fn bracket_pair_for(c: char) -> Option<(char, char)> {
    match c {
        '(' | ')' => Some(('(', ')')),
        '[' | ']' => Some(('[', ']')),
        '{' | '}' => Some(('{', '}')),
        '<' | '>' => Some(('<', '>')),
        '"' => Some(('"', '"')),
        '$' => Some(('$', '$')),
        '\'' => Some(('\'', '\'')),
        '`' => Some(('`', '`')),
        _ => None,
    }
}
