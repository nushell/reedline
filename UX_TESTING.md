# UX Test Checklist

As we currently don't have automated tests for the user facing terminal logic, we still have to check a few things manually.
This list does not try to cover every case but tries to catch the most likely breaking points or previous gotchas.
Exhaustiveness should be achieved by covering the components with appropriate unit tests.

## Do I have to perform all the manual tests?

Ideally we would validate the user experience for every PR but there are probably some good heuristics for when it is a *really good* idea to run through the manual checklist.

- Your PR changed the repaint logic.
- You changed how key presses are dispatched.
- You added a completely new component.
- The component you changed is not covered by tests, that uphold a contract for the I/O facing engine.
- You did a large refactoring touching several components at once.

## Configuration

To catch potential index overflows etc. running the example binary in debug mode via `cargo run` can be helpful. Yet in some cases the experience might be better/smoother when running the actual release build via `cargo run --release`. This is especially true for resizing. If the slower execution in debug mode causes noticeable issues report them with the checklist.

> Copy the checklist below, as part of your PR finalization

## Manual checks

Relevant features tested (leave open if you did not consider those areas touched by your PR):

- [ ] core editing and default Emacs keybindings
- [ ] history
- [ ] syntax highlighting
- [ ] completion/hinting
- [ ] vi mode

### Info

Build: [ ] debug / [ ] release

Platform:

Terminal emulator:

Inside a [ ] ssh,[ ] tmux or [ ] screen session?

### Basics

- [ ] Typing of a short line containing both upper- and lowercase characters.
- [ ] Movement left/right using the arrow keys
- [ ] Word to the left with `Ctrl-b` or `Ctrl-Left`, Word to the right with `Ctrl-f`
- [ ] `Enter` to complete entry

#### Clearing

- [ ] Type something and abort the entry with `Ctrl-c`, you should end up on an empty prompt below.
- [ ] Type something and press `Ctrl-l` to clear the screen. Your current entry should still be there and passed through when pressing `Enter`

#### Unicode and Emojis

- [ ] Paste the line `Emoji test 😊 checks 🤦🏼‍♂️ unicode` and move the cursor over the emojis.
- [ ] Are you able to delete the smiley?
- [ ] `Home`/`End` at accurate positions
- [ ] Check that the emoji containing line can be entered

## History

- [ ] On the empty line press the `up-arrow` key to see if you can recall the previous entry
- [ ] Press `Enter` to execute this line (it should *not* be duplicated in the history, after checking leave history recall by `down-arrow`)
- [ ] On an empty line start typing the beginning of a line in the history. Hit the `up-arrow` to find the matching entry.
- [ ] After that run `Ctrl-r` to start traditional reverse search. Type your initial search. Can you find more hits by pressing `Ctrl-r` or `up-arrow`?
- [ ] Abort this search by pressing `Ctrl-c`

## Syntax highlighting

- [ ] Upon entering `test`, this word is highlighted differently.

## Completion

**TODO:** *define desired behavior*

### VI mode

**TODO:** *define basic set to test*
