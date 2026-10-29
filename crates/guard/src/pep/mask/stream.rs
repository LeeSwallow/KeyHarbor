//! Masking output that arrives in pieces (a long-running child's stdout, J-83). A value can
//! be split across two reads, so the last bytes that could still be the start of one are held
//! until more arrive, the stream pauses, or it ends. Nothing is emitted that would cut a value
//! in two.

use super::Masker;
use zeroize::Zeroizing;

pub struct StreamMasker<'a> {
    masker: &'a Masker,
    pending: Zeroizing<Vec<u8>>,
}

impl<'a> StreamMasker<'a> {
    pub fn new(masker: &'a Masker) -> Self {
        Self {
            masker,
            pending: Zeroizing::new(Vec::new()),
        }
    }

    /// Takes `bytes` and returns the masked output that is safe to show now.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let hold = self.masker.longest().saturating_sub(1);
        if self.pending.len() <= hold {
            return Vec::new();
        }
        let mut split = self.pending.len() - hold;
        // A value that starts before the cut ends inside the buffer (it is at most `longest`
        // long), so its match is final; never cut through it.
        if let Some(start) = self.masker.match_spanning(&self.pending, split) {
            split = start;
        }
        self.emit(split)
    }

    /// The stream paused: shows what is held, except a trailing piece that could be the
    /// start of a value.
    pub fn idle(&mut self) -> Vec<u8> {
        let keep = self.masker.value_start_at_end(&self.pending);
        self.emit(self.pending.len() - keep)
    }

    /// The stream ended: shows everything held, masked.
    pub fn flush(&mut self) -> Vec<u8> {
        self.emit(self.pending.len())
    }

    fn emit(&mut self, upto: usize) -> Vec<u8> {
        let out = self.masker.redact_bytes(&self.pending[..upto]);
        // Zeroizing the drained part: copy the rest down, then wipe the tail.
        let rest = self.pending.len() - upto;
        self.pending.copy_within(upto.., 0);
        zeroize::Zeroize::zeroize(&mut self.pending[rest..]);
        self.pending.truncate(rest);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyharbor_domain::vault::{FieldName, ItemName, PlaceholderRef};

    fn masker() -> Masker {
        Masker::build([(
            PlaceholderRef {
                item: ItemName::parse("api").unwrap(),
                field: FieldName::parse("key").unwrap(),
            },
            Zeroizing::new("sk-SECRETVALUE".to_owned()),
        )])
    }

    /// Feeds `pieces` one by one and returns all output, with or without a final flush.
    fn run(masker: &Masker, pieces: &[&str]) -> String {
        let mut stream = StreamMasker::new(masker);
        let mut out = Vec::new();
        for piece in pieces {
            out.extend(stream.push(piece.as_bytes()));
        }
        out.extend(stream.flush());
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn a_value_split_across_reads_is_still_masked() {
        let m = masker();
        let whole = "token=sk-SECRETVALUE done";
        for cut in 0..=whole.len() {
            let (a, b) = whole.split_at(cut);
            assert_eq!(
                run(&m, &[a, b]),
                "token={{secret:api.key}} done",
                "cut at {cut}"
            );
        }
        let bytes: Vec<String> = whole.chars().map(String::from).collect();
        let pieces: Vec<&str> = bytes.iter().map(String::as_str).collect();
        assert_eq!(run(&m, &pieces), "token={{secret:api.key}} done");
    }

    #[test]
    fn nothing_partial_is_shown_before_the_value_completes() {
        let m = masker();
        let mut stream = StreamMasker::new(&m);
        let shown = stream.push(b"a long line of output then sk-SECR");
        let shown = String::from_utf8(shown).unwrap();
        assert!(!shown.contains("sk-S"), "{shown:?}");
        assert!(shown.starts_with("a long line"), "{shown:?}");
        let rest = String::from_utf8(stream.push(b"ETVALUE!")).unwrap();
        let tail = String::from_utf8(stream.flush()).unwrap();
        assert_eq!(
            format!("{shown}{rest}{tail}"),
            "a long line of output then {{secret:api.key}}!"
        );
    }

    #[test]
    fn a_pause_inside_a_value_shows_nothing_of_it() {
        let m = masker();
        let mut stream = StreamMasker::new(&m);
        let mut shown = stream.push(b"key: sk-SEC");
        shown.extend(stream.idle());
        assert_eq!(String::from_utf8(shown.clone()).unwrap(), "key: ");
        shown.extend(stream.push(b"RETVALUE"));
        shown.extend(stream.idle());
        assert_eq!(String::from_utf8(shown).unwrap(), "key: {{secret:api.key}}");
        // A value written to its end shows as its placeholder at once.
        let mut stream = StreamMasker::new(&m);
        let mut shown = stream.push(b"sk-SECRETVALUE");
        shown.extend(stream.idle());
        assert_eq!(String::from_utf8(shown).unwrap(), "{{secret:api.key}}");
    }

    #[test]
    fn a_value_written_a_byte_at_a_time_with_pauses_never_shows() {
        let m = masker();
        let mut stream = StreamMasker::new(&m);
        let mut shown = stream.push(b"Ready> ");
        for byte in b"sk-SECRETVALUE" {
            shown.extend(stream.push(&[*byte]));
            shown.extend(stream.idle());
            let text = String::from_utf8(shown.clone()).unwrap();
            assert!(!text.contains("sk"), "{text:?}");
        }
        assert_eq!(
            String::from_utf8(shown).unwrap(),
            "Ready> {{secret:api.key}}"
        );
    }

    #[test]
    fn a_prompt_without_a_newline_shows_once_the_stream_pauses() {
        let m = masker();
        let mut stream = StreamMasker::new(&m);
        let early = stream.push(b"Password: ");
        assert!(early.is_empty());
        assert_eq!(stream.idle(), b"Password: ");
        assert_eq!(run(&Masker::empty(), &["no values"]), "no values");
    }
}
