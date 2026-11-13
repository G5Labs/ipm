//! Secret redaction across streaming boundaries.
//!
//! Every configured secret *value* is redacted regardless of which key carried
//! it (item 11): the workspace token, the listener token, any grant material,
//! and any value the control plane marks secret. Module stdout/stderr arrive in
//! arbitrary chunks, so a secret may straddle two reads. The [`Redactor`] holds
//! back a tail shorter than the longest secret until the next chunk (or a final
//! flush) proves it cannot be a prefix, so a split secret is still masked.

pub const MASK: &str = "[REDACTED]";

/// How the next text fragment relates to an unresolved secret prefix carried
/// from the fragments before it.
/// One pass of the scanner over a buffer.
struct Scan {
    /// Masked byte spans within the consumed prefix.
    spans: Vec<(usize, usize)>,
    /// Bytes of the buffer that are resolved; the rest is held.
    consumed: usize,
    /// Leading held bytes that a cut span had already covered.
    covered: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Continuation {
    /// Byte length of the head that completes a carried prefix (0: none).
    pub completed: usize,
    /// Whether the whole head merely extends a carried prefix.
    pub extends: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixContinuation {
    /// The carried prefix plus this many leading bytes of the fragment is a
    /// whole secret.
    Completed(usize),
    /// The whole fragment extends the prefix without completing it.
    Extends,
    /// The fragment does not continue the prefix.
    Broken,
}

#[derive(Clone, Debug)]
pub struct Secrets {
    /// Sorted longest-first so an enclosing secret masks before a substring.
    values: Vec<Vec<u8>>,
    longest: usize,
}

impl Secrets {
    pub fn new(values: impl IntoIterator<Item = String>) -> Self {
        // Every nonempty configured value is a secret (Agent API §12.1
        // names short values split across frames); the platform never
        // configures trivially short secrets, so no floor is applied.
        let mut values: Vec<Vec<u8>> = values
            .into_iter()
            // Native provider auth may trim a configured key before emitting
            // diagnostics. Mask that spelling in stdout and stderr too.
            .flat_map(|value| {
                let trimmed = (value.trim() != value).then(|| value.trim().to_string());
                std::iter::once(value).chain(trimmed)
            })
            .filter(|value| !value.is_empty())
            .map(String::into_bytes)
            .collect();
        values.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        values.dedup();
        let longest = values.iter().map(Vec::len).max().unwrap_or(0);
        Self { values, longest }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The same set plus one more configured value.
    pub fn with(&self, value: &str) -> Self {
        let mut values: Vec<String> = self
            .values
            .iter()
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .collect();
        values.push(value.to_owned());
        Self::new(values)
    }

    /// Redact a complete buffer.
    pub fn redact_bytes(&self, input: &[u8]) -> Vec<u8> {
        self.scan(input, true).0
    }

    /// The byte spans of `input` that complete secrets occupy, overlapping
    /// and adjacent-at-a-start matches merged into one span (with `abc` and
    /// `bcTAIL`, `abcTAIL` is one span), longest match first at every start.
    /// When `at_end` is false the scan stops at the first byte that begins a
    /// *proper prefix* of a secret running to the end of the buffer and
    /// reports how many bytes were consumed; the caller holds the remainder
    /// until more bytes arrive. Such a byte inside a span under construction
    /// cuts the span there — the bytes before it are already covered by
    /// complete matches — and `covered` reports how many of the held bytes
    /// the span had already covered, so a self-overlapping secret (`aa` over
    /// a run of `a`s) retains at most `longest - 1` bytes and never rescans.
    fn scan_spans(&self, input: &[u8], at_end: bool) -> Scan {
        let mut scan = Scan {
            spans: Vec::new(),
            consumed: input.len(),
            covered: 0,
        };
        if self.values.is_empty() {
            return scan;
        }
        let pending = |pos: usize| {
            let rest = &input[pos..];
            !at_end
                && rest.len() < self.longest
                && self
                    .values
                    .iter()
                    .any(|secret| secret.len() > rest.len() && secret.starts_with(rest))
        };
        let longest_at = |pos: usize| {
            self.values
                .iter()
                .filter(|secret| input[pos..].starts_with(secret))
                .map(|secret| pos + secret.len())
                .max()
        };
        let mut index = 0;
        while index < input.len() {
            if pending(index) {
                scan.consumed = index;
                return scan;
            }
            let Some(mut end) = longest_at(index) else {
                index += 1;
                continue;
            };
            let mut start = index + 1;
            while start < end {
                if pending(start) {
                    scan.spans.push((index, start));
                    scan.consumed = start;
                    scan.covered = end - start;
                    return scan;
                }
                if let Some(further) = longest_at(start) {
                    end = end.max(further);
                }
                start += 1;
            }
            scan.spans.push((index, end));
            index = end;
        }
        scan
    }

    /// Scan `input`, masking every complete secret (see `scan_spans`).
    fn scan(&self, input: &[u8], at_end: bool) -> (Vec<u8>, usize) {
        let scan = self.scan_spans(input, at_end);
        let mut out = Vec::with_capacity(scan.consumed);
        let mut position = 0;
        for (start, end) in scan.spans {
            out.extend_from_slice(&input[position..start]);
            out.extend_from_slice(MASK.as_bytes());
            position = end;
        }
        out.extend_from_slice(&input[position..scan.consumed]);
        (out, scan.consumed)
    }

    /// The byte spans of `text` that complete secrets occupy (see
    /// `scan_spans`), on character boundaries.
    pub fn match_spans(&self, text: &str) -> Vec<(usize, usize)> {
        self.scan_spans(text.as_bytes(), true).spans
    }

    /// Find the secrets that straddle two consecutive text fragments (the
    /// last string of one event and the first string of the next) without
    /// changing either: `(prev_cut, next_cut)` are the byte offsets (on
    /// character boundaries) from which `prev` and up to which `next` carry
    /// spanning bytes — the earliest start of any spanning secret in `prev`
    /// and the furthest end of any in `next`, so every match is covered.
    pub fn find_boundary(&self, prev: &str, next: &str) -> Option<(usize, usize)> {
        if self.values.is_empty() || self.longest < 2 {
            return None;
        }
        let keep = self.longest - 1;
        let mut tail_start = prev.len().saturating_sub(keep);
        while tail_start < prev.len() && !prev.is_char_boundary(tail_start) {
            tail_start += 1;
        }
        let mut head_end = keep.min(next.len());
        while head_end > 0 && !next.is_char_boundary(head_end) {
            head_end -= 1;
        }
        let tail = prev.as_bytes()[tail_start..].to_vec();
        let head = next.as_bytes()[..head_end].to_vec();
        let mut joined = tail.clone();
        joined.extend_from_slice(&head);
        // Every occurrence that starts in the tail and ends in the head spans
        // the boundary: the union of their spans is masked.
        let mut cut: Option<(usize, usize)> = None;
        for secret in &self.values {
            for start in 0..tail.len() {
                let end = start + secret.len();
                if end > tail.len()
                    && end <= joined.len()
                    && &joined[start..end] == secret.as_slice()
                {
                    let candidate = (start, end - tail.len());
                    cut = Some(match cut {
                        None => candidate,
                        Some((s, e)) => (s.min(candidate.0), e.max(candidate.1)),
                    });
                }
            }
        }
        let (start_in_tail, end_in_head) = cut?;
        let mut prev_cut = tail_start + start_in_tail;
        while !prev.is_char_boundary(prev_cut) {
            prev_cut -= 1;
        }
        let mut next_cut = end_in_head;
        while next_cut < next.len() && !next.is_char_boundary(next_cut) {
            next_cut += 1;
        }
        Some((prev_cut, next_cut))
    }

    /// Mask a secret that straddles two consecutive text fragments: the
    /// spanning bytes are masked in both. Returns whether anything was masked.
    pub fn mask_boundary(&self, prev: &mut String, next: &mut String) -> bool {
        let Some((prev_cut, next_cut)) = self.find_boundary(prev, next) else {
            return false;
        };
        prev.truncate(prev_cut);
        prev.push_str(MASK);
        let rest = next[next_cut..].to_owned();
        next.clear();
        next.push_str(MASK);
        next.push_str(&rest);
        true
    }

    /// Byte lengths, longest first, of every suffix of `text` (on a character
    /// boundary, down to a single byte) that is a proper prefix of some
    /// secret: each could still be completed by later fragments, and each
    /// must be carried — a longer suffix continuing one secret says nothing
    /// about a shorter suffix continuing another.
    pub fn unresolved_prefix_lens(&self, text: &str) -> Vec<usize> {
        let mut lens = Vec::new();
        if self.longest < 2 {
            return lens;
        }
        let max = (self.longest - 1).min(text.len());
        let bytes = text.as_bytes();
        for len in (1..=max).rev() {
            let start = bytes.len() - len;
            if !text.is_char_boundary(start) {
                continue;
            }
            let suffix = &bytes[start..];
            if self
                .values
                .iter()
                .any(|secret| secret.len() > len && secret.starts_with(suffix))
            {
                lens.push(len);
            }
        }
        lens
    }

    /// Whether emitting `tail` in the clear would expose a substantial part
    /// of some secret it is a proper prefix of: four bytes or more, or at
    /// least half of that secret. Two or three letters that happen to open
    /// a 200-byte token ("ey" and "eyJ" open every JWT) are not substantial;
    /// two bytes of a three-byte secret are. A boundary that never continues
    /// can therefore expose at most three bytes of a long secret.
    pub fn prefix_is_substantial(&self, tail: &[u8]) -> bool {
        !tail.is_empty()
            && self.values.iter().any(|secret| {
                secret.len() > tail.len()
                    && secret.starts_with(tail)
                    && (tail.len() >= 4 || tail.len() * 2 >= secret.len())
            })
    }
    /// The longest unresolved suffix (see `unresolved_prefix_lens`), 0 when
    /// none.
    pub fn unresolved_prefix_len(&self, text: &str) -> usize {
        self.unresolved_prefix_lens(text)
            .first()
            .copied()
            .unwrap_or(0)
    }

    /// Continue an unresolved prefix carried from earlier fragments with the
    /// next fragment's first text, against every configured secret at once:
    /// `completed` is the longest completing head (0 when none), `extends`
    /// whether some secret is merely extended by the whole head.
    pub fn continuations(&self, carry: &[u8], head: &str) -> Continuation {
        let mut result = Continuation::default();
        if carry.is_empty() {
            return result;
        }
        let head_bytes = head.as_bytes();
        for secret in &self.values {
            if !secret.starts_with(carry) || secret.len() <= carry.len() {
                continue;
            }
            let rest = &secret[carry.len()..];
            if head_bytes.starts_with(rest) {
                let mut end = rest.len();
                while end < head.len() && !head.is_char_boundary(end) {
                    end += 1;
                }
                result.completed = result.completed.max(end);
            } else if rest.starts_with(head_bytes) {
                result.extends = true;
            }
        }
        result
    }

    /// The single strongest continuation: an extension (the whole head is
    /// secret bytes) outranks a completion, which outranks a break.
    pub fn continue_prefix(&self, carry: &[u8], head: &str) -> PrefixContinuation {
        let continuation = self.continuations(carry, head);
        if continuation.extends {
            PrefixContinuation::Extends
        } else if continuation.completed > 0 {
            PrefixContinuation::Completed(continuation.completed)
        } else {
            PrefixContinuation::Broken
        }
    }

    pub fn redact_str(&self, input: &str) -> String {
        String::from_utf8_lossy(&self.redact_bytes(input.as_bytes())).into_owned()
    }

    /// Native diagnostics may end at a capture limit before the complete
    /// credential was retained. Preserve the marker while masking that prefix.
    pub fn redact_diagnostic(&self, input: &str) -> String {
        const CUT: &str = " [output truncated]";
        input
            .split_inclusive(CUT)
            .map(|part| {
                let Some(body) = part.strip_suffix(CUT) else {
                    return self.redact_str(part);
                };
                let mut redactor = Redactor::new(self.clone());
                let mut output = redactor.push(body.as_bytes());
                // A pending prefix cannot be proved harmless after truncation.
                if redactor.retained() > 0 {
                    output.extend_from_slice(MASK.as_bytes());
                }
                output.extend_from_slice(CUT.as_bytes());
                String::from_utf8_lossy(&output).into_owned()
            })
            .collect()
    }

    pub fn redact_json(&self, value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::String(s) => serde_json::Value::String(self.redact_str(s)),
            serde_json::Value::Array(items) => {
                serde_json::Value::Array(items.iter().map(|v| self.redact_json(v)).collect())
            }
            // Keys are module-supplied text as much as values are. A
            // redacted key that collides with one already placed is suffixed
            // `#2`, `#3`, … in map order, so the result is deterministic.
            serde_json::Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (key, value) in map {
                    let mut redacted = self.redact_str(key);
                    if out.contains_key(&redacted) {
                        let base = redacted.clone();
                        let mut n = 2;
                        while out.contains_key(&redacted) {
                            redacted = format!("{base}#{n}");
                            n += 1;
                        }
                    }
                    out.insert(redacted, self.redact_json(value));
                }
                serde_json::Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

/// Streaming redactor that tolerates secrets split across chunk boundaries.
pub struct Redactor {
    secrets: Secrets,
    carry: Vec<u8>,
    /// The emitted output ends with a mask that the first held byte adjoins,
    /// so a span starting there continues it instead of adding another.
    open: bool,
    /// Leading held bytes that were already inside a masked span when the
    /// span was cut at a pending byte: they are never emitted in the clear.
    covered: usize,
}

impl Redactor {
    pub fn new(secrets: Secrets) -> Self {
        Self {
            secrets,
            carry: Vec::new(),
            open: false,
            covered: 0,
        }
    }

    /// Feed a chunk; returns the bytes that are now safe to emit. Only a
    /// trailing proper prefix of a secret is held back (at most `longest - 1`
    /// bytes), so a secret split across any number of chunks is still
    /// masked, and a run that keeps matching a self-overlapping secret is
    /// emitted as one mask without retaining or rescanning the run.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        if self.secrets.is_empty() {
            return chunk.to_vec();
        }
        self.carry.extend_from_slice(chunk);
        self.drain(false)
    }

    fn drain(&mut self, at_end: bool) -> Vec<u8> {
        let scan = self.secrets.scan_spans(&self.carry, at_end);
        let mut spans = scan.spans;
        let covered_now = self.covered.min(scan.consumed);
        if covered_now > 0 {
            spans.push((0, covered_now));
        }
        spans.sort_unstable();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        // Only overlapping spans merge: two adjacent secrets stay two masks,
        // as they do in a whole-buffer scan.
        for (start, end) in spans {
            match merged.last_mut() {
                Some((_, last_end)) if start < *last_end => *last_end = (*last_end).max(end),
                _ => merged.push((start, end)),
            }
        }
        let mut out = Vec::with_capacity(scan.consumed);
        let mut position = 0;
        for (index, (start, end)) in merged.iter().enumerate() {
            out.extend_from_slice(&self.carry[position..*start]);
            if !(index == 0 && *start == 0 && self.open) {
                out.extend_from_slice(MASK.as_bytes());
            }
            position = *end;
        }
        out.extend_from_slice(&self.carry[position..scan.consumed]);
        // The output ends in an open mask only when a span was cut at a
        // pending byte: the held bytes then continue that span. Adjacent
        // matches are separate masks, as in a whole-buffer scan.
        if scan.consumed > 0 {
            self.open = scan.covered > 0;
        }
        self.covered = scan.covered.max(self.covered.saturating_sub(scan.consumed));
        self.carry.drain(..scan.consumed);
        out
    }

    /// Replace the secret set mid-stream (a rotated lease token, a delivered
    /// capability); held bytes are re-examined against the new set.
    pub fn set_secrets(&mut self, secrets: Secrets) {
        self.secrets = secrets;
    }

    /// Flush held bytes at end of stream.
    pub fn finish(&mut self) -> Vec<u8> {
        let out = self.drain(true);
        self.carry.clear();
        self.open = false;
        self.covered = 0;
        out
    }

    /// Bytes currently held back.
    pub fn retained(&self) -> usize {
        self.carry.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_provider_credentials_are_redacted_across_chunks() {
        let secrets = Secrets::new(["  fixture-provider-key\n".into()]);
        let mut redactor = Redactor::new(secrets);
        let mut output = redactor.push(b"key=fixture-pro");
        output.extend(redactor.push(b"vider-key"));
        output.extend(redactor.finish());
        assert_eq!(output, b"key=[REDACTED]");
    }

    #[test]
    fn redacts_every_secret_value_regardless_of_key() {
        let secrets = Secrets::new(["supersecrettoken".to_string(), "another-secret-1".into()]);
        let line = "token=supersecrettoken other: another-secret-1 done";
        assert_eq!(
            secrets.redact_str(line),
            "token=[REDACTED] other: [REDACTED] done"
        );
    }

    #[test]
    fn a_prefix_is_substantial_at_four_bytes_or_half_a_short_secret() {
        let secrets = Secrets::new([
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop".to_string(),
            "k9q".into(),
        ]);
        assert!(!secrets.prefix_is_substantial(b"ey"));
        assert!(!secrets.prefix_is_substantial(b"eyJ"));
        assert!(secrets.prefix_is_substantial(b"eyJh"));
        assert!(secrets.prefix_is_substantial(b"k9"));
        assert!(!secrets.prefix_is_substantial(b"k"));
        assert!(!secrets.prefix_is_substantial(b"zz"));
        // Only a proper prefix of a configured secret can be substantial,
        // however long an unrelated tail is.
        assert!(!secrets.prefix_is_substantial(b"unrelated-tail"));
        assert!(!secrets.prefix_is_substantial(b"k9q"));
        assert!(!secrets.prefix_is_substantial(b""));
    }

    #[test]
    fn split_secret_across_chunks_is_still_masked() {
        let secrets = Secrets::new(["supersecrettoken".to_string()]);
        let mut redactor = Redactor::new(secrets);
        let mut out = Vec::new();
        out.extend(redactor.push(b"prefix supersec"));
        out.extend(redactor.push(b"rettoken suffix"));
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "prefix [REDACTED] suffix");
    }

    #[test]
    fn three_way_split_and_adjacent_secrets() {
        let secrets = Secrets::new(["abcdefghijkl".to_string()]);
        let mut redactor = Redactor::new(secrets);
        let mut out = Vec::new();
        for chunk in [&b"xxabcd"[..], b"efgh", b"ijklabcdefghijklyy"] {
            out.extend(redactor.push(chunk));
        }
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "xx[REDACTED][REDACTED]yy");
    }

    #[test]
    fn short_values_are_redacted_even_when_split_across_chunks() {
        let secrets = Secrets::new(["k9q".to_string()]);
        assert_eq!(
            secrets.redact_str("pw=k9q k9q."),
            "pw=[REDACTED] [REDACTED]."
        );
        let mut redactor = Redactor::new(secrets);
        let mut out = Vec::new();
        out.extend(redactor.push(b"pw=k"));
        out.extend(redactor.push(b"9"));
        out.extend(redactor.push(b"q done"));
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "pw=[REDACTED] done");
        assert!(Secrets::new([String::new()]).is_empty());
    }

    #[test]
    fn secret_split_across_two_text_fragments_is_masked_in_both() {
        let secrets = Secrets::new(["abc123".to_string()]);
        let mut prev = "delta abc".to_string();
        let mut next = "123 more".to_string();
        assert!(secrets.mask_boundary(&mut prev, &mut next));
        assert_eq!(prev, "delta [REDACTED]");
        assert_eq!(next, "[REDACTED] more");
        let mut prev = "nothing here".to_string();
        let mut next = "abc123 whole".to_string();
        assert!(!secrets.mask_boundary(&mut prev, &mut next));
        assert_eq!(
            next, "abc123 whole",
            "a whole occurrence is the per-fragment scan's job"
        );
    }

    #[test]
    fn a_shorter_secret_never_swallows_the_prefix_of_a_longer_one() {
        // Streaming: `k9` is complete, but the remainder is also a proper
        // prefix of `k9TAIL`, so it is held until the next chunk decides.
        let secrets = Secrets::new(["k9".to_string(), "k9TAIL".into()]);
        let mut redactor = Redactor::new(secrets.clone());
        let mut out = Vec::new();
        out.extend(redactor.push(b"k9"));
        assert!(out.is_empty(), "{out:?}");
        out.extend(redactor.push(b"TAIL"));
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "[REDACTED]");
        // At the end of the stream the complete shorter secret is masked.
        assert_eq!(secrets.redact_str("k9 and then"), "[REDACTED] and then");
        // Where both match, the longest is masked.
        assert_eq!(secrets.redact_str("k9TAIL"), "[REDACTED]");
        // Whole-buffer scans still mask a shorter secret followed by other text.
        assert_eq!(secrets.redact_str("k9TA"), "[REDACTED]TA");
    }

