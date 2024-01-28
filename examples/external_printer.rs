// Create a default reedline object to handle user input
// to run:
// cargo run --example external_printer --features=external_printer

use {
    reedline::{DefaultPrompt, ExternalPrinterChannel, Reedline, Signal},
    std::thread,
    std::thread::sleep,
    std::time::Duration,
};

fn main() {
    // Create a channel for the external printer.
    let channel = ExternalPrinterChannel::default();

    // Get a clone of the channel sender to use in a separate thread.
    let printer = channel.sender();

    // Note that the senders/printers can also be cloned.
    // let printer_clone = printer.clone();

    // external printer that prints a message every second
    thread::spawn(move || {
        let mut i = 1;
        loop {
            sleep(Duration::from_secs(1));
            assert!(printer
                .print(format!("Message {i} delivered.\nWith two lines!"))
                .is_ok());
            i += 1;
        }
    });

    let printer = channel.sender();

    // external printer that prints a bunch of messages after 3 seconds
    thread::spawn(move || {
        sleep(Duration::from_secs(3));
        for _ in 0..10 {
            sleep(Duration::from_millis(1));
            assert!(printer.print("Hello!").is_ok());
        }
    });

    // create a `Reedline` struct and assign the channel for the external printer
    let mut line_editor = Reedline::create().with_external_printer(channel);
    let prompt = DefaultPrompt::default();

    loop {
        if let Ok(sig) = line_editor.read_line(&prompt) {
            match sig {
                Signal::Success(buffer) => {
                    println!("We processed: {buffer}");
                }
                Signal::CtrlD | Signal::CtrlC => {
                    println!("\nAborted!");
                    break;
                }
            }
            continue;
        }
        break;
    }
}
