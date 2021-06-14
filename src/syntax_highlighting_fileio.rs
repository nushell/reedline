use nu_ansi_term::{Color, Style};
use nu_json::Value as nujVal;
use std::collections::HashMap;
use std::io::Read;
use std::{fs::File, num::ParseIntError};

// fn main() {
//     // this needs tweaking so we don't rely on env vars
//     let syntax_file_location = match std::env::var("REEDLINE_SYNTAX_FILE") {
//         Ok(synpath) => synpath,
//         Err(_) => "".to_string(),
//     };
//     // get the contents of the syntax file into a string
//     let syntax_buffer = syntax_file_contents(syntax_file_location);
//     // parse the syntax buffer json string into a key value where key is a string
//     // and value is nu_ansi_term::Style
//     let syntax = parse_syntax_buffer(syntax_buffer);
//     // just show what we parsed
//     print!("Syntax: {:?}", syntax);
// }

#[allow(dead_code)]
fn parse_syntax_buffer(syntax_buffer: String) -> HashMap<String, Style> {
    // if there's a minimal buffer let's just return a new hashmap which should
    // indicate to the consumer to use the default syntax highlighting
    if syntax_buffer.chars().count() < 1 {
        HashMap::new()
    } else {
        // a new empty Hashmap for our settings
        let mut syntax_hash: HashMap<String, Style> = HashMap::new();
        // using the nushell nu_json crate, open the syntax json file. The reason
        // for nu_json is because it allows comments
        let data: nujVal = nu_json::from_str(&syntax_buffer)
            .unwrap_or_else(|_| panic!("unable to load json syntax"));
        // there may be a better way to do this but this puts the json into an
        // iterable object so we can go through it parsing out the key values
        let obj = data.as_object().expect("error with json object");
        for (key, value) in obj.iter() {
            syntax_hash.insert(key.to_string(), parse_color((*value).to_string()));
        }

        syntax_hash
    }
}

#[allow(dead_code)]
fn syntax_file_contents(syntax_file_path: String) -> String {
    // open the file, don't hand errors. LOL. Please help!
    let mut file = File::open(&syntax_file_path)
        .unwrap_or_else(|_| panic!("unable to open file {}", syntax_file_path));
    // where to put the file contents
    let mut buffer = String::new();
    // read into a string and pass it back
    file.read_to_string(&mut buffer)
        .unwrap_or_else(|_| panic!("unable to read file {} into buffer", syntax_file_path));

    buffer
}

#[allow(dead_code)]
fn parse_color(line: String) -> Style {
    // expecting "#ff0000|#ffffff|b" where the tokens are
    // foreground|background|attributes
    // valid attributes are
    // b = bold
    // d = dimmed
    // i = italic
    // u = underline
    // l = blink
    // r = reverse
    // h = hidden
    // s = strikethrough
    // n = normal - attributes are ignored
    // If you want to foreground only set it like "#ff0000||attribute"
    // If you want to have background only set it like "|#ffffff|attribute"

    // let line = json_line.to_string();
    let tokens: Vec<&str> = line.trim_matches('\"').split('|').collect();
    match tokens.len() {
        3 => {
            let syntax_style = parse_syntax_color(&tokens[0], &tokens[1], &tokens[2]);
            syntax_style.expect("unable to process syntax style")
        }
        _ => Style::new(),
    }
}

#[allow(dead_code)]
fn color_from_hex(hex_color: &str) -> Result<Option<Color>, ParseIntError> {
    // right now we only allow hex colors with hashtag and 6 characters
    let trimmed = hex_color.trim_matches('#');
    if trimmed.len() != 6 {
        Ok(None)
    } else {
        // make a nu_ansi_term::Color::Rgb color by converting hex to decimal
        Ok(Some(Color::Rgb(
            u8::from_str_radix(&trimmed[..2], 16)?,
            u8::from_str_radix(&trimmed[2..4], 16)?,
            u8::from_str_radix(&trimmed[4..6], 16)?,
        )))
    }
}

#[allow(dead_code)]
fn parse_syntax_color(fg: &str, bg: &str, attr: &str) -> Result<Style, String> {
    // get the nu_ansi_term::Color foreground color
    let fg_color = match color_from_hex(fg) {
        Ok(f) => f,
        _ => return Err("Error with foreground color".to_string()),
    };
    // get the nu_ansi_term::Color background color
    let bg_color = match color_from_hex(bg) {
        Ok(b) => b,
        _ => return Err("Error with background color".to_string()),
    };

    // setup the attributes available in nu_ansi_term::Style
    let mut bold = false;
    let mut dimmed = false;
    let mut italic = false;
    let mut underline = false;
    let mut blink = false;
    let mut reverse = false;
    let mut hidden = false;
    let mut strikethrough = false;

    // since we can combine styles like bold-italic, iterate through the chars
    // and set the bools for later use in the nu_ansi_term::Style application
    for ch in attr.to_lowercase().chars() {
        match ch {
            'l' => blink = true,
            'b' => bold = true,
            'd' => dimmed = true,
            'h' => hidden = true,
            'i' => italic = true,
            'r' => reverse = true,
            's' => strikethrough = true,
            'u' => underline = true,
            'n' => (),
            _ => return Err("Error with color attributes".to_string()),
        }
    }

    // here's where we build the nu_ansi_term::Style
    Ok(Style {
        foreground: fg_color,
        background: bg_color,
        is_blink: blink,
        is_bold: bold,
        is_dimmed: dimmed,
        is_hidden: hidden,
        is_italic: italic,
        is_reverse: reverse,
        is_strikethrough: strikethrough,
        is_underline: underline,
    })
}

#[test]
fn test_syntax_parsing() {
    let syntax_json = "{
        \"internal_command\": \"#ffcc00|#8cfcfc|b\",
        \"sub_command\": \"#ff0000|#ffffff|bli\",
    }"
    .to_string();
    let expected =
        "{\"internal_command\": Style { fg(Rgb(255, 204, 0)), on(Rgb(140, 252, 252)), bold }, \"sub_command\": Style { fg(Rgb(255, 0, 0)), on(Rgb(255, 255, 255)), blink, bold, italic }}";
    let syntax = parse_syntax_buffer(syntax_json);
    assert_eq!(expected, format!("{:?}", syntax));
}
