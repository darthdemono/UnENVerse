// SPDX-License-Identifier: Apache-2.0
// Copyright the UnENVerse contributors.
//! What a certificate says about itself: when it is valid and for which names.
//!
//! A certificate entry's `issued_at` and `expires_at` come from here, not from the clock of the
//! machine that fetched it, so a renewal check reads the dates the CA signed.

use x509_parser::extensions::GeneralName;
use x509_parser::pem::Pem;

/// The leaf of a PEM chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertInfo {
    /// `YYYY-MM-DDTHH:MM:SSZ`.
    pub not_before: String,
    pub not_after: String,
    /// Seconds since the epoch, for comparisons.
    pub not_after_epoch: i64,
    /// DNS names from the subject alternative names, lower case.
    pub dns_names: Vec<String>,
}

/// Reads the first certificate in `pem`, which for a chain written by an ACME client is the leaf.
pub fn leaf(pem: &str) -> Result<CertInfo, String> {
    let block = Pem::iter_from_buffer(pem.as_bytes())
        .filter_map(Result::ok)
        .find(|p| p.label == "CERTIFICATE")
        .ok_or("no certificate in the PEM text")?;
    let cert = block
        .parse_x509()
        .map_err(|e| format!("not a valid certificate: {e}"))?;
    let validity = cert.validity();
    let mut dns_names: Vec<String> = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|san| {
            san.value
                .general_names
                .iter()
                .filter_map(|n| match n {
                    GeneralName::DNSName(d) => Some(d.to_ascii_lowercase()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    dns_names.sort();
    Ok(CertInfo {
        not_before: crate::pgp::iso(validity.not_before.timestamp()),
        not_after: crate::pgp::iso(validity.not_after.timestamp()),
        not_after_epoch: validity.not_after.timestamp(),
        dns_names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_dates_and_names_the_ca_signed() {
        let key = rcgen::KeyPair::generate().unwrap();
        let mut params =
            rcgen::CertificateParams::new(vec!["B.Example.test".into(), "a.example.test".into()])
                .unwrap();
        params.not_before = time::OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        params.not_after = time::OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let pem = params.self_signed(&key).unwrap().pem();
        let info = leaf(&pem).unwrap();
        assert_eq!(info.not_before, "2023-11-14T22:13:20Z");
        assert_eq!(info.not_after, "2027-01-15T08:00:00Z");
        assert_eq!(info.not_after_epoch, 1_800_000_000);
        assert_eq!(info.dns_names, ["a.example.test", "b.example.test"]);
    }

    #[test]
    fn text_without_a_certificate_is_refused() {
        assert!(leaf("hello").unwrap_err().contains("no certificate"));
        let key_only = rcgen::KeyPair::generate().unwrap().serialize_pem();
        assert!(leaf(&key_only).is_err(), "a key is not a certificate");
    }

    #[test]
    fn a_certificate_block_that_is_not_one_is_refused() {
        let bad = "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        assert!(leaf(bad).unwrap_err().contains("not a valid certificate"));
    }
}
