use std::iter::Peekable;

use crate::{
    edit_mode::vi::ViMode, Direction, EditCommand, MotionTarget, ReedlineEvent, Vi, WordEdge,
    WordKind,
};

use super::parser::{ParseResult, ReedlineOption};

pub fn parse_motion<'iter, I>(
    input: &mut Peekable<I>,
    command_char: Option<char>,
) -> ParseResult<Motion>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('h') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Left)
        }
        Some('l') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Right)
        }
        Some('j') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Down)
        }
        Some('k') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Up)
        }
        Some('b') => {
            let _ = input.next();
            ParseResult::Valid(Motion::PreviousWord)
        }
        Some('B') => {
            let _ = input.next();
            ParseResult::Valid(Motion::PreviousBigWord)
        }
        Some('w') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextWord)
        }
        Some('W') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextBigWord)
        }
        Some('e') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextWordEnd)
        }
        Some('E') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NextBigWordEnd)
        }
        Some('0') => {
            let _ = input.next();
            ParseResult::Valid(Motion::Start)
        }
        Some('^') => {
            let _ = input.next();
            ParseResult::Valid(Motion::NonBlankStart)
        }
        Some('$') => {
            let _ = input.next();
            ParseResult::Valid(Motion::End)
        }
        Some('f') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::RightUntil(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some('t') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::RightBefore(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some('F') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::LeftUntil(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some('T') => {
            let _ = input.next();
            match input.peek() {
                Some(&x) => {
                    input.next();
                    ParseResult::Valid(Motion::LeftBefore(*x))
                }
                None => ParseResult::Incomplete,
            }
        }
        Some(';') => {
            let _ = input.next();
            ParseResult::Valid(Motion::ReplayCharSearch)
        }
        Some(',') => {
            let _ = input.next();
            ParseResult::Valid(Motion::ReverseCharSearch)
        }
        Some('g') => {
            let _ = input.next();
            match input.peek() {
                Some('g') => {
                    input.next();
                    ParseResult::Valid(Motion::FirstLine)
                }
                Some(_) => ParseResult::Invalid,
                None => ParseResult::Incomplete,
            }
        }
        Some('G') => {
            let _ = input.next();
            ParseResult::Valid(Motion::LastLine)
        }
        ch if ch == command_char.as_ref().as_ref() && command_char.is_some() => {
            let _ = input.next();
            ParseResult::Valid(Motion::Line)
        }
        None => ParseResult::Incomplete,
        _ => ParseResult::Invalid,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    NextWord,
    NextBigWord,
    NextWordEnd,
    NextBigWordEnd,
    PreviousWord,
    PreviousBigWord,
    Line,
    Start,
    NonBlankStart,
    End,
    FirstLine,
    LastLine,
    RightUntil(char),
    RightBefore(char),
    LeftUntil(char),
    LeftBefore(char),
    ReplayCharSearch,
    ReverseCharSearch,
}

impl Motion {
    /// The [`MotionTarget`] this motion resolves to, for motions with a static
    /// target.
    ///
    /// `None` for motions that have no fixed mapping: `h`/`l`, `^`, the
    /// doubled-operator `Line` (`dd`/`cc`/`yy`), and the `;`/`,` replays (whose
    /// target is the dynamic `last_char_search`). Those keep their own lowering.
    /// Every motion with a static target reads this one map on both the
    /// bare-motion path (`Move`/`Extend`) and the operator path (`Cut`/`Copy`),
    /// so its semantics live in a single place.
    ///
    /// `j`/`k` map to [`MotionTarget::Line`] for the *operator* path
    /// (`dj`/`cj`/`yj` snap linewise); their bare-motion arms in
    /// [`Motion::to_reedline`] never consult this map, because a bare `j`/`k`
    /// is not a buffer motion at all — it lowers to menu/history events.
    pub(super) fn target(&self) -> Option<MotionTarget> {
        // A word target, spelled compactly.
        let word = |kind: WordKind, edge: WordEdge, direction: Direction| MotionTarget::Word {
            kind,
            edge,
            direction,
        };

        match self {
            // `w` / `W` — start of the next word, forward.
            Motion::NextWord => Some(word(WordKind::Small, WordEdge::Start, Direction::Forward)),
            Motion::NextBigWord => Some(word(WordKind::Big, WordEdge::Start, Direction::Forward)),
            // `e` / `E` — end of the next word, forward.
            Motion::NextWordEnd => Some(word(WordKind::Small, WordEdge::End, Direction::Forward)),
            Motion::NextBigWordEnd => Some(word(WordKind::Big, WordEdge::End, Direction::Forward)),
            // `b` / `B` — start of the previous word, backward.
            Motion::PreviousWord => {
                Some(word(WordKind::Small, WordEdge::Start, Direction::Backward))
            }
            Motion::PreviousBigWord => {
                Some(word(WordKind::Big, WordEdge::Start, Direction::Backward))
            }

            // `0` / `$` — start / end of the current line.
            Motion::Start => Some(MotionTarget::LineEdge(Direction::Backward)),
            Motion::End => Some(MotionTarget::LineEdge(Direction::Forward)),

            Motion::RightUntil(c) => Some(MotionTarget::Find {
                ch: *c,
                direction: Direction::Forward,
                stop: crate::FindStop::On,
            }),
            Motion::RightBefore(c) => Some(MotionTarget::Find {
                ch: *c,
                direction: Direction::Forward,
                stop: crate::FindStop::Before,
            }),
            Motion::LeftUntil(c) => Some(MotionTarget::Find {
                ch: *c,
                direction: Direction::Backward,
                stop: crate::FindStop::On,
            }),
            Motion::LeftBefore(c) => Some(MotionTarget::Find {
                ch: *c,
                direction: Direction::Backward,
                stop: crate::FindStop::Before,
            }),
            Motion::FirstLine => Some(MotionTarget::BufferEdge(Direction::Backward)),
            Motion::LastLine => Some(MotionTarget::BufferEdge(Direction::Forward)),

            // `j`/`k` — the adjacent line, for the operator path only (see the
            // method docs; the bare-motion arms lower to events instead).
            Motion::Down => Some(MotionTarget::Line(Direction::Forward)),
            Motion::Up => Some(MotionTarget::Line(Direction::Backward)),

            // Not yet lowered — keep the existing per-variant EditCommand path.
            _ => None,
        }
    }

    pub fn to_reedline(&self, vi_state: &mut Vi) -> Vec<ReedlineOption> {
        let select_mode = vi_state.mode == ViMode::Visual;
        match self {
            Motion::Left => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::MenuLeft,
                ReedlineEvent::Edit(vec![EditCommand::MoveLeft {
                    select: select_mode,
                }]),
            ]))],
            Motion::Right => vec![ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                ReedlineEvent::HistoryHintComplete,
                ReedlineEvent::MenuRight,
                ReedlineEvent::Edit(vec![EditCommand::MoveRight {
                    select: select_mode,
                }]),
            ]))],
            Motion::Up => vec![if select_mode {
                ReedlineOption::Edit(EditCommand::MoveLineUp { select: true })
            } else {
                ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                    ReedlineEvent::MenuUp,
                    ReedlineEvent::Up,
                ]))
            }],
            Motion::Down => vec![if select_mode {
                ReedlineOption::Edit(EditCommand::MoveLineDown { select: true })
            } else {
                ReedlineOption::Event(ReedlineEvent::UntilFound(vec![
                    ReedlineEvent::MenuDown,
                    ReedlineEvent::Down,
                ]))
            }],
            // Motions with a `MotionTarget` collapse to one dispatch: resolve the
            // target (see `Motion::target`), then move or extend by visual mode.
            Motion::NextWord
            | Motion::NextBigWord
            | Motion::NextWordEnd
            | Motion::NextBigWordEnd
            | Motion::PreviousWord
            | Motion::PreviousBigWord => {
                let target = self.target().expect("motion resolves to a MotionTarget");
                let edit = if select_mode {
                    EditCommand::Extend(target)
                } else {
                    EditCommand::Move(target)
                };
                vec![ReedlineOption::Edit(edit)]
            }
            Motion::Line => vec![], // Placeholder as unusable standalone motion
            Motion::Start => vec![ReedlineOption::Edit(EditCommand::MoveToLineStart {
                select: select_mode,
            })],
            Motion::NonBlankStart => {
                vec![ReedlineOption::Edit(EditCommand::MoveToLineNonBlankStart {
                    select: select_mode,
                })]
            }
            Motion::End => vec![ReedlineOption::Edit(EditCommand::MoveToLineEnd {
                select: select_mode,
            })],
            Motion::FirstLine => vec![if select_mode {
                ReedlineOption::Edit(EditCommand::MoveToStart { select: true })
            } else {
                ReedlineOption::Event(ReedlineEvent::ToStart)
            }],
            Motion::LastLine => vec![if select_mode {
                ReedlineOption::Edit(EditCommand::MoveToEnd { select: true })
            } else {
                ReedlineOption::Event(ReedlineEvent::ToEnd)
            }],
            Motion::RightUntil(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::ToRight(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveRightUntil {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::RightBefore(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::TillRight(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveRightBefore {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::LeftUntil(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::ToLeft(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveLeftUntil {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::LeftBefore(ch) => {
                vi_state.last_char_search = Some(ViCharSearch::TillLeft(*ch));
                vec![ReedlineOption::Edit(EditCommand::MoveLeftBefore {
                    c: *ch,
                    select: select_mode,
                })]
            }
            Motion::ReplayCharSearch => {
                if let Some(char_search) = vi_state.last_char_search.as_ref() {
                    vec![ReedlineOption::Edit(char_search.to_move(select_mode))]
                } else {
                    vec![]
                }
            }
            Motion::ReverseCharSearch => {
                if let Some(char_search) = vi_state.last_char_search.as_ref() {
                    vec![ReedlineOption::Edit(
                        char_search.reverse().to_move(select_mode),
                    )]
                } else {
                    vec![]
                }
            }
        }
    }
}

/// Vi left-right motions to or till a character.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ViCharSearch {
    /// f
    ToRight(char),
    /// F
    ToLeft(char),
    /// t
    TillRight(char),
    /// T
    TillLeft(char),
}

impl ViCharSearch {
    /// Swap the direction of the to or till for ','
    pub fn reverse(&self) -> Self {
        match self {
            ViCharSearch::ToRight(c) => ViCharSearch::ToLeft(*c),
            ViCharSearch::ToLeft(c) => ViCharSearch::ToRight(*c),
            ViCharSearch::TillRight(c) => ViCharSearch::TillLeft(*c),
            ViCharSearch::TillLeft(c) => ViCharSearch::TillRight(*c),
        }
    }

    pub fn to_move(&self, select_mode: bool) -> EditCommand {
        match self {
            ViCharSearch::ToRight(c) => EditCommand::MoveRightUntil {
                c: *c,
                select: select_mode,
            },
            ViCharSearch::ToLeft(c) => EditCommand::MoveLeftUntil {
                c: *c,
                select: select_mode,
            },
            ViCharSearch::TillRight(c) => EditCommand::MoveRightBefore {
                c: *c,
                select: select_mode,
            },
            ViCharSearch::TillLeft(c) => EditCommand::MoveLeftBefore {
                c: *c,
                select: select_mode,
            },
        }
    }

    pub fn to_cut(&self) -> EditCommand {
        match self {
            ViCharSearch::ToRight(c) => EditCommand::CutRightUntil(*c),
            ViCharSearch::ToLeft(c) => EditCommand::CutLeftUntil(*c),
            ViCharSearch::TillRight(c) => EditCommand::CutRightBefore(*c),
            ViCharSearch::TillLeft(c) => EditCommand::CutLeftBefore(*c),
        }
    }

    pub fn to_copy(&self) -> EditCommand {
        match self {
            ViCharSearch::ToRight(c) => EditCommand::CopyRightUntil(*c),
            ViCharSearch::TillRight(c) => EditCommand::CopyRightBefore(*c),
            ViCharSearch::ToLeft(c) => EditCommand::CopyLeftUntil(*c),
            ViCharSearch::TillLeft(c) => EditCommand::CopyLeftBefore(*c),
        }
    }
}
