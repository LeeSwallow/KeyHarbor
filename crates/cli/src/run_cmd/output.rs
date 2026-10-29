//! Showing the child's output as it streams, masked (J-83).

use keyharbor_guard::pep::mask::{Masker, StreamMasker};
use std::{
    io::{Read, Write},
    sync::mpsc,
    time::Duration,
};
use zeroize::Zeroizing;

/// How long output may sit unshown when the child stops writing mid-line (a prompt, a
/// progress bar). Fixed: short enough to feel immediate, long enough that one write is not
/// split by a pause between the reads of it.
const IDLE_FLUSH: Duration = Duration::from_millis(100);

/// Copies `input` to `output` through a stream masker: what is safe is shown at once, and
/// the rest when the child pauses or closes the stream (J-83).
pub(super) fn pump(input: impl Read + Send, mut output: impl Write, masker: &Masker) {
    let (chunks, received) = mpsc::channel::<Zeroizing<Vec<u8>>>();
    std::thread::scope(|scope| {
        scope.spawn(move || read_chunks(input, &chunks));
        let mut stream = StreamMasker::new(masker);
        loop {
            let shown = match received.recv_timeout(IDLE_FLUSH) {
                Ok(chunk) => stream.push(&chunk),
                Err(mpsc::RecvTimeoutError::Timeout) => stream.idle(),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = output
                        .write_all(&stream.flush())
                        .and_then(|()| output.flush());
                    return;
                }
            };
            if !shown.is_empty()
                && output
                    .write_all(&shown)
                    .and_then(|()| output.flush())
                    .is_err()
            {
                return;
            }
        }
    });
}

/// Sends what `input` gives, as it comes, until it ends.
fn read_chunks(mut input: impl Read, chunks: &mpsc::Sender<Zeroizing<Vec<u8>>>) {
    let mut buffer = Zeroizing::new(vec![0u8; 8192]);
    loop {
        match input.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if chunks.send(Zeroizing::new(buffer[..n].to_vec())).is_err() {
                    return;
                }
            }
        }
    }
}
