use {
    crate::line_buffer::LineBuffer,
    std::{
        collections::{BTreeMap, BTreeSet},
        rc::Rc,
        str::Chars,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    // Creates a new `Span` from start and end inputs.
    // The end parameter must be greater than or equal to the start parameter.
    pub fn new(start: usize, end: usize) -> Span {
        assert!(
            end >= start,
            "Can't create a Span whose end < start, start={}, end={}",
            start,
            end
        );

        Span { start, end }
    }
}

pub trait TabHandler {
    fn handle(&mut self, line: &mut LineBuffer);
    fn reset_index(&mut self);
    fn get_completer(&self) -> &Box<dyn Completer>;
}

pub struct DefaultTabHandler {
    completer: Box<dyn Completer>,
    initial_line: LineBuffer,
    index: usize,
}

impl DefaultTabHandler {
    pub fn with_completer(mut self, completer: Box<dyn Completer>) -> DefaultTabHandler {
        self.completer = completer;
        self
    }
}
impl Default for DefaultTabHandler {
    fn default() -> Self {
        DefaultTabHandler {
            completer: Box::new(DefaultCompleter::default()),
            initial_line: LineBuffer::new(),
            index: 0,
        }
    }
}
impl TabHandler for DefaultTabHandler {
    fn get_completer(&self) -> &Box<dyn Completer> {
        &self.completer
    }
    // With this function we handle the tab events.
    //
    // If completions vector is not empty we proceed to replace
    //  in the line_buffer only the specified range of characters.
    // If internal index is 0 it means that is the first tab event pressed.
    // If internal index is greater than completions vector, we bring it back to 0.
    fn handle(&mut self, line: &mut LineBuffer) {
        if self.index == 0 {
            self.initial_line = LineBuffer::new();
            self.initial_line.set_buffer(line.get_buffer().into());
            self.initial_line
                .set_insertion_point(line.insertion_point());
        } else {
            line.set_buffer(self.initial_line.get_buffer().into());
            line.set_insertion_point(self.initial_line.insertion_point())
        }
        let completions = self.completer.complete(
            self.initial_line.get_buffer(),
            self.initial_line.insertion_point().offset,
        );
        if !completions.is_empty() {
            match self.index {
                index if index < completions.len() => {
                    self.index += 1;
                    let span = completions[index].0;
                    let mut insertion_point = line.insertion_point();
                    insertion_point.offset += completions[index].1.len() - (span.end - span.start);

                    // TODO improve the support for multiline replace
                    line.replace(span.start..span.end, 0, &completions[index].1);
                    line.set_insertion_point(insertion_point);
                }
                _ => {
                    self.reset_index();
                }
            }
        }
    }

    // This function is required to reset the index
    // when following the completion we perform another action
    // that is not going to continue with the list of completions.
    fn reset_index(&mut self) {
        self.index = 0;
    }
}

pub trait Completer {
    fn complete(&self, line: &str, pos: usize) -> Vec<(Span, String)>;
}

#[derive(Debug, Clone)]
pub struct DefaultCompleter {
    root: CompletionNode,
    inclusions: Rc<BTreeSet<char>>,
    min_word_len: usize,
}

impl Default for DefaultCompleter {
    fn default() -> Self {
        let inclusions = Rc::new(BTreeSet::new());
        Self {
            root: CompletionNode::new(inclusions.clone()),
            inclusions,
            min_word_len: 2,
        }
    }
}
impl Completer for DefaultCompleter {
    /// Returns a vector of completions and the position in which they must be replaced;
    /// based on the provided input.
    ///
    /// # Arguments
    ///
    /// * `line`    The line to complete
    /// * `pos`   The cursor position
    ///
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer,Span};
    ///
    /// let mut completions = DefaultCompleter::default();
    /// completions.insert(vec!["batman","robin","batmobile","batcave","robber"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(
    ///     completions.complete("bat",3),
    ///     vec![
    ///         (Span { start: 0, end: 3 }, "batcave".into()),
    ///         (Span { start: 0, end: 3 }, "batman".into()),
    ///         (Span { start: 0, end: 3 }, "batmobile".into()),
    ///     ]);
    ///
    /// assert_eq!(
    ///     completions.complete("to the bat",10),
    ///     vec![
    ///         (Span { start: 7, end: 10 }, "batcave".into()),
    ///         (Span { start: 7, end: 10 }, "batman".into()),
    ///         (Span { start: 7, end: 10 }, "batmobile".into()),
    ///     ]);
    /// ```
    fn complete(&self, line: &str, pos: usize) -> Vec<(Span, String)> {
        let mut span_line_whitespaces = 0;
        let mut completions = vec![];
        if !line.is_empty() {
            let mut splitted = line[0..pos].split(' ').rev();
            let mut span_line: String = String::new();
            for _ in 0..splitted.clone().count() {
                if let Some(s) = splitted.next() {
                    if s.is_empty() {
                        span_line_whitespaces += 1;
                        continue;
                    }
                    if span_line.is_empty() {
                        span_line = s.to_string();
                    } else {
                        span_line = format!("{} {}", s, span_line);
                    }
                    if let Some(mut extensions) = self.root.complete(span_line.chars()) {
                        extensions.sort();
                        completions.extend(
                            extensions
                                .iter()
                                .map(|ext| {
                                    (
                                        Span::new(
                                            pos - span_line.len() - span_line_whitespaces,
                                            pos,
                                        ),
                                        format!("{}{}", span_line, ext),
                                    )
                                })
                                .filter(|t| t.1.len() > (t.0.end - t.0.start))
                                .collect::<Vec<(Span, String)>>(),
                        );
                    }
                }
            }
        }
        completions.dedup();
        completions
    }
}
impl DefaultCompleter {
    pub fn new(external_commands: Vec<String>) -> Self {
        let mut dc = DefaultCompleter::default();
        dc.insert(external_commands);
        dc
    }

