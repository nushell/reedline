use std::iter::Peekable;

pub fn parse_motion<'iter, I>(input: &mut Peekable<I>) -> Option<Motion>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('b') => {
            let _ = input.next();
            Some(Motion::PreviousWord)
        }
        Some('B') => {
            let _ = input.next();
            Some(Motion::PreviousBigWord)
        }
        Some('w') => {
            let _ = input.next();
            Some(Motion::NextWord)
        }
        Some('W') => {
            let _ = input.next();
            Some(Motion::NextBigWord)
        }
        Some('e') => {
            let _ = input.next();
            Some(Motion::NextWordEnd)
        }
        Some('E') => {
            let _ = input.next();
            Some(Motion::NextBigWordEnd)
        }
        Some('d') => {
            let _ = input.next();
            Some(Motion::Line)
        }
        Some('0') => {
            let _ = input.next();
            Some(Motion::Start)
        }
        Some('$') => {
            let _ = input.next();
            Some(Motion::End)
        }
        Some('f') => {
            let _ = input.next();
            input.peek().map(|c| Motion::RightUntil(**c))
        }
        Some('t') => {
            let _ = input.next();
            input.peek().map(|c| Motion::RightBefore(**c))
        }
        Some('F') => {
            let _ = input.next();
            input.peek().map(|c| Motion::LeftUntil(**c))
        }
        Some('T') => {
            let _ = input.next();
            input.peek().map(|c| Motion::LeftBefore(**c))
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Motion {
    NextWord,
    NextBigWord,
    NextWordEnd,
    NextBigWordEnd,
    PreviousWord,
    PreviousBigWord,
    Line,
    Start,
    End,
    RightUntil(char),
    RightBefore(char),
    LeftUntil(char),
    LeftBefore(char),
}
