use std::{collections::HashSet, ops::Deref};

use crate::{
    history::SearchQuery, menu_functions::parse_selection_char, Completer, History, HistoryItem,
    Result, Span, Suggestion,
};

const SELECTION_CHAR: char = '!';

// The HistoryCompleter is created just before updating the menu
// It pulls data from the object that contains access to the History
pub(crate) struct HistoryCompleter<'menu>(&'menu dyn History);

// Safe to implement Send since the HistoryCompleter should only be used when
// updating the menu and that must happen in the same thread
unsafe impl Send for HistoryCompleter<'_> {}

fn search_unique(
    completer: &HistoryCompleter,
    line: &str,
) -> Result<impl Iterator<Item = HistoryItem>> {
    let parsed = parse_selection_char(line, SELECTION_CHAR);
    let values = completer.0.search(SearchQuery::all_that_contain_rev(
        parsed.remainder.to_string(),
    ))?;

    let mut seen_matching_command_lines = HashSet::new();
    Ok(values
        .into_iter()
        .filter(move |value| seen_matching_command_lines.insert(value.command_line.clone())))
}

impl Completer for HistoryCompleter<'_> {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        match search_unique(self, line) {
            Err(_) => vec![],
            Ok(search_results) => search_results
                .map(|value| self.create_suggestion(line, pos, value.command_line.deref()))
                .collect(),
        }
    }

    // TODO: Implement `fn partial_complete()`

    fn total_completions(&mut self, line: &str, _pos: usize) -> usize {
        search_unique(self, line).map(|i| i.count()).unwrap_or(0)
    }
}

impl<'menu> HistoryCompleter<'menu> {
    pub fn new(history: &'menu dyn History) -> Self {
        Self(history)
    }

    fn create_suggestion(&self, line: &str, pos: usize, value: &str) -> Suggestion {
        let span = Span {
            start: pos - line.len(),
            end: pos,
        };

        Suggestion {
            value: value.to_string(),
            description: None,
            style: None,
            extra: None,
            span,
            append_whitespace: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::*;

    fn new_history_item(command_line: &str) -> HistoryItem {
        HistoryItem {
            id: None,
            start_timestamp: None,
            command_line: command_line.to_string(),
            session_id: None,
            hostname: None,
            cwd: None,
            duration: None,
            exit_status: None,
            more_info: None,
        }
    }

    #[test]
    fn duplicates_in_history() -> Result<()> {
        let command_line_history = vec![
            "git stash drop",
            "git add .",
            "git status | something | else",
            "git status",
            "git commit -m 'something'",
            "ls",
            "git push",
            "git status",
        ];
        let expected_history_size = command_line_history.len();
        let mut history = FileBackedHistory::new(command_line_history.len())?;
        for command_line in command_line_history {
            history.save(new_history_item(command_line))?;
        }
        let input = "git s";
        let mut sut = HistoryCompleter::new(&history);

        let actual = sut.complete(input, input.len());
        let num_completions = sut.total_completions(input, input.len());

        assert_eq!(actual[0].value, "git status", "it was the last command");
        assert_eq!(
            actual[1].value, "git status | something | else",
            "next match that is not 'git status' again even though it is next in history"
        );
        assert_eq!(actual[2].value, "git stash drop", "last match");
        assert_eq!(actual.get(3), None);
        assert_eq!(
            3, num_completions,
            "total_completions is consistent with complete"
        );

        assert_eq!(
            history.count_all().expect("no error") as usize,
            expected_history_size,
            "History contains duplicates."
        );
        Ok(())
    }

    #[rstest]
    #[case(vec![], "any", vec![])]
    #[case(vec!["old match","recent match","between","recent match"], "match", vec!["recent match","old match"])]
    #[case(vec!["a","b","c","a","b","c"], "", vec!["c","b","a"])]
    fn complete_doesnt_return_duplicates(
        #[case] history_items: Vec<&str>,
        #[case] line: &str,
        #[case] expected: Vec<&str>,
    ) -> Result<()> {
        let mut history = FileBackedHistory::new(history_items.len())?;
        for history_item in history_items {
            history.save(new_history_item(history_item))?;
        }
        let mut sut = HistoryCompleter::new(&history);
        let actual: Vec<String> = sut
            .complete(line, line.len())
            .into_iter()
            .map(|suggestion| suggestion.value)
            .collect();
        assert_eq!(actual, expected);
        Ok(())
    }
}