    pub fn new_with_wordlen(external_commands: Vec<String>, min_word_len: usize) -> Self {
        let mut dc = DefaultCompleter::default().set_min_word_len(min_word_len);
        dc.insert(external_commands);
        dc
    }

    /// Insert external_commands list in the object root
    ///
    /// # Arguments
    ///
    /// * `line`    A vector of String containing the external commands
    ///
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer};
    ///
    /// let mut completions = DefaultCompleter::default();
    ///
    /// // Insert multiple words
    /// completions.insert(vec!["a","line","with","many","words"].iter().map(|s| s.to_string()).collect());
    ///
    /// // The above line is equal to the following:
    /// completions.insert(vec!["a","line","with"].iter().map(|s| s.to_string()).collect());
    /// completions.insert(vec!["many","words"].iter().map(|s| s.to_string()).collect());
    /// ```
    pub fn insert(&mut self, external_commands: Vec<String>) {
        for word in external_commands {
            if word.len() >= self.min_word_len {
                self.root.insert(word.chars());
            }
        }
    }

    /// Create a new DefaultCompleter with provided non alphabet characters whitelisted.
    /// The default DefaultCompleter will only parse alphabet characters (a-z, A-Z). Use this to
    /// introduce additional accepted special characters.
    ///
    /// # Arguments
    ///
    /// * `incl`    An array slice with allowed characters
    ///
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer,Span};
    ///
    /// let mut completions = DefaultCompleter::default();
    /// completions.insert(vec!["test-hyphen","test_underscore"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(
    ///     completions.complete("te",2),
    ///     vec![(Span { start: 0, end: 2 }, "test".into())]);
    ///
    /// let mut completions = DefaultCompleter::with_inclusions(&['-', '_']);
    /// completions.insert(vec!["test-hyphen","test_underscore"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(
    ///     completions.complete("te",2),
    ///     vec![
    ///         (Span { start: 0, end: 2 }, "test-hyphen".into()),
    ///         (Span { start: 0, end: 2 }, "test_underscore".into()),
    ///     ]);
    /// ```
    pub fn with_inclusions(incl: &[char]) -> Self {
        let mut set = BTreeSet::new();
        incl.iter().for_each(|c| {
            set.insert(*c);
        });
        let inclusions = Rc::new(set);
        Self {
            root: CompletionNode::new(inclusions.clone()),
            inclusions,
            ..Self::default()
        }
    }

    /// Clears all the data from the tree
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer};
    ///
    /// let mut completions = DefaultCompleter::default();
    /// completions.insert(vec!["batman","robin","batmobile","batcave","robber"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(completions.word_count(), 5);
    /// assert_eq!(completions.size(), 24);
    /// completions.clear();
    /// assert_eq!(completions.size(), 1);
    /// assert_eq!(completions.word_count(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.root.clear();
    }

    /// Returns a count of how many words that exist in the tree
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer};
    ///
    /// let mut completions = DefaultCompleter::default();
    /// completions.insert(vec!["batman","robin","batmobile","batcave","robber"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(completions.word_count(), 5);
    /// ```
    pub fn word_count(&self) -> u32 {
        self.root.word_count()
    }

    /// Returns the size of the tree, the amount of nodes, not words
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer};
    ///
    /// let mut completions = DefaultCompleter::default();
    /// completions.insert(vec!["batman","robin","batmobile","batcave","robber"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(completions.size(), 24);
    /// ```
    pub fn size(&self) -> u32 {
        self.root.subnode_count()
    }

    /// Returns the minimum word length to complete. This allows you
    /// to pass full sentences to `insert()` and not worry about
    /// pruning out small words like "a" or "to", because they will be
    /// ignored.
    /// # Example
    /// ```
    /// use reedline::{DefaultCompleter,Completer};
    ///
    /// let mut completions = DefaultCompleter::default().set_min_word_len(4);
    /// completions.insert(vec!["one","two","three","four","five"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(completions.word_count(), 3);
    ///
    /// let mut completions = DefaultCompleter::default().set_min_word_len(1);
    /// completions.insert(vec!["one","two","three","four","five"].iter().map(|s| s.to_string()).collect());
    /// assert_eq!(completions.word_count(), 5);
    /// ```
    pub fn min_word_len(&self) -> usize {
        self.min_word_len
    }

    /// Sets the minimum word length to complete on. Smaller words are
    /// ignored. This only affects future calls to `insert()` -
    /// changing this won't start completing on smaller words that
    /// were added in the past, nor will it exclude larger words
    /// already inserted into the completion tree.
    pub fn set_min_word_len(mut self, len: usize) -> DefaultCompleter {
        self.min_word_len = len;
        self
    }
}

