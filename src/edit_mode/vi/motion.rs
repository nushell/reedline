use std::iter::Peekable;

pub fn parse_motion<'iter, I>(input: &mut Peekable<I>) -> Option<Motion>
where
    I: Iterator<Item = &'iter char>,
{
    match input.peek() {
        Some('w') => {
            let _ = input.next();
            Some(Motion::Word)
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
            match input.peek() {
                Some(c) => Some(Motion::RightUntil(**c)),
                None => None,
            }
        }
        Some('t') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::RightBefore(**c)),
                None => None,
            }
        }
        Some('F') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::LeftUntil(**c)),
                None => None,
            }
        }
        Some('T') => {
            let _ = input.next();
            match input.peek() {
                Some(c) => Some(Motion::LeftBefore(**c)),
                None => None,
            }
        }
        _ => None,
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Motion {
    Word,
    Line,
    Start,
    End,
    RightUntil(char),
    RightBefore(char),
    LeftUntil(char),
    LeftBefore(char),
}
