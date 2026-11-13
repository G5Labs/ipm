//! Versioned SHA-256 chaining state for bounded native-session appends.

use sha2::compress256;
use sha2::digest::generic_array::GenericArray;

const INITIAL: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];
const HEADER_BYTES: usize = 1 + 8 * 4 + 8 + 1;

#[derive(Clone, Debug)]
pub(crate) struct ResumableSha256 {
    words: [u32; 8],
    len: u64,
    tail: Vec<u8>,
}

impl ResumableSha256 {
    pub(crate) fn new() -> Self {
        Self {
            words: INITIAL,
            len: 0,
            tail: Vec::new(),
        }
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        let mut hash = Self::new();
        hash.update(bytes)
            .expect("a session file fits SHA-256 length");
        hash
    }

    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_BYTES || bytes[0] != 1 {
            return None;
        }
        let tail_len = usize::from(bytes[HEADER_BYTES - 1]);
        if tail_len >= 64 || bytes.len() != HEADER_BYTES + tail_len {
            return None;
        }
        let mut words = [0; 8];
        for (word, chunk) in words.iter_mut().zip(bytes[1..33].as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*chunk);
        }
        let len = u64::from_be_bytes(bytes[33..41].try_into().ok()?);
        if len % 64 != tail_len as u64 {
            return None;
        }
        Some(Self {
            words,
            len,
            tail: bytes[HEADER_BYTES..].to_vec(),
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(HEADER_BYTES + self.tail.len());
        bytes.push(1);
        for word in self.words {
            bytes.extend_from_slice(&word.to_be_bytes());
        }
        bytes.extend_from_slice(&self.len.to_be_bytes());
        bytes.push(self.tail.len() as u8);
        bytes.extend_from_slice(&self.tail);
        bytes
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    pub(crate) fn update(&mut self, mut bytes: &[u8]) -> Option<()> {
        let added = u64::try_from(bytes.len()).ok()?;
        self.len = self.len.checked_add(added)?;
        if !self.tail.is_empty() {
            let take = bytes.len().min(64 - self.tail.len());
            self.tail.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.tail.len() == 64 {
                compress256(
                    &mut self.words,
                    &[GenericArray::clone_from_slice(&self.tail)],
                );
                self.tail.clear();
            }
        }
        let (blocks, remainder) = bytes.as_chunks::<64>();
        for block in blocks {
            compress256(&mut self.words, &[GenericArray::clone_from_slice(block)]);
        }
        self.tail.extend_from_slice(remainder);
        Some(())
    }

    pub(crate) fn digest_hex(&self) -> String {
        let mut words = self.words;
        let mut final_block = [0_u8; 128];
        final_block[..self.tail.len()].copy_from_slice(&self.tail);
        final_block[self.tail.len()] = 0x80;
        let padded_len = if self.tail.len() < 56 { 64 } else { 128 };
        final_block[padded_len - 8..padded_len].copy_from_slice(&(self.len * 8).to_be_bytes());
        for block in final_block[..padded_len].as_chunks::<64>().0 {
            compress256(&mut words, &[GenericArray::clone_from_slice(block)]);
        }
        let mut digest = [0_u8; 32];
        for (index, word) in words.iter().enumerate() {
            digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        hex::encode(digest)
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::ResumableSha256;

    #[test]
    fn resumable_sha256_matches_full_digest_at_padding_and_random_partitions() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        for len in [
            0, 1, 55, 56, 63, 64, 65, 119, 120, 127, 128, 129, 255, 1023, 8193,
        ] {
            let mut bytes = vec![0; len];
            for byte in &mut bytes {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                *byte = seed as u8;
            }
            for partition in 0..16 {
                let mut hash = ResumableSha256::new();
                let mut offset = 0;
                while offset < bytes.len() {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let step = (seed as usize % (partition * 17 + 1)).max(1);
                    let end = (offset + step).min(bytes.len());
                    hash.update(&bytes[offset..end]).unwrap();
                    hash = ResumableSha256::decode(&hash.encode()).unwrap();
                    offset = end;
                }
                assert_eq!(hash.digest_hex(), hex::encode(Sha256::digest(&bytes)));
                assert_eq!(hash.len(), bytes.len() as u64);
            }
            let one_shot = ResumableSha256::from_bytes(&bytes);
            assert_eq!(one_shot.digest_hex(), hex::encode(Sha256::digest(&bytes)));
        }
        let mut malformed = ResumableSha256::from_bytes(b"hello").encode();
        malformed[0] = 2;
        assert!(ResumableSha256::decode(&malformed).is_none());
        malformed[0] = 1;
        malformed[41] = 4;
        assert!(ResumableSha256::decode(&malformed).is_none());
    }
}
