// Create a default reedline object to handle user input
// to run:
// cargo run --example external_printer --features=external_printer

use {
    reedline::ExternalPrinter,
    reedline::{DefaultPrompt, Reedline, Signal},
    std::thread,
    std::thread::sleep,
    std::time::Duration,
};

fn main() {
    let printer = ExternalPrinter::default();
    // make a clone to use it in a different thread
    let p_clone = printer.clone();
    // get the Sender<String> to have full sending control
    let p_sender = printer.sender();

    // external printer that prints a message every second
    thread::spawn(move || {
        let mut i = 1;
        loop {
            sleep(Duration::from_secs(1));
            assert!(p_clone
                .print(format!("Message {i} delivered.\nWith two lines!"))
                .is_ok());
            i += 1;
        }
    });

    // external printer that prints a bunch of messages after 3 seconds
    thread::spawn(move || {
        sleep(Duration::from_secs(3));
        for _ in 0..10 {
            sleep(Duration::from_millis(1));
            assert!(p_sender.send("Fast Hello !".to_string()).is_ok());
        }
    });

    let mut line_editor = Reedline::create().with_external_printer(printer);
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
