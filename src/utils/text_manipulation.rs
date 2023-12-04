use unicode_segmentation::UnicodeSegmentation;

pub fn remove_last_grapheme(string: &str) -> &str {
    let mut it = UnicodeSegmentation::graphemes(string, true);

    if it.next_back().is_some() {
        it.as_str()
    } else {
        ""
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn remove_last_char_works_with_empty_string() {
        let string = "";

        assert_eq!(remove_last_grapheme(string), "");
    }

    #[test]
    fn remove_last_char_works_with_normal_string() {
        let string = "this is a string";

        assert_eq!(remove_last_grapheme(string), "this is a strin");
    }

    #[test]
    fn remove_last_char_works_with_string_containing_emojis() {
        let string = "this is a 😞😄";

        assert_eq!(remove_last_grapheme(string), "this is a 😞");
    }
}
