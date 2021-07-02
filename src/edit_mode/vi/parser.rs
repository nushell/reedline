use std::{iter::Peekable, str::Bytes};

#[derive(Debug, PartialEq, Eq)]
enum Motion {
    NoMove,
    LeftChar,
    RightChar,
    Up,
    Down,
    WordInner,
    WordAround,
    WordBeginningRight,
    WordEnd,
    WordBeginningLeft,
    LineBeginning,
    LineEnd,
    LineFirstPrint,
    // LineLastPrint, // Requires g switch which complicates the differentiation between motion and command TODO: lookahead without consuming
    WholeLine,
    CharSearch(char, CharSearchOption),
    SameWordForward,
    SameWordBackward,
}

#[derive(Debug, PartialEq, Eq)]
enum CharSearchOption {
    ForwardBefore,  // t
    ForwardOnTop,   // f
    BackwardBefore, // T
    BackwardOnTop,  // F
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Move,
    Delete,
    DeleteChar,
    ReplaceChar(char),
    Copy,
    Uppercase,
    Lowercase,
    SwitchCase,
    Paste,
    EnterViInsert,
    ChangeViInsert,
}

impl Action {
    fn whole_line_repeat_char(&self) -> Option<u8> {
        match *self {
            Action::Delete => Some(b'd'),
            Action::Copy => Some(b'y'),
            Action::Lowercase => Some(b'u'),
            Action::Uppercase => Some(b'U'),
            Action::SwitchCase => Some(b'~'),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ViCommand {
    multiplier: u64,
    action: Action,
    motion: Motion,
}

#[derive(Debug, PartialEq, Eq)]
enum ParseResult<T> {
    Success(T),
    Incomplete,
    Invalid,
}

type InputIterator<'a> = Peekable<Bytes<'a>>;

fn parse_motion(input: &mut InputIterator, is_action_motion: bool) -> ParseResult<Motion> {
    match input.peek() {
        Some(b'h') => {
            let _ = input.next();
            ParseResult::Success(Motion::LeftChar)
        }
        Some(b'l') => {
            let _ = input.next();
            ParseResult::Success(Motion::RightChar)
        }
        Some(b'k') => {
            let _ = input.next();
            ParseResult::Success(Motion::Up)
        }
        Some(b'j') => {
            let _ = input.next();
            ParseResult::Success(Motion::Down)
        }
        Some(b'w') => {
            let _ = input.next();
            ParseResult::Success(Motion::WordBeginningRight)
        }
        Some(b'b') => {
            let _ = input.next();
            ParseResult::Success(Motion::WordBeginningLeft)
        }
        Some(b'e') => {
            let _ = input.next();
            ParseResult::Success(Motion::WordEnd)
        }
        Some(b'i') if is_action_motion => {
            let _ = input.next();
            match input.peek() {
                Some(b'w') => {
                    let _ = input.next();
                    ParseResult::Success(Motion::WordInner)
                }
                None => ParseResult::Incomplete,
                _ => ParseResult::Invalid,
            }
        }
        Some(b'a') if is_action_motion => {
            let _ = input.next();
            match input.peek() {
                Some(b'w') => {
                    let _ = input.next();
                    ParseResult::Success(Motion::WordAround)
                }
                None => ParseResult::Incomplete,
                _ => ParseResult::Invalid,
            }
        }
        Some(b'0') => {
            let _ = input.next();
            ParseResult::Success(Motion::LineBeginning)
        }
        Some(b'$') => {
            let _ = input.next();
            ParseResult::Success(Motion::LineEnd)
        }
        Some(b'_') => {
            let _ = input.next();
            ParseResult::Success(Motion::LineFirstPrint)
        }
        Some(b'*') => {
            let _ = input.next();
            ParseResult::Success(Motion::SameWordForward)
        }
        Some(b'#') => {
            let _ = input.next();
            ParseResult::Success(Motion::SameWordBackward)
        }
        Some(b'f') | Some(b'F') | Some(b't') | Some(b'T') => {
            let search_option = match input.next() {
                Some(b'f') => CharSearchOption::ForwardOnTop,
                Some(b'F') => CharSearchOption::BackwardOnTop,
                Some(b't') => CharSearchOption::ForwardBefore,
                Some(b'T') => CharSearchOption::BackwardBefore,
                _ => {unreachable!();}
            };
            match input.peek() {
                Some(&x) => {
                    // TODO: Support unicode chars as well
                    let _ = input.next();
                    ParseResult::Success(Motion::CharSearch(x.into(), search_option))
                }
                None => ParseResult::Incomplete,
            }
        }
        None => ParseResult::Incomplete,

        _ => ParseResult::Invalid,
    }
}

fn parse_action(input: &mut InputIterator) -> ParseResult<(Action, Option<Motion>)> {
    match input.peek() {
        Some(b'd') => {
            let _ = input.next();
            ParseResult::Success((Action::Delete, None))
        }
        Some(b'D') => {
            let _ = input.next();
            ParseResult::Success((Action::Delete, Some(Motion::LineEnd)))
        }
        Some(b'y') => {
            let _ = input.next();
            ParseResult::Success((Action::Copy, None))
        }
        Some(b'Y') => {
            let _ = input.next();
            ParseResult::Success((Action::Copy, Some(Motion::WholeLine)))
        }
        Some(b'p') => {
            let _ = input.next();
            ParseResult::Success((Action::Paste, Some(Motion::NoMove)))
        }
        Some(b'i') => {
            let _ = input.next();
            ParseResult::Success((Action::EnterViInsert, Some(Motion::NoMove)))
        }
        Some(b'a') => {
            let _ = input.next();
            ParseResult::Success((Action::EnterViInsert, Some(Motion::LeftChar)))
        }
        Some(b'c') => {
            let _ = input.next();
            ParseResult::Success((Action::ChangeViInsert, None))
        }
        Some(b'C') => {
            let _ = input.next();
            ParseResult::Success((Action::ChangeViInsert, Some(Motion::LineEnd)))
        }
        Some(_) => match parse_motion(input, false) {
            ParseResult::Success(motion) => ParseResult::Success((Action::Move, Some(motion))),
            ParseResult::Incomplete => ParseResult::Incomplete,
            ParseResult::Invalid => ParseResult::Invalid,
        },
        None => ParseResult::Incomplete,
    }
}

fn parse_number(input: &mut InputIterator) -> u64 {
    match input.peek() {
        Some(x) if x.is_ascii_digit() => {
            if *x == b'0' {
                // Bare `0` is a movement executed once
                return 1;
            }
            let mut count: u64 = 0;
            while let Some(&c) = input.peek() {
                if c.is_ascii_digit() {
                    let _ = input.next();
                    count *= 10;
                    count += (c - b'0') as u64;
                } else {
                    break;
                }
            }
            count
        }
        _ => 1,
    }
}

fn parse(input: &mut InputIterator) -> ParseResult<ViCommand> {
    let mut multiplier = parse_number(input);
    let command = parse_action(input);
    match command {
        ParseResult::Success((action, None)) => {
            multiplier *= parse_number(input);
            match parse_motion(input, true) {
                ParseResult::Success(motion) => ParseResult::Success(ViCommand {
                    multiplier,
                    action,
                    motion,
                }),
                ParseResult::Incomplete => ParseResult::Incomplete,
                ParseResult::Invalid => ParseResult::Invalid,
            }
        }
        ParseResult::Success((action, Some(motion))) => ParseResult::Success(ViCommand {
            multiplier,
            action,
            motion,
        }),
        ParseResult::Incomplete => ParseResult::Incomplete,
        ParseResult::Invalid => ParseResult::Invalid,
    }
}

fn vi_parse(input: &str) -> ParseResult<ViCommand> {
    let mut bytes = input.bytes().peekable();

    parse(&mut bytes)
}

fn main() {
    for arg in std::env::args().skip(1) {
        println!("{:?}", vi_parse(&arg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_delete_word() {
        let input = "dw";
        let output = vi_parse(&input);

        // assert_eq!(
        //     output,
        //     ViCommand {
        //         multiplier: NonZeroU64::new(1).unwrap(),
        //         action: Some(Action::Delete),
        //         motion: Some(Motion::WordBeginningRight)
        //     }
        // );
    }

    #[test]
    fn test_two_delete_word() {
        let input = "2dw";
        let output = vi_parse(&input);

        // assert_eq!(
        //     output,
        //     ViCommand {
        //         multiplier: NonZeroU64::new(2).unwrap(),
        //         action: Some(Action::Delete),
        //         motion: Some(Motion::WordBeginningRight)
        //     }
        // );
    }

    #[test]
    fn test_two_delete_two_word() {
        let input = "2d2w";
        let output = vi_parse(&input);

        // assert_eq!(
        //     output,
        //     ViCommand {
        //         multiplier: NonZeroU64::new(4).unwrap(),
        //         action: Some(Action::Delete),
        //         motion: Some(Motion::WordBeginningRight)
        //     }
        // );
    }

    #[test]
    fn test_two_up() {
        let input = "2k";
        let output = vi_parse(&input);

        // assert_eq!(
        //     output,
        //     ViCommand {
        //         multiplier: NonZeroU64::new(2).unwrap(),
        //         action: Some(Action::Move),
        //         motion: Some(Motion::Up),
        //     }
        // );
    }

    fn fixture_number_parsing(input: &str) -> u64 {
        let string = input.to_string();
        let mut iter = string.bytes().peekable();
        parse_number(&mut iter)
    }

    #[test]
    fn test_number_parsing() {
        assert_eq!(fixture_number_parsing(""), 1);

        assert_eq!(fixture_number_parsing("x"), 1);
        assert_eq!(fixture_number_parsing("0"), 1);
        assert_eq!(fixture_number_parsing("10"), 10);
        assert_eq!(fixture_number_parsing("10b"), 10);
    }
}

/*
2d2w

2w = motion with optional count
2(d2w) = command + motion with optional count

any number multiplies with existing number, eg 2d2w this becomes the equiv 4dw (canonical for 4dw, d4w, etc)

*/
