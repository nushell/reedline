#[derive(Debug, PartialEq, Eq)]
pub struct EditStack<T> {
    internal_list: Vec<T>,
    index: usize,
}

impl<T> EditStack<T> {
    pub fn new() -> Self
    where
        T: Default,
    {
        EditStack {
            internal_list: vec![T::default()],
            index: 0,
        }
    }
}

impl<T> EditStack<T>
where
    T: Default + Clone + Send,
{
    /// Go back one point in the undo stack. If present on first edit do nothing
    pub(super) fn undo(&mut self) -> &T {
        self.index = if self.index == 0 { 0 } else { self.index - 1 };
        &self.internal_list[self.index]
    }

    /// Go forward one point in the undo stack. If present on the last edit do nothing
    pub(super) fn redo(&mut self) -> &T {
        self.index = if self.index == self.internal_list.len() - 1 {
            self.index
        } else {
            self.index + 1
        };
        &self.internal_list[self.index]
    }

    /// Insert a new entry to the undo stack.
    /// NOTE: (IMP): If we have hit undo a few times then discard all the other values that come
    /// after the current point
    pub(super) fn insert(&mut self, value: T) {
        if self.index < self.internal_list.len() - 1 {
            self.internal_list.resize_with(self.index + 1, || {
                panic!("Impossible state reached: Bug in UndoStack logic")
            });
        }
        self.internal_list.push(value);
        self.index += 1;
    }

    /// Reset the stack to the initial state
    pub(super) fn reset(&mut self) {
        self.index = 0;
        self.internal_list = vec![T::default()];
    }

    /// Return the entry currently being pointed to
    pub(super) fn current(&mut self) -> &T {
        &self.internal_list[self.index]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn edit_stack<T>(values: &[T], index: usize) -> EditStack<T>
    where
        T: Clone,
    {
        EditStack {
            internal_list: values.to_vec(),
            index,
        }
    }

    #[rstest]
    #[case(edit_stack(&[1, 2, 3][..], 2), 2)]
    #[case(edit_stack(&[1][..], 0), 1)]
    fn undo_works(#[case] stack: EditStack<isize>, #[case] value_after_undo: isize) {
        let mut stack = stack;

        let value = stack.undo();
        assert_eq!(*value, value_after_undo);
    }

    #[rstest]
    #[case(edit_stack(&[1, 2, 3][..], 1), 3)]
    #[case(edit_stack(&[1][..], 0), 1)]
    fn redo_works(#[case] stack: EditStack<isize>, #[case] value_after_undo: isize) {
        let mut stack = stack;

        let value = stack.redo();
        assert_eq!(*value, value_after_undo);
    }

    #[rstest]
    #[case(edit_stack(&[1, 2, 3][..], 1), 4, edit_stack(&[1, 2, 4], 2))]
    #[case(edit_stack(&[1, 2, 3][..], 2), 3, edit_stack(&[1, 2, 3, 3], 3))]
    fn insert_works(
        #[case] old_stack: EditStack<isize>,
        #[case] value_to_insert: isize,
        #[case] expected_stack: EditStack<isize>,
    ) {
        let mut stack = old_stack;

        stack.insert(value_to_insert);
        assert_eq!(stack, expected_stack);
    }
}
