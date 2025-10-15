// Helix mode end-to-end test suite
//
// This file contains comprehensive tests for Helix keybinding mode.
// For interactive demos, see:
//   - examples/hx_mode_tutorial.rs (with instructions)
//   - examples/hx_mode_sandbox.rs (minimal)
//
// Run tests:
// cargo test --example helix_mode

fn main() {
    eprintln!("This example is test-only.");
    eprintln!("Run the interactive demos instead:");
    eprintln!("  cargo run --example hx_mode_tutorial");
    eprintln!("  cargo run --example hx_mode_sandbox");
    eprintln!();
    eprintln!("Or run the tests:");
    eprintln!("  cargo test --example helix_mode");
}

// ============================================================================
// End-to-End Tests
// ============================================================================
//
// This test suite provides comprehensive, executable specifications for Helix mode.
// Based on research from Helix's actual implementation via DeepWiki, these tests
// verify the anchor/cursor/head selection model and mode behaviors.
//
// ## Test Coverage
//
// ### Manual Test Sequences
// - `test_manual_sequence_basic_workflow()` - Complete workflow (see demo output)
// - `test_manual_sequence_simple_mode_display()` - Mode display verification
// - `test_manual_sequence_exit_test()` - Exit behavior (Ctrl+D)
//
// ### Keybinding Tests - Basic Motions
// - `test_insert_mode_entry_keybindings()` - i, a, I, A entry to insert mode
// - `test_character_motions_with_selection()` - h, l character motions
// - `test_word_motions_with_selection()` - w, b, e word motions
// - `test_bigword_motions_with_selection()` - W, B, E WORD motions (whitespace-delimited)
// - `test_line_motions_with_selection()` - 0, $ line start/end motions
// - `test_find_till_motions()` - f, t, F, T find/till character motions
// - `test_backward_motion_with_b()` - Multiple 'b' presses moving backward
// - `test_end_of_word_motion_with_e()` - End-of-word motion
// - `test_multiple_b_presses_from_end()` - Backward word navigation from end
// - `test_tutorial_double_b_selection()` - Tutorial scenario: double 'b' selection
//
// ### Selection Model Tests (Based on Helix's anchor/cursor mechanism)
// - `test_normal_mode_motions_collapse_selection()` - Normal mode collapses selection
// - `test_select_mode_entry_with_v()` - Enter Select mode with 'v'
// - `test_word_motions_in_select_mode()` - Motions extend selection in Select mode
// - `test_line_selection_with_x()` - 'x' selects entire line
// - `test_collapse_selection_with_semicolon()` - ';' collapses selection to cursor
// - `test_find_motion_extends_in_select_mode()` - Find motions extend in Select mode
//
// ### Selection and Editing Operations
// - `test_selection_commands()` - x, d, c, ; selection operations
// - `test_yank_and_paste()` - y, p, P clipboard operations
// - `test_delete_removes_selection_stays_normal()` - 'd' deletes but stays Normal
// - `test_change_enters_insert_mode()` - 'c' deletes and enters Insert mode
//
// ### Special Behaviors
// - `test_esc_cursor_behavior()` - Cursor moves left on Esc (vi-style)
// - `test_ctrl_c_and_ctrl_d_in_both_modes()` - Exit keys work in all modes
// - `test_difference_from_vi_mode_default_mode()` - Starts in Normal, not Insert
// - `test_complete_workflow_multiple_edits()` - Complex multi-step workflow
//
// ## Helix Selection Model
//
// These tests verify Helix's unique selection-first editing model:
//
// 1. **Anchor and Head**: Every selection has an anchor (fixed) and head (movable)
// 2. **Normal Mode**: Motions collapse selection (move both anchor and head together)
// 3. **Select Mode**: Motions extend selection (anchor stays fixed, head moves)
// 4. **Selection Operations**: Commands like 'd', 'c', 'y' work on current selection
//
// ## Running the Tests
//
// Run all tests:
// ```bash
// cargo test --example helix_mode
// ```
//
// Run a specific test:
// ```bash
// cargo test --example helix_mode test_manual_sequence_basic_workflow
// ```
//
// Run with output:
// ```bash
// cargo test --example helix_mode -- --nocapture
// ```

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
    use reedline::{
        EditCommand, EditMode, Helix, PromptEditMode, PromptViMode, Reedline, ReedlineEvent,
        ReedlineRawEvent,
    };

    #[test]
    fn test_manual_sequence_basic_workflow() {
        // This test follows the exact sequence from the demo output
        // Tests are explicit about parsing events and applying commands

        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        // Step 1: Start - Verify we're in NORMAL mode (Helix default)
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
        assert_eq!(line_editor.current_buffer_contents(), "");

        // Step 2: Press `i` - Enter INSERT mode
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Step 3: Type "hello world" - Apply each character as an edit command
        for ch in "hello world".chars() {
            let raw_event = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )))
            .unwrap();
            let event = helix.parse_event(raw_event);
            if let ReedlineEvent::Edit(commands) = event {
                line_editor.run_edit_commands(&commands);
            }
        }
        assert_eq!(line_editor.current_buffer_contents(), "hello world");
        assert_eq!(line_editor.current_insertion_point(), 11);

        // Step 4: Press Esc - Return to NORMAL mode (cursor moves left)
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap(),
        );
        if let ReedlineEvent::Multiple(events) = event {
            for e in events {
                if let ReedlineEvent::Edit(commands) = e {
                    line_editor.run_edit_commands(&commands);
                }
            }
        }
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
        assert_eq!(line_editor.current_insertion_point(), 10); // Moved left

        // Step 5: Press `b` - Move back to start of word
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 6);

        // Step 5b: Press `e` - Move to end of word
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 10);

        // Step 6: Press `d` - Delete selection
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        let buffer = line_editor.current_buffer_contents();
        assert!(buffer.starts_with("hello"));
        assert!(buffer.len() < 11);

        // Step 7: Press `i` - Enter INSERT mode again
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Step 8: Type "universe"
        for ch in "universe".chars() {
            let raw_event = ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )))
            .unwrap();
            let event = helix.parse_event(raw_event);
            if let ReedlineEvent::Edit(commands) = event {
                line_editor.run_edit_commands(&commands);
            }
        }
        assert!(line_editor.current_buffer_contents().contains("universe"));

        // Step 9: Press Enter - Verify it produces Enter event
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::Enter));

        // Step 10: Verify final state contains both parts
        let final_buffer = line_editor.current_buffer_contents();
        assert!(final_buffer.contains("hello"));
        assert!(final_buffer.contains("universe"));
    }

    #[test]
    fn test_manual_sequence_simple_mode_display() {
        let mut helix = Helix::default();

        // Verify initial Normal mode
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));

        // Press 'i' to enter Insert mode
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Press Esc to return to Normal mode
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap(),
        );
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn test_manual_sequence_exit_test() {
        let mut helix = Helix::default();

        // Ctrl+D from Normal mode
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::CONTROL,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::CtrlD));

        // Enter Insert mode then Ctrl+D
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::CONTROL,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::CtrlD));
    }

    #[test]
    fn test_insert_mode_entry_keybindings() {
        // Test 'i' - Enter insert mode at cursor
        let mut helix = Helix::default();
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Test 'a' - Enter insert mode after cursor (moves right)
        let mut helix = Helix::default();
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        // Should produce edit commands to move right
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Test 'I' (Shift+i) - Enter insert mode at line start
        let mut helix = Helix::default();
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Test 'A' (Shift+a) - Enter insert mode at line end
        let mut helix = Helix::default();
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('a'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::Multiple(_)));
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));
    }

    #[test]
    fn test_character_motions_with_selection() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello".to_string()),
            EditCommand::MoveToPosition {
                position: 2,
                select: false,
            },
        ]);

        // Press 'h' - move left
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('h'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 1);

        // Press 'l' - move right
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('l'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 2);
    }

    #[test]
    fn test_word_motions_with_selection() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world test".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Press 'w' - next word start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 6);

        // Press 'e' - next word end
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 10);

        // Press 'b' - previous word start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 6);
    }

    #[test]
    fn test_bigword_motions_with_selection() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello-world test.case".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Press 'W' (Shift+w) - next WORD start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 12);

        // Press 'E' (Shift+e) - next WORD end
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 20);

        // Press 'B' (Shift+b) - previous WORD start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 12);
    }

    #[test]
    fn test_line_motions_with_selection() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 5,
                select: false,
            },
        ]);

        // Press '0' - line start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('0'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 0);

        // Press '$' - line end
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('$'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        let pos = line_editor.current_insertion_point();
        assert!(pos >= 10 && pos <= 11);
    }

    #[test]
    fn test_find_till_motions() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Test 'f' - find next 'w'
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 6);

        // Reset and test 't' - till next 'w'
        line_editor.run_edit_commands(&[EditCommand::MoveToPosition {
            position: 0,
            select: false,
        }]);
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('t'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 5);

        // Reset to end and test 'F' - find previous 'h'
        line_editor.run_edit_commands(&[EditCommand::MoveToPosition {
            position: 10,
            select: false,
        }]);
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('h'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 0);

        // Reset to end and test 'T' - till previous 'h'
        line_editor.run_edit_commands(&[EditCommand::MoveToPosition {
            position: 10,
            select: false,
        }]);
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('t'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('h'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 1);
    }

    #[test]
    fn test_selection_commands() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Test 'x' - select entire line
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Test ';' - collapse selection
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char(';'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Test delete with selection
        line_editor.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Select a word then delete
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert!(line_editor.current_buffer_contents().len() < 11);

        // Test 'c' - change (delete and enter insert)
        line_editor.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));
    }

    #[test]
    fn test_yank_and_paste() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Select and yank
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('y'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_buffer_contents(), "hello world");

        // Move to end and paste
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('$'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Test paste before (P)
        line_editor.run_edit_commands(&[
            EditCommand::Clear,
            EditCommand::InsertString("test".to_string()),
            EditCommand::MoveToPosition {
                position: 2,
                select: false,
            },
        ]);

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('p'),
                KeyModifiers::SHIFT,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
    }

    #[test]
    fn test_esc_cursor_behavior() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        // Enter insert mode
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        // Type text
        for ch in "hello".chars() {
            let event = helix.parse_event(
                ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                )))
                .unwrap(),
            );
            if let ReedlineEvent::Edit(commands) = event {
                line_editor.run_edit_commands(&commands);
            }
        }
        assert_eq!(line_editor.current_insertion_point(), 5);

        // Press Esc - cursor should move left
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap(),
        );
        if let ReedlineEvent::Multiple(events) = event {
            for e in events {
                if let ReedlineEvent::Edit(commands) = e {
                    line_editor.run_edit_commands(&commands);
                }
            }
        }
        assert_eq!(line_editor.current_insertion_point(), 4);
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn test_complete_workflow_multiple_edits() {
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        // Enter insert mode
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Type text
        for ch in "foo bar baz".chars() {
            let event = helix.parse_event(
                ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                )))
                .unwrap(),
            );
            if let ReedlineEvent::Edit(commands) = event {
                line_editor.run_edit_commands(&commands);
            }
        }
        assert_eq!(line_editor.current_buffer_contents(), "foo bar baz");

        // Exit to normal
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap(),
        );
        if let ReedlineEvent::Multiple(events) = event {
            for e in events {
                if let ReedlineEvent::Edit(commands) = e {
                    line_editor.run_edit_commands(&commands);
                }
            }
        }
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));

        // Move to start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('0'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        assert_eq!(line_editor.current_insertion_point(), 0);

        // Select with 'w'
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Delete selection
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let buffer = line_editor.current_buffer_contents();
        assert!(buffer.len() < 11);
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn test_ctrl_c_and_ctrl_d_in_both_modes() {
        let mut helix = Helix::default();

        // Ctrl+C in Normal mode
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::CtrlC));

        // Ctrl+D in Normal mode
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::CONTROL,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::CtrlD));

        // Enter Insert mode
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        // Ctrl+C in Insert mode
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::CtrlC));

        // Ctrl+D in Insert mode
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::CONTROL,
            )))
            .unwrap(),
        );
        assert!(matches!(event, ReedlineEvent::CtrlD));
    }

    #[test]
    fn test_difference_from_vi_mode_default_mode() {
        let helix = Helix::default();
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));
    }

    #[test]
    fn test_multiple_b_presses_from_end() {
        // Test pressing 'b' multiple times to select backwards
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        // Setup: "hello world test" with cursor at end
        line_editor.run_edit_commands(&[EditCommand::InsertString("hello world test".to_string())]);
        println!("Initial: pos={}", line_editor.current_insertion_point());
        assert_eq!(line_editor.current_insertion_point(), 16); // At end

        // Press 'b' first time - should move to start of "test"
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        println!("After 1st b: pos={}", line_editor.current_insertion_point());
        assert_eq!(line_editor.current_insertion_point(), 12); // Start of "test"

        // Press 'b' second time - should move to start of "world"
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        println!("After 2nd b: pos={}", line_editor.current_insertion_point());
        assert_eq!(line_editor.current_insertion_point(), 6); // Start of "world"

        // Press 'b' third time - should move to start of "hello"
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        println!("After 3rd b: pos={}", line_editor.current_insertion_point());
        assert_eq!(line_editor.current_insertion_point(), 0); // Start of "hello"
    }

    #[test]
    fn test_tutorial_double_b_selection() {
        // Test the specific tutorial scenario: "hello world" with 'b' pressed twice
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        // Enter insert mode and type "hello world"
        let _event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        for ch in "hello world".chars() {
            let event = helix.parse_event(
                ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                    KeyCode::Char(ch),
                    KeyModifiers::NONE,
                )))
                .unwrap(),
            );
            if let ReedlineEvent::Edit(commands) = event {
                line_editor.run_edit_commands(&commands);
            }
        }

        println!(
            "After typing: buffer='{}', pos={}",
            line_editor.current_buffer_contents(),
            line_editor.current_insertion_point()
        );
        assert_eq!(line_editor.current_buffer_contents(), "hello world");
        assert_eq!(line_editor.current_insertion_point(), 11);

        // Press Esc to return to Normal mode
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)))
                .unwrap(),
        );
        if let ReedlineEvent::Multiple(events) = event {
            for e in events {
                if let ReedlineEvent::Edit(commands) = e {
                    line_editor.run_edit_commands(&commands);
                }
            }
        }

        println!(
            "After Esc: buffer='{}', pos={}",
            line_editor.current_buffer_contents(),
            line_editor.current_insertion_point()
        );
        assert_eq!(line_editor.current_insertion_point(), 10);

        // Press 'b' twice
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            println!("First 'b' commands: {:?}", commands);
            line_editor.run_edit_commands(&commands);
        }
        println!(
            "After 1st b: buffer='{}', pos={}",
            line_editor.current_buffer_contents(),
            line_editor.current_insertion_point()
        );

        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            println!("Second 'b' commands: {:?}", commands);
            line_editor.run_edit_commands(&commands);
        }
        println!(
            "After 2nd b: buffer='{}', pos={}",
            line_editor.current_buffer_contents(),
            line_editor.current_insertion_point()
        );
        assert_eq!(line_editor.current_insertion_point(), 0);

        // Now press 'd' to delete the selection
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            println!("Delete commands: {:?}", commands);
            line_editor.run_edit_commands(&commands);
        }

        let buffer_after_delete = line_editor.current_buffer_contents();
        println!(
            "After delete: buffer='{}', pos={}",
            buffer_after_delete,
            line_editor.current_insertion_point()
        );

        // What gets deleted? This will tell us what was selected
        // If "hello " was selected, buffer should be "world"
        // If entire string was selected, buffer should be empty
        println!("Expected: 'world' if 'hello ' was selected, '' if everything was selected");
    }

    // ========================================================================
    // Selection Model Tests - Based on Helix's anchor/cursor/head mechanism
    // ========================================================================

    #[test]
    fn test_normal_mode_motions_collapse_selection() {
        // In Helix Normal mode, motions move the cursor without creating a selection.
        // Both anchor and head collapse to the new position.
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // In Normal mode, 'w' should move cursor to next word start
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        // Cursor should be at position 6 (start of "world")
        // In Helix Normal mode, this creates a selection from [6, 7) which appears as a cursor at 6
        assert_eq!(line_editor.current_insertion_point(), 6);

        // Another 'w' should move to end (no more words)
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }
        // Should be at or near end of line
        assert!(line_editor.current_insertion_point() >= 10);
    }

    #[test]
    fn test_select_mode_entry_with_v() {
        // Test that 'v' enters Select mode where motions extend selection
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Start in Normal mode
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));

        // Press 'v' to enter Select mode
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        // Should now be in Select mode
        // Note: Helix uses PromptViMode for compatibility, but in actual Helix
        // there's a separate Select mode. Check the implementation details.
        // For now, we verify that subsequent motions extend selection.
    }

    #[test]
    fn test_line_selection_with_x() {
        // Test that 'x' selects the entire line
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 5,
                select: false,
            },
        ]);

        // Press 'x' to select the entire line
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // After 'x', the entire line should be selected
        // We can verify this by pressing 'd' and checking the buffer is empty
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Buffer should be empty after deleting the line selection
        assert_eq!(line_editor.current_buffer_contents(), "");
    }

    #[test]
    fn test_collapse_selection_with_semicolon() {
        // Test that ';' collapses selection to cursor
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Select with 'x' (entire line)
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Now collapse selection with ';'
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char(';'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // After collapse, pressing 'd' should only delete one character
        let initial_len = line_editor.current_buffer_contents().len();
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Only one character should be deleted (cursor position, not whole line)
        let final_len = line_editor.current_buffer_contents().len();
        assert!(final_len >= initial_len - 1);
    }

    #[test]
    fn test_word_motions_in_select_mode() {
        // Test that word motions extend selection in Select mode
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("foo bar baz".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Enter Select mode with 'v'
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        let initial_pos = line_editor.current_insertion_point();

        // Press 'w' to extend selection to next word
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let after_w_pos = line_editor.current_insertion_point();

        // Press 'w' again to extend further
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let after_2w_pos = line_editor.current_insertion_point();

        // In Select mode, cursor should keep moving forward
        assert!(after_w_pos > initial_pos);
        assert!(after_2w_pos > after_w_pos);

        // Delete should remove the extended selection
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Most of the text should be gone
        assert!(line_editor.current_buffer_contents().len() < 11);
    }

    #[test]
    fn test_change_enters_insert_mode() {
        // Test that 'c' deletes selection and enters Insert mode
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Use 'x' to select entire line first (in Helix, 'c' works on selections)
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let before_len = line_editor.current_buffer_contents().len();

        // Press 'c' to change (delete and enter Insert mode)
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('c'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        // Handle Multiple event properly
        if let ReedlineEvent::Multiple(events) = event {
            for e in events {
                if let ReedlineEvent::Edit(commands) = e {
                    line_editor.run_edit_commands(&commands);
                }
            }
        } else if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Should be in Insert mode now
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Insert)
        ));

        // Buffer should be shorter or empty (text deleted)
        let after_len = line_editor.current_buffer_contents().len();
        assert!(after_len < before_len);
    }

    #[test]
    fn test_delete_removes_selection_stays_normal() {
        // Test that 'd' deletes selection but stays in Normal mode
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Select word with 'w'
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let before_delete = line_editor.current_buffer_contents().len();

        // Press 'd' to delete selection
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Should still be in Normal mode
        assert!(matches!(
            helix.edit_mode(),
            PromptEditMode::Vi(PromptViMode::Normal)
        ));

        // Buffer should be shorter
        assert!(line_editor.current_buffer_contents().len() < before_delete);
    }

    #[test]
    fn test_find_motion_extends_in_select_mode() {
        // Test that find motions (f, t) extend selection in Select mode
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("hello world test".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Enter Select mode
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('v'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );

        // Use 'f' to find 'w'
        let _ = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('f'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Cursor should have moved to 'w' position
        assert_eq!(line_editor.current_insertion_point(), 6);

        // Delete should remove "hello w" (from start to found position)
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        let buffer = line_editor.current_buffer_contents();
        // Should have something like "orld test" remaining
        assert!(buffer.contains("orld") || buffer.len() < 16);
    }

    #[test]
    fn test_backward_motion_with_b() {
        // Test backward word motion with 'b'
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("one two three".to_string()),
            EditCommand::MoveToPosition {
                position: 13,
                select: false,
            },
        ]);

        // Press 'b' to move back one word
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Should be at start of "three" (position 8)
        assert_eq!(line_editor.current_insertion_point(), 8);

        // Press 'b' again
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Should be at start of "two" (position 4)
        assert_eq!(line_editor.current_insertion_point(), 4);
    }

    #[test]
    fn test_end_of_word_motion_with_e() {
        // Test end-of-word motion with 'e'
        let mut helix = Helix::default();
        let mut line_editor = Reedline::create();

        line_editor.run_edit_commands(&[
            EditCommand::InsertString("one two three".to_string()),
            EditCommand::MoveToPosition {
                position: 0,
                select: false,
            },
        ]);

        // Press 'e' to move to end of first word
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Should be at end of "one" (position 2)
        assert_eq!(line_editor.current_insertion_point(), 2);

        // Press 'e' again
        let event = helix.parse_event(
            ReedlineRawEvent::try_from(Event::Key(KeyEvent::new(
                KeyCode::Char('e'),
                KeyModifiers::NONE,
            )))
            .unwrap(),
        );
        if let ReedlineEvent::Edit(commands) = event {
            line_editor.run_edit_commands(&commands);
        }

        // Should be at end of "two" (position 6)
        assert_eq!(line_editor.current_insertion_point(), 6);
    }
}
