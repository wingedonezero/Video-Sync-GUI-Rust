// src/main.rs

// The `vsg_rust` here refers to the library crate defined in `src/lib.rs`.
use vsg_rust::run;

fn main() -> iced::Result {
    // Call the public run function from our library.
    run()
}
