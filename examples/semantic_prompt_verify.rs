// Semantic prompt verification example
// Run with: cargo run --example semantic_prompt_verify
//
// This example shows the raw escape sequences that would be emitted,
// making them visible for verification without needing a compatible terminal.

use reedline::{Osc133Markers, Osc633Markers, PromptKind, SemanticPromptMarkers};

fn main() {
    println!("Semantic Prompt Verification");
    println!("============================");
    println!();

    // Demonstrate OSC 133 markers
    println!("OSC 133 Markers (Standard):");
    println!("----------------------------");
    let osc133 = Osc133Markers;
    show_markers(&osc133, "133");
    println!();

    // Demonstrate OSC 633 markers (VS Code)
    println!("OSC 633 Markers (VS Code):");
    println!("--------------------------");
    let osc633 = Osc633Markers;
    show_markers(&osc633, "633");
    println!();

    // Show expected sequence
    println!("Expected Prompt Sequence: ");
    println!("------------------------");
    println!("For a prompt like: '~/src > ls -la'");
    println!();
    println!("1. prompt_start(Primary)  -> OSC 133;A;k=i ST  (before '~/src ')");
    println!("2. [left prompt text]     -> '~/src '");
    println!("3. [indicator text]       -> '> '");
    println!("4. command_input_start()  -> OSC 133;B ST     (after indicator)");
    println!("5. prompt_start(Right)    -> OSC 133;P;k=r ST (before right prompt)");
    println!("6. [right prompt text]    -> any right prompt");
    println!();

    // Show multiline case
    println!("Multiline Continuation:");
    println!("----------------------");
    println!("For a multiline prompt continuation:");
    println!();
    println!("1. prompt_start(Secondary) -> OSC 133;A;k=s ST (before '::: ')");
    println!("2. [continuation indicator] -> '::: '");
    println!("3. command_input_start()   -> OSC 133;B ST    (after indicator)");
    println!();

    // Show actual bytes
    println!("Raw Byte Sequences:");
    println!("------------------");
    print_raw(
        "OSC 133;A;k=i ST",
        &osc133.prompt_start(PromptKind::Primary),
    );
    print_raw(
        "OSC 133;A;k=s ST",
        &osc133.prompt_start(PromptKind::Secondary),
    );
    print_raw("OSC 133;P;k=r ST", &osc133.prompt_start(PromptKind::Right));
    print_raw("OSC 133;B ST    ", &osc133.command_input_start());
    println!();
    println!("Note: C (pre-exec) and D (post-exec) markers are emitted by the shell,");
    println!("not by reedline, as they relate to command execution lifecycle.");
}

fn show_markers(markers: &dyn SemanticPromptMarkers, prefix: &str) {
    println!(
        "  Primary prompt start:   OSC {prefix};A;k=i ST = {}",
        escape_for_display(&markers.prompt_start(PromptKind::Primary))
    );
    println!(
        "  Secondary prompt start: OSC {prefix};A;k=s ST = {}",
        escape_for_display(&markers.prompt_start(PromptKind::Secondary))
    );
    println!(
        "  Right prompt start:     OSC {prefix};P;k=r ST = {}",
        escape_for_display(&markers.prompt_start(PromptKind::Right))
    );
    println!(
        "  Command input start:    OSC {prefix};B ST     = {}",
        escape_for_display(&markers.command_input_start())
    );
    // Note: C (command_executed) and D (command_finished) markers are handled by the shell,
    // not by reedline, as they relate to command execution lifecycle.
}

fn escape_for_display(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\x1b' => "\\e".to_string(),
            '\\' => "\\".to_string(),
            c if c.is_control() => format!("\\x{:02x}", c as u8),
            c => c.to_string(),
        })
        .collect()
}

fn print_raw(label: &str, s: &str) {
    print!("  {label} = ");
    for byte in s.bytes() {
        if byte == 0x1b {
            print!("ESC ");
        } else if byte == 0x5c {
            print!("\\\\ ");
        } else if byte < 0x20 {
            print!("0x{byte:02x} ");
        } else {
            print!("{} ", byte as char);
        }
    }
    println!();
}
