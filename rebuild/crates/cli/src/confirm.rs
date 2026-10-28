//! Asking the person at the terminal before an operation that weakens a protection (R-35).
//!
//! Only when stdin and stderr are both terminals, and the answer is read from `/dev/tty`, not
//! from stdin. Without a terminal the answer is `Unavailable` and the operation is refused.
//! A program that drives a pseudo-terminal can still answer (03 §3.12): this stops agents
//! that merely run commands, not one that emulates a terminal.

use keyharbor_domain::ports::{Answer, Confirmation};
use std::io::{BufRead, BufReader, IsTerminal, Write};

pub struct Terminal;

impl Confirmation for Terminal {
    fn confirm(&self, action: &str) -> Answer {
        if !(std::io::stdin().is_terminal() && std::io::stderr().is_terminal()) {
            return Answer::Unavailable;
        }
        let Ok(mut tty) = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
        else {
            return Answer::Unavailable;
        };
        if write!(
            tty,
            "KeyHarbor: {action}\nThis weakens a protection. Type yes to go ahead: "
        )
        .and_then(|()| tty.flush())
        .is_err()
        {
            return Answer::Unavailable;
        }
        let mut line = String::new();
        match BufReader::new(tty).read_line(&mut line) {
            Ok(_) if line.trim() == "yes" => Answer::Yes,
            Ok(_) => Answer::No,
            Err(_) => Answer::Unavailable,
        }
    }
}