#[derive(Debug, Clone)]
struct CompletionNode {
    subnodes: BTreeMap<char, CompletionNode>,
    leaf: bool,
    inclusions: Rc<BTreeSet<char>>,
}

impl CompletionNode {
    fn new(incl: Rc<BTreeSet<char>>) -> Self {
        Self {
            subnodes: BTreeMap::new(),
            leaf: false,
            inclusions: incl,
        }
    }

    fn clear(&mut self) {
        self.subnodes.clear();
    }

    fn word_count(&self) -> u32 {
        let mut count = self.subnodes.values().map(|n| n.word_count()).sum();
        if self.leaf {
            count += 1;
        }
        count
    }

    fn subnode_count(&self) -> u32 {
        self.subnodes
            .values()
            .map(|n| n.subnode_count())
            .sum::<u32>()
            + 1
    }

    fn insert(&mut self, mut iter: Chars) {
        if let Some(c) = iter.next() {
            if self.inclusions.contains(&c) || c.is_alphanumeric() || c.is_whitespace() {
                let inclusions = self.inclusions.clone();
                let subnode = self
                    .subnodes
                    .entry(c)
                    .or_insert_with(|| CompletionNode::new(inclusions));
                subnode.insert(iter);
            } else {
                self.leaf = true;
            }
        } else {
            self.leaf = true;
        }
    }

    fn complete(&self, mut iter: Chars) -> Option<Vec<String>> {
        if let Some(c) = iter.next() {
            if let Some(subnode) = self.subnodes.get(&c) {
                subnode.complete(iter)
            } else {
                None
            }
        } else {
            Some(self.collect("".to_string()))
        }
    }

    fn collect(&self, partial: String) -> Vec<String> {
        let mut completions = vec![];
        if self.leaf {
            completions.push(partial.clone());
        }

        if !self.subnodes.is_empty() {
            for (c, node) in &self.subnodes {
                let mut partial = partial.clone();
                partial.push(*c);
                completions.append(&mut node.collect(partial));
            }
        }
        completions
    }
}
