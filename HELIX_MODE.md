# Helix Mode Testing Guide

## Quick Start

### Default: Explicit Mode Display
```bash
nix develop
cargo run --example helix_mode
```
Shows "[ NORMAL ] 〉" or "[ INSERT ] :" in the prompt.

### Alternative: Simple Icon Prompt
```bash
nix develop
cargo run --example helix_mode -- --simple-prompt
```
Shows only "〉" (normal) or ":" (insert) icons.

## Manual Test Sequence

1. **Start the example** - You'll be in NORMAL mode (Helix default)
   - You'll see: `[ NORMAL ] 〉`
2. **Try typing** - Nothing happens (normal mode doesn't insert text)
3. **Press `i`** - Enter INSERT mode at cursor
   - Prompt changes to: `[ INSERT ] :`
4. **Type "hello"** - Text should appear
5. **Press `Esc`** - Return to NORMAL mode
   - Prompt changes back to: `[ NORMAL ] 〉`
6. **Press `A`** (Shift+a) - Enter INSERT mode at line end
   - Prompt shows: `[ INSERT ] :`
7. **Type " world"** - Text appends at end
8. **Press `Enter`** - Submit the line
9. **See output** - "You entered: hello world"
10. **Press `Ctrl+D`** - Exit

## Implemented Keybindings

### Normal Mode (default)

**Insert mode entry:**
- `i` - Enter insert mode at cursor
- `a` - Enter insert mode after cursor  
- `I` (Shift+i) - Enter insert mode at line start
- `A` (Shift+a) - Enter insert mode at line end

**Character motions (extend selection):**
- `h` - Move left
- `l` - Move right

**Word motions (extend selection):**
- `w` - Next word start
- `b` - Previous word start
- `e` - Next word end

**Line motions (extend selection):**
- `0` - Line start
- `$` (Shift+4) - Line end

**Selection commands:**
- `x` - Select entire line
- `d` - Delete selection
- `c` - Change selection (delete and enter insert mode)
- `y` - Yank/copy selection
- `p` - Paste after cursor
- `P` (Shift+p) - Paste before cursor
- `;` - Collapse selection to cursor
- `Alt+;` - Swap cursor and anchor (flip selection direction)

**Other:**
- `Enter` - Accept/submit line
- `Ctrl+C` - Abort/exit
- `Ctrl+D` - Exit/EOF

### Insert Mode
- All printable characters - Insert text
- `Esc` - Return to normal mode (cursor moves left, vi-style)
- `Backspace` - Delete previous character
- `Enter` - Accept/submit line
- `Ctrl+C` - Abort/exit
- `Ctrl+D` - Exit/EOF

## Expected Behavior

### Normal Mode
- Cursor should be visible but typing regular keys does nothing
- Modal entry keys (i/a/I/A) switch to insert mode
- Prompt should indicate mode (implementation depends on prompt)

### Insert Mode  
- All text input works normally
- Esc returns to normal with cursor adjustment

## Differences from Vi Mode

| Feature | Vi Mode | Helix Mode |
|---------|---------|------------|
| Default mode | Insert | **Normal** |
| Insert entry | i/a/I/A/o/O | i/a/I/A (subset) |
| Esc behavior | Normal mode | Normal mode + cursor left |
| Philosophy | Command mode is special | Selection/motion first |

## Automated Tests

Run the test suite:
```bash
nix develop
cargo test --lib | grep helix
```

All 26 helix mode tests should pass:
- Mode entry/exit tests (7)
- Motion tests with selection (7)
- Selection command tests (8)
- Exit tests (4)

## Customizing Mode Display

Reedline provides native support for displaying the current mode through the `Prompt` trait.

### Built-in Options

1. **Explicit mode display** (default) - Shows "[ NORMAL ]" / "[ INSERT ]" with icon
2. **Simple icon prompt** - Shows only indicator icon (`:` for insert, `〉` for normal)

See `examples/helix_mode.rs` for both implementations with a command-line flag to toggle.

### Important Note About Right Prompt

The `render_prompt_right()` method does **not** receive the current `edit_mode`, so it cannot dynamically display mode changes. Only `render_prompt_indicator()` receives the mode parameter and updates in real-time.

### Example Mode Display

```rust
struct HelixModePrompt;

impl Prompt for HelixModePrompt {
    fn render_prompt_indicator(&self, edit_mode: PromptEditMode) -> Cow<'_, str> {
        match edit_mode {
            PromptEditMode::Vi(vi_mode) => match vi_mode {
                PromptViMode::Normal => Cow::Borrowed("[ NORMAL ] 〉"),
                PromptViMode::Insert => Cow::Borrowed("[ INSERT ] : "),
            },
            _ => Cow::Borrowed("> "),
        }
    }
    // ... other Prompt trait methods
}
```

This approach ensures the mode display updates immediately when you switch modes.

## Implemented Features

✅ **Basic motions with selection** - h/l, w/b/e, 0/$  
✅ **Selection commands** - x (select line), d (delete), c (change), ; (collapse), Alt+; (flip)  
✅ **Yank/paste** - y (copy), p/P (paste after/before)  
✅ **Insert mode entry** - i/a/I/A  
✅ **Mode switching** - Esc to normal, c to insert after change

## Known Limitations

Not yet implemented:
- Vertical motions (j/k for multi-line editing)
- Find/till motions (f/t)
- Counts and repeat (dot command)
- Text objects (iw, i", i(, etc.)
- Multi-cursor
- Undo/redo (u/U)
- Additional normal mode commands
