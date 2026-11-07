// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! The OpenSSH agent protocol (R32), the part that needs no socket.
//!
//! `unv ssh-agent` lets `ssh` use a key that lives in the vault and never in `~/.ssh`. The agent
//! answers two questions: which keys do you hold (`REQUEST_IDENTITIES`) and sign this for me
//! (`SIGN_REQUEST`). It never returns a private key: the only things that leave are public key
//! blobs, comments and signatures. Anything else, including adding or removing keys, is refused
//! with `FAILURE`.
//!
//! Wire format (draft-miller-ssh-agent): a message is a `uint32` length, a type byte and a body;
//! strings are a `uint32` length and bytes. Signing is done by the reviewed `ssh-key` crate, not
//! by code here. Only Ed25519 keys are accepted; an encrypted key, or any other algorithm, is
//! skipped at load time and counted so the caller can say so.

use ssh_key::{PrivateKey, SigningKey};

pub const FAILURE: u8 = 5;
pub const REQUEST_IDENTITIES: u8 = 11;
pub const IDENTITIES_ANSWER: u8 = 12;
pub const SIGN_REQUEST: u8 = 13;
pub const SIGN_RESPONSE: u8 = 14;

/// The largest message accepted from a client. A sign request carries a hash-sized challenge or a
/// session blob; nothing legitimate comes near this.
pub const MAX_MESSAGE: usize = 256 * 1024;

/// One key the agent holds: its public blob, its comment, and the private half it signs with.
pub struct AgentKey {
    pub blob: Vec<u8>,
    pub comment: String,
    private: PrivateKey,
}

impl AgentKey {
    /// Parses an OpenSSH private key. Refuses an encrypted key (the agent cannot ask for a
    /// passphrase) and any algorithm other than Ed25519.
    pub fn from_pem(pem: &str, fallback_comment: &str) -> Result<Self, String> {
        let private = PrivateKey::from_openssh(pem.trim()).map_err(|e| e.to_string())?;
        if private.is_encrypted() {
            return Err("the key is encrypted".into());
        }
        if private.algorithm() != ssh_key::Algorithm::Ed25519 {
            return Err(format!("{} keys are not supported", private.algorithm()));
        }
        let blob = private.public_key().to_bytes().map_err(|e| e.to_string())?;
        let comment = if private.comment().is_empty() {
            fallback_comment.to_string()
        } else {
            private.comment().to_string()
        };
        Ok(Self {
            blob,
            comment,
            private,
        })
    }

    /// The signature blob for `data`: the algorithm name and the raw signature, as `ssh` expects.
    fn sign(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        sign_with(&self.private, data)
    }
}

fn sign_with<K: SigningKey>(key: &K, data: &[u8]) -> Result<Vec<u8>, String> {
    let sig = key.try_sign(data).map_err(|e| e.to_string())?;
    Vec::<u8>::try_from(sig).map_err(|e| e.to_string())
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put_string(out: &mut Vec<u8>, s: &[u8]) {
    put_u32(out, s.len() as u32);
    out.extend_from_slice(s);
}

/// Reads a big-endian `uint32` from the front of `buf`.
fn take_u32(buf: &mut &[u8]) -> Option<u32> {
    let (head, rest) = buf.split_first_chunk::<4>()?;
    *buf = rest;
    Some(u32::from_be_bytes(*head))
}

/// Reads a length-prefixed string from the front of `buf`, refusing a length past the end.
fn take_string<'a>(buf: &mut &'a [u8]) -> Option<&'a [u8]> {
    let n = take_u32(buf)? as usize;
    if n > buf.len() {
        return None;
    }
    let (s, rest) = buf.split_at(n);
    *buf = rest;
    Some(s)
}

