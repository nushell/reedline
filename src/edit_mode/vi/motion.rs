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
    Word,
    Line,
    Start,
    End,
    RightUntil(char),
    RightBefore(char),
    LeftUntil(char),
    LeftBefore(char),
}
