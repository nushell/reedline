use std::{iter::Peekable, str::Bytes};

#[derive(Debug, PartialEq, Eq)]
enum Motion {
    Word,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Delete,
    DeleteInner,
    Paste,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,
    EnterViInsert,
}

#[derive(Debug, PartialEq, Eq)]
struct ParseResult {
    multiplier: Option<u64>,
    command: Option<Command>,
    count: Option<u64>,
    motion: Option<Motion>,
}

type InputIterator<'a> = Peekable<Bytes<'a>>;

fn parse_motion(input: &mut InputIterator) -> Option<Motion> {
    match input.peek() {
        Some(b'w') => {
            let _ = input.next();
            Some(Motion::Word)
        }
        _ => None,
    }
}

fn parse_command(input: &mut InputIterator) -> Option<Command> {
    match input.peek() {
        Some(b'd') => {
            let _ = input.next();
            match input.peek() {
                Some(b'i') => {
                    let _ = input.next();
                    Some(Command::DeleteInner)
                }
                _ => Some(Command::Delete),
            }
        }
        Some(b'p') => {
            let _ = input.next();
            Some(Command::Paste)
        }
        Some(b'h') => {
            let _ = input.next();
            Some(Command::MoveLeft)
        }
        Some(b'l') => {
            let _ = input.next();
            Some(Command::MoveRight)
        }
        Some(b'j') => {
            let _ = input.next();
            Some(Command::MoveDown)
        }
        Some(b'k') => {
            let _ = input.next();
            Some(Command::MoveUp)
        }
        Some(b'i') => {
            let _ = input.next();
            Some(Command::EnterViInsert)
        }
        _ => None,
    }
}

fn parse_number(input: &mut InputIterator) -> Option<u64> {
    match input.peek() {
        Some(x) if x.is_ascii_digit() => {
            let mut count: u64 = 0;
            while let Some(&c) = input.peek() {
                if c.is_ascii_digit() {
                    let _ = input.next();
                    count *= 10;
                    count += (c - b'0') as u64;
                } else {
                    return Some(count);
                }
            }
            Some(count)
        }
        _ => None,
    }
}

fn parse(input: &mut InputIterator) -> ParseResult {
    let multiplier = parse_number(input);
    let command = parse_command(input);
    let count = parse_number(input);
    let motion = parse_motion(input);

    println!("multiplier: {:?}", multiplier);
    println!("command: {:?}", command);
    println!("count: {:?}", count);
    println!("motion: {:?}", motion);

    // validate input here

    ParseResult {
        multiplier,
        command,
        count,
        motion,
    }
}

fn vi_parse(input: &str) -> ParseResult {
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
    use pretty_assertions::{assert_eq, assert_ne};

    #[test]
    fn test_delete_word() {
        let input = "dw";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: None,
                command: Some(Command::Delete),
                count: None,
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_delete_word() {
        let input = "2dw";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: None,
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_delete_two_word() {
        let input = "2d2w";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::Delete),
                count: Some(2),
                motion: Some(Motion::Word)
            }
        );
    }

    #[test]
    fn test_two_up() {
        let input = "2k";
        let output = vi_parse(&input);

        assert_eq!(
            output,
            ParseResult {
                multiplier: Some(2),
                command: Some(Command::MoveUp),
                count: None,
                motion: None,
            }
        );
    }
}

/*
2d2w

2w = motion with optional count
2(d2w) = command + motion with optional count

any number multiplies with existing number, eg 2d2w this becomes the equiv 4dw (canonical for 4dw, d4w, etc)

*/