/// The answer to one request payload (type byte and body, without the length prefix). Returns the
/// reply payload and, when a signature was made, the comment of the key that made it, so the
/// caller can audit it.
pub fn handle(request: &[u8], keys: &[AgentKey]) -> (Vec<u8>, Option<String>) {
    let failure = || (vec![FAILURE], None);
    let Some((&kind, mut body)) = request.split_first() else {
        return failure();
    };
    match kind {
        REQUEST_IDENTITIES => {
            let mut out = vec![IDENTITIES_ANSWER];
            put_u32(&mut out, keys.len() as u32);
            for k in keys {
                put_string(&mut out, &k.blob);
                put_string(&mut out, k.comment.as_bytes());
            }
            (out, None)
        }
        SIGN_REQUEST => {
            let (Some(blob), Some(data), Some(_flags)) = (
                take_string(&mut body),
                take_string(&mut body),
                take_u32(&mut body),
            ) else {
                return failure();
            };
            let Some(key) = keys.iter().find(|k| k.blob == blob) else {
                return failure();
            };
            match key.sign(data) {
                Ok(sig) => {
                    let mut out = vec![SIGN_RESPONSE];
                    put_string(&mut out, &sig);
                    (out, Some(key.comment.clone()))
                }
                Err(_) => failure(),
            }
        }
        _ => failure(),
    }
}