    #[test]
    fn overlapping_secrets_are_masked_as_one_span_in_buffers_and_streams() {
        let secrets = Secrets::new(["abc".to_string(), "bcTAIL".into()]);
        assert_eq!(secrets.redact_str("x abcTAIL y"), "x [REDACTED] y");
        assert_eq!(secrets.match_spans("x abcTAIL y"), [(2, 9)]);
        assert_eq!(secrets.redact_str("abc TAIL"), "[REDACTED] TAIL");
        for chunks in [
            &["ab", "cTAIL"][..],
            &["abc", "TAIL"][..],
            &["a", "bcTA", "IL"][..],
        ] {
            let mut redactor = Redactor::new(secrets.clone());
            let mut out = Vec::new();
            for chunk in chunks {
                out.extend(redactor.push(chunk.as_bytes()));
            }
            out.extend(redactor.finish());
            assert_eq!(String::from_utf8(out).unwrap(), "[REDACTED]", "{chunks:?}");
        }
        // A complete shorter secret whose overlap never materialises is
        // released once the stream shows it cannot.
        let mut redactor = Redactor::new(secrets.clone());
        let mut out = Vec::new();
        out.extend(redactor.push(b"abc"));
        out.extend(redactor.push(b" more"));
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "[REDACTED] more");
    }

    #[test]
    fn a_self_overlapping_run_is_one_mask_with_bounded_retention() {
        let secrets = Secrets::new(["aa".to_string()]);
        let mut redactor = Redactor::new(secrets.clone());
        let mut out = Vec::new();
        for _ in 0..16 {
            out.extend(redactor.push(&[b'a'; 8192]));
            assert!(redactor.retained() < 2, "{}", redactor.retained());
        }
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "[REDACTED]");
        // The held byte a cut span already covered is never emitted in the
        // clear when the run stops.
        let mut redactor = Redactor::new(secrets.clone());
        let mut out = Vec::new();
        out.extend(redactor.push(b"aaa"));
        out.extend(redactor.push(b"b"));
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "[REDACTED]b");
        let mut redactor = Redactor::new(secrets);
        let mut out = Vec::new();
        out.extend(redactor.push(b"xaa"));
        out.extend(redactor.finish());
        assert_eq!(String::from_utf8(out).unwrap(), "x[REDACTED]");
    }

    #[test]
    fn every_secret_spanning_a_pair_is_masked_and_every_suffix_is_carried() {
        // Two secrets span the same boundary with different heads: the union
        // of their spans is masked.
        let secrets = Secrets::new(["abc".to_string(), "bcTAIL".into()]);
        let mut prev = "ab".to_owned();
        let mut next = "cTAIL rest".to_owned();
        assert!(secrets.mask_boundary(&mut prev, &mut next));
        assert_eq!(prev, "[REDACTED]");
        assert_eq!(next, "[REDACTED] rest");
        // Every suffix that is a proper prefix of some secret is reported,
        // longest first: `ab` (of `abX`) and `b` (of `bTAIL`).
        let secrets = Secrets::new(["abX".to_string(), "bTAIL".into()]);
        assert_eq!(secrets.unresolved_prefix_lens("xab"), [2, 1]);
        assert_eq!(secrets.unresolved_prefix_len("xab"), 2);
        // A completion and an extension by different secrets are both seen.
        let secrets = Secrets::new(["k9".to_string(), "k9TAIL".into()]);
        let continuation = secrets.continuations(b"k", "9");
        assert_eq!(continuation.completed, 1);
        assert!(continuation.extends);
        assert_eq!(
            secrets.continue_prefix(b"k", "9"),
            PrefixContinuation::Extends
        );
    }

    #[test]
    fn keys_are_redacted_with_deterministic_collision_suffixes() {
        let secrets = Secrets::new(["hunter2hunter2".to_string()]);
        let value = serde_json::json!({
            "hunter2hunter2": 1,
            "plain": "hunter2hunter2",
            "[REDACTED]": 2,
        });
        assert_eq!(
            secrets.redact_json(&value),
            serde_json::json!({"[REDACTED]": 2, "[REDACTED]#2": 1, "plain": "[REDACTED]"})
        );
    }

    #[test]
    fn unresolved_prefixes_are_carried_across_any_number_of_fragments() {
        let secrets = Secrets::new(["abc123".to_string()]);
        assert_eq!(secrets.unresolved_prefix_len("delta ab"), 2);
        assert_eq!(secrets.unresolved_prefix_len("delta a"), 1);
        assert_eq!(secrets.unresolved_prefix_len("delta abc12"), 5);
        assert_eq!(
            secrets.unresolved_prefix_len("delta abc123"),
            0,
            "a whole secret is not a prefix"
        );
        assert_eq!(secrets.unresolved_prefix_len("delta x"), 0);
        assert_eq!(
            secrets.continue_prefix(b"ab", "c1"),
            PrefixContinuation::Extends
        );
        assert_eq!(
            secrets.continue_prefix(b"abc1", "23 more"),
            PrefixContinuation::Completed(2)
        );
        assert_eq!(
            secrets.continue_prefix(b"ab", "xyz"),
            PrefixContinuation::Broken
        );
        assert_eq!(
            secrets.continue_prefix(b"", "abc123"),
            PrefixContinuation::Broken
        );
    }

    #[test]
    fn streaming_redactor_learns_new_secrets_mid_stream() {
        let mut redactor = Redactor::new(Secrets::new(["first-secret".to_string()]));
        let mut out = Vec::new();
        out.extend(redactor.push(b"a=first-secret b="));
        redactor.set_secrets(Secrets::new([
            "first-secret".to_string(),
            "second-secret".into(),
        ]));
        out.extend(redactor.push(b"second-"));
        out.extend(redactor.push(b"secret end"));
        out.extend(redactor.finish());
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "a=[REDACTED] b=[REDACTED] end"
        );
    }
}
