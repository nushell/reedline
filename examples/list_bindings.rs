use reedline::{
    get_reedline_default_keybindings, get_reedline_keybinding_modifiers, get_reedline_keycodes,
    EditCommandDiscriminants, PromptEditModeDiscriminants, ReedlineEventDiscriminants,
};
use strum::IntoEnumIterator;

fn main() {
    get_all_keybinding_info();
    println!();
}

/// List all keybinding information
fn get_all_keybinding_info() {
    println!("--Key Modifiers--");
    for mods in get_reedline_keybinding_modifiers().iter() {
        println!("{mods}");
    }

    println!("\n--Modes--");
    for modes in PromptEditModeDiscriminants::iter() {
        println!("{modes:?}");
    }

    println!("\n--Key Codes--");
    for kcs in get_reedline_keycodes().iter() {
        println!("{kcs}");
    }

    println!("\n--Reedline Events--");
    for rle in ReedlineEventDiscriminants::iter() {
        println!("{rle:?}");
    }

    println!("\n--Edit Commands--");
    for edit in EditCommandDiscriminants::iter() {
        println!("{edit:?}");
    }

    println!("\n--Default Keybindings--");
    for (mode, modifier, code, event) in get_reedline_default_keybindings() {
        println!("mode: {mode}, keymodifiers: {modifier}, keycode: {code}, event: {event}");
    }
}