/// Serves one client over any byte stream until it closes, calling `signed(comment)` after each
/// signature. A message over [`MAX_MESSAGE`] ends the connection without being read.
pub fn serve<S: std::io::Read + std::io::Write>(
    stream: &mut S,
    keys: &[AgentKey],
    mut signed: impl FnMut(&str),
) -> std::io::Result<()> {
    loop {
        let mut len = [0u8; 4];
        match stream.read_exact(&mut len) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        }
        let n = u32::from_be_bytes(len) as usize;
        if n == 0 || n > MAX_MESSAGE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "message length out of range",
            ));
        }
        let mut payload = vec![0u8; n];
        stream.read_exact(&mut payload)?;
        let (reply, by) = handle(&payload, keys);
        if let Some(comment) = by {
            signed(&comment);
        }
        stream.write_all(&(reply.len() as u32).to_be_bytes())?;
        stream.write_all(&reply)?;
        stream.flush()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entropy::Source;

    /// A fresh Ed25519 key made by the project's own generator, with the private seed so a test
    /// can prove it never appears in an answer.
    fn key(comment: &str) -> (AgentKey, Vec<u8>) {
        let made = crate::generate_ssh_keypair(comment, &Source::Os).unwrap();
        let pem = made["private_key"].as_str().unwrap().to_string();
        let parsed = PrivateKey::from_openssh(pem.trim()).unwrap();
        let seed = parsed
            .key_data()
            .ed25519()
            .unwrap()
            .private
            .to_bytes()
            .to_vec();
        (AgentKey::from_pem(&pem, "fallback").unwrap(), seed)
    }

    fn sign_request(blob: &[u8], data: &[u8]) -> Vec<u8> {
        let mut r = vec![SIGN_REQUEST];
        put_string(&mut r, blob);
        put_string(&mut r, data);
        put_u32(&mut r, 0);
        r
    }

    #[test]
    fn identities_list_each_public_blob_and_comment_and_no_private_bytes() {
        let (a, seed_a) = key("alpha");
        let (b, seed_b) = key("beta");
        let (reply, by) = handle(&[REQUEST_IDENTITIES], &[a, b]);
        assert_eq!(reply[0], IDENTITIES_ANSWER);
        assert_eq!(u32::from_be_bytes(reply[1..5].try_into().unwrap()), 2);
        assert!(by.is_none());
        for seed in [seed_a, seed_b] {
            assert!(
                !reply.windows(seed.len()).any(|w| w == seed.as_slice()),
                "a private key must never be in an answer"
            );
        }
        let text = String::from_utf8_lossy(&reply);
        assert!(text.contains("alpha") && text.contains("beta"));
    }

    #[test]
    fn a_signature_is_an_ed25519_blob_and_names_the_key_that_made_it() {
        let (a, seed) = key("alpha");
        let blob = a.blob.clone();
        let (reply, by) = handle(&sign_request(&blob, b"challenge"), &[a]);
        assert_eq!(reply[0], SIGN_RESPONSE);
        assert_eq!(by.as_deref(), Some("alpha"));
        let mut rest = &reply[1..];
        let sig = take_string(&mut rest).unwrap();
        // The signature blob is the algorithm name followed by the 64 raw signature bytes.
        let mut s = sig;
        assert_eq!(take_string(&mut s).unwrap(), b"ssh-ed25519");
        assert_eq!(take_string(&mut s).unwrap().len(), 64);
        assert!(!reply.windows(seed.len()).any(|w| w == seed.as_slice()));
    }

    #[test]
    fn an_unknown_key_a_short_request_and_every_other_request_are_refused() {
        let (a, _) = key("alpha");
        let keys = [a];
        for bad in [
            vec![],
            vec![SIGN_REQUEST],
            sign_request(b"not a held key", b"x"),
            vec![17, 0, 0, 0, 0],
            vec![18, 0, 0, 0, 1, 1],
            vec![9],
        ] {
            let (reply, by) = handle(&bad, &keys);
            assert_eq!(reply, vec![FAILURE], "{bad:?}");
            assert!(by.is_none());
        }
    }

    #[test]
    fn a_string_longer_than_the_message_is_refused_not_read() {
        let (a, _) = key("alpha");
        let mut r = vec![SIGN_REQUEST];
        put_u32(&mut r, u32::MAX);
        r.extend_from_slice(b"short");
        assert_eq!(handle(&r, &[a]).0, vec![FAILURE]);
    }

    #[test]
    fn text_that_is_not_a_key_is_refused_at_load() {
        assert!(AgentKey::from_pem("not a key", "c").is_err());
    }

    #[test]
    fn the_fallback_comment_is_used_when_the_key_has_none() {
        let made = crate::generate_ssh_keypair("", &Source::Os).unwrap();
        let pem = made["private_key"].as_str().unwrap();
        let k = AgentKey::from_pem(pem, "from the entry name").unwrap();
        assert_eq!(k.comment, "from the entry name");
    }

    /// A stream with a scripted input and a captured output.
    struct Duplex {
        r: std::io::Cursor<Vec<u8>>,
        w: Vec<u8>,
    }
    impl std::io::Read for Duplex {
        fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
            std::io::Read::read(&mut self.r, b)
        }
    }
    impl std::io::Write for Duplex {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.w.write(b)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn serve_answers_framed_requests_in_order_audits_signatures_and_ends_on_close() {
        let (a, _) = key("alpha");
        let blob = a.blob.clone();
        let mut input = Vec::new();
        for req in [vec![REQUEST_IDENTITIES], sign_request(&blob, b"d")] {
            input.extend_from_slice(&(req.len() as u32).to_be_bytes());
            input.extend_from_slice(&req);
        }
        let mut d = Duplex {
            r: std::io::Cursor::new(input),
            w: Vec::new(),
        };
        let mut audited = Vec::new();
        serve(&mut d, &[a], |c| audited.push(c.to_string())).unwrap();
        assert_eq!(audited, vec!["alpha"]);
        let first = u32::from_be_bytes(d.w[0..4].try_into().unwrap()) as usize;
        assert_eq!(d.w[4], IDENTITIES_ANSWER);
        // The second frame follows the first, and is the signature response.
        assert_eq!(d.w[4 + first + 4], SIGN_RESPONSE);
    }

    #[test]
    fn an_oversized_or_empty_frame_ends_the_connection_unread() {
        for len in [0u32, (MAX_MESSAGE as u32) + 1] {
            let mut d = Duplex {
                r: std::io::Cursor::new(len.to_be_bytes().to_vec()),
                w: Vec::new(),
            };
            assert!(serve(&mut d, &[], |_| {}).is_err());
            assert!(d.w.is_empty());
        }
    }
}
