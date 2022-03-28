/// This outlines the operations (interface) that any data strcuture will need to obey to be useful
/// to the core_editor as undo/redo stack
pub trait EditStack<T>: Send
where
    T: Default,
    T: Clone,
    T: Send,
{
    /// Go back one point in the undo stack.
    /// If present on first edit do nothing
    fn undo(&mut self) -> &T;

    /// Go forward one point in the undo stack.
    /// If present on the last edit do nothing
    fn redo(&mut self) -> &T;

    /// Insert a new entry to the undo stack.
    /// NOTE: (IMP): If we have hit undo a few times then discard all the other values that come
    /// after the current point
    fn insert(&mut self, value: T);

    /// Reset the stack to the initial state
    fn reset(&mut self);

    /// Return the entry currently being pointed to
    fn current(&mut self) -> &T;

    /// List out all the entries on the undo stack
    /// Mostly used for debugging. Might remove this one
    fn edits<'a>(&'a self) -> Box<dyn Iterator<Item = &'a T> + 'a>;
}

#[derive(Debug, PartialEq, Eq)]
pub struct BasicEditStack<T> {
    internal_list: Vec<T>,
    index: usize,
}

impl<T> BasicEditStack<T> {
    pub fn new() -> Self
    where
        T: Default,
    {
        BasicEditStack {
            internal_list: vec![T::default()],
            index: 0,
        }
    }
}

impl<T> EditStack<T> for BasicEditStack<T>
where
    T: Default,
    T: Clone,
    T: Send,
{
    fn undo(&mut self) -> &T {
        self.index = if self.index == 0 { 0 } else { self.index - 1 };
        &self.internal_list[self.index]
    }

    fn redo(&mut self) -> &T {
        self.index = if self.index == self.internal_list.len() - 1 {
            self.index
        } else {
            self.index + 1
        };
        &self.internal_list[self.index]
    }

    fn insert(&mut self, value: T) {
        if self.index < self.internal_list.len() - 1 {
            self.internal_list.resize_with(self.index + 1, || {
                panic!("Impossible state reached: Bug in UndoStack logic")
            });
        }
        self.internal_list.push(value);
        self.index += 1;
    }

    fn reset(&mut self) {
        self.index = 0;
        self.internal_list = vec![T::default()];
    }

    fn edits<'a>(&'a self) -> Box<dyn Iterator<Item = &'a T> + 'a> {
        Box::new(self.internal_list.iter().take(self.index + 1))
    }

    fn current(&mut self) -> &T {
        &self.internal_list[self.index]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn edit_stack<T>(values: &[T], index: usize) -> BasicEditStack<T>
    where
        T: Clone,
    {
        BasicEditStack {
            internal_list: values.to_vec(),
            index,
        }
    }

    #[rstest]
    #[case(edit_stack(&[1, 2, 3][..], 2), 2)]
    #[case(edit_stack(&[1][..], 0), 1)]
    fn undo_works(#[case] stack: BasicEditStack<isize>, #[case] value_after_undo: isize) {
        let mut stack = stack;

        let value = stack.undo();
        assert_eq!(*value, value_after_undo);
    }

    #[rstest]
    #[case(edit_stack(&[1, 2, 3][..], 1), 3)]
    #[case(edit_stack(&[1][..], 0), 1)]
    fn redo_works(#[case] stack: BasicEditStack<isize>, #[case] value_after_undo: isize) {
        let mut stack = stack;

        let value = stack.redo();
        assert_eq!(*value, value_after_undo);
    }

    #[rstest]
    #[case(edit_stack(&[1, 2, 3][..], 1), 4, edit_stack(&[1, 2, 4], 2))]
    #[case(edit_stack(&[1, 2, 3][..], 2), 3, edit_stack(&[1, 2, 3, 3], 3))]
    fn insert_works(
        #[case] old_stack: BasicEditStack<isize>,
        #[case] value_to_insert: isize,
        #[case] expected_stack: BasicEditStack<isize>,
    ) {
        let mut stack = old_stack;

        stack.insert(value_to_insert);
        assert_eq!(stack, expected_stack);
    }
}
