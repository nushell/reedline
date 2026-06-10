use std::{collections::HashSet, ops::Deref};

use crate::{
    history::SearchQuery, menu_functions::parse_selection_char, Completer, History, HistoryItem,
    Result, Span, Suggestion,
};

const SELECTION_CHAR: char = '!';

// The HistoryCompleter is created just before updating the menu
// It pulls data from the object that contains access to the History
pub(crate) struct HistoryCompleter<'menu> {
    history: &'menu dyn History,
    buffer: Option<String>,
}

struct SearchPrompt {
    text: String,
    replace_span: Span,
}

fn search_ranked(
    completer: &HistoryCompleter,
    line: &str,
    pos: usize,
) -> Result<(Vec<HistoryItem>, Option<SearchPrompt>)> {
    let parsed = parse_selection_char(line, SELECTION_CHAR);
    let values = completer.history.search(SearchQuery::all_that_contain_rev(
        parsed.remainder.to_string(),
    ))?;
    let prompt = completer.search_prompt(line, pos);
    let mut seen_matching_command_lines = HashSet::new();

    let values = values
        .into_iter()
        .filter(|value| seen_matching_command_lines.insert(value.command_line.clone()));

    let Some(prompt) = prompt else {
        return Ok((values.collect(), None));
    };

    let (mut matches, other_matches): (Vec<_>, Vec<_>) =
        values.partition(|value| value.command_line.contains(&prompt.text));

    if matches.is_empty() {
        Ok((other_matches, None))
    } else {
        matches.extend(other_matches);
        Ok((matches, Some(prompt)))
    }
}

impl Completer for HistoryCompleter<'_> {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        match search_ranked(self, line, pos) {
            Err(_) => vec![],
            Ok((values, prompt)) => values
                .into_iter()
                .map(|value| {
                    self.create_suggestion(line, pos, value.command_line.deref(), prompt.as_ref())
                })
                .collect(),
        }
    }

    // TODO: Implement `fn partial_complete()`

    fn total_completions(&mut self, line: &str, pos: usize) -> usize {
        search_ranked(self, line, pos)
            .map(|(values, _)| values.len())
            .unwrap_or(0)
    }
}

impl<'menu> HistoryCompleter<'menu> {
    pub fn new(history: &'menu dyn History) -> Self {
        Self {
            history,
            buffer: None,
        }
    }

    pub fn with_buffer(mut self, buffer: &str) -> Self {
        self.buffer = Some(buffer.to_string());
        self
    }

    fn search_prompt(&self, line: &str, pos: usize) -> Option<SearchPrompt> {
        let buffer = self.buffer.as_deref()?;
        let start = pos.checked_sub(line.len())?;
        if buffer.get(start..pos)? != line {
            return None;
        }

        let before_search = &buffer[..start];
        let prompt = before_search.trim();
        let leading_whitespace = before_search.len() - before_search.trim_start().len();

        (!prompt.is_empty()).then(|| SearchPrompt {
            text: prompt.to_string(),
            replace_span: Span::new(leading_whitespace, pos),
        })
    }

    /// Assumes `line.len() <= pos` (i.e. `line` is the cursor-prefix slice).
    /// Update this span calculation before HistoryMenu opts into `InputMode::FullBuffer`,
    /// where `line` would be the entire buffer and `pos - line.len()` would underflow.
    fn create_suggestion(
        &self,
        line: &str,
        pos: usize,
        value: &str,
        prompt: Option<&SearchPrompt>,
    ) -> Suggestion {
        let span = prompt
            .filter(|prompt| value.contains(&prompt.text))
            .map(|prompt| prompt.replace_span)
            .unwrap_or(Span {
                start: pos - line.len(),
                end: pos,
            });

        Suggestion {
            value: value.to_string(),
            description: None,
            style: None,
            extra: None,
            span,
            append_whitespace: false,
            ..Default::default()
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

    fn apply_suggestion(buffer: &str, suggestion: &Suggestion) -> String {
        let mut buffer = buffer.to_string();
        buffer.replace_range(suggestion.span.start..suggestion.span.end, &suggestion.value);
        buffer
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

    #[test]
    fn diff_history_prioritizes_existing_prompt() -> Result<()> {
        let mut history = FileBackedHistory::new(5)?;
        for history_item in [
            "git status",
            "git stash",
            "docker logs",
            "git log --oneline",
            "hg log",
        ] {
            history.save(new_history_item(history_item))?;
        }

        let buffer = "git st";
        let mut sut = HistoryCompleter::new(&history).with_buffer(buffer);
        let actual = sut.complete("", buffer.len());
        let actual_values: Vec<_> = actual
            .iter()
            .map(|suggestion| suggestion.value.as_str())
            .collect();

        assert_eq!(
            actual_values,
            [
                "git stash",
                "git status",
                "hg log",
                "git log --oneline",
                "docker logs"
            ]
        );
        assert_eq!(actual[0].span, Span::new(0, buffer.len()));
        assert_eq!(apply_suggestion(buffer, &actual[0]), "git stash");
        assert_eq!(sut.total_completions("", buffer.len()), 5);

        let buffer = "git log";
        let line = "log";
        let mut sut = HistoryCompleter::new(&history).with_buffer(buffer);
        let actual = sut.complete(line, buffer.len());
        let actual_values: Vec<_> = actual
            .iter()
            .map(|suggestion| suggestion.value.as_str())
            .collect();

        assert_eq!(actual_values, ["git log --oneline", "hg log", "docker logs"]);
        assert_eq!(actual[0].span, Span::new(0, buffer.len()));
        assert_eq!(apply_suggestion(buffer, &actual[0]), "git log --oneline");
        assert_eq!(actual[1].span, Span::new(buffer.len() - line.len(), buffer.len()));
        assert_eq!(apply_suggestion(buffer, &actual[1]), "git hg log");
        Ok(())
    }

    #[test]
    fn diff_history_falls_back_when_existing_prompt_does_not_match() -> Result<()> {
        let mut history = FileBackedHistory::new(3)?;
        for history_item in ["cargo test", "ls", "docker logs"] {
            history.save(new_history_item(history_item))?;
        }

        let buffer = "zzlog";
        let line = "log";
        let mut sut = HistoryCompleter::new(&history).with_buffer(buffer);
        let actual_values: Vec<_> = sut
            .complete(line, buffer.len())
            .into_iter()
            .map(|suggestion| suggestion.value)
            .collect();

        assert_eq!(actual_values, ["docker logs"]);
        Ok(())
    }
}
