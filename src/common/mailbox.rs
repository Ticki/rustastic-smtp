// Copyright 2014 The Rustastic SMTP Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tools to parse and represent an email address in an SMTP transaction.

use std::string::String;
use super::utils;
use std::io::net::ip::IpAddr;
use std::ascii::OwnedAsciiExt;
use std::ascii::AsciiExt;
use std::borrow::ToOwned;

/// Maximum length of the local part.
static MAX_MAILBOX_LOCAL_PART_LEN: usize = 64;

/// Maximum length of an email address.
///
/// The RFC doesn't actually specify 254 chars, but it does say that a reverse path starts with
/// "<", ends with ">" and including those symbols has a maximum length of 256.
static MAX_MAILBOX_LEN: usize = 254;

/// Maximum length of a domain name.
static MAX_DOMAIN_LEN: usize = 255;

#[test]
fn test_static_vars() {
    assert_eq!(64, MAX_MAILBOX_LOCAL_PART_LEN);
    assert_eq!(254, MAX_MAILBOX_LEN);
    assert_eq!(255, MAX_DOMAIN_LEN);
}

fn get_mailbox_local_part(s: &str) -> Option<&str> {
    utils::get_dot_string(s).or_else(|| utils::get_quoted_string(s))
}

#[test]
fn test_local_part() {
    assert_eq!(Some("rust.cool"), get_mailbox_local_part("rust.cool"));
    assert_eq!(Some("\"rust \\a cool\""), get_mailbox_local_part("\"rust \\a cool\""));
    assert_eq!(Some("\"rust.cool\""), get_mailbox_local_part("\"rust.cool\""));
    assert_eq!(Some("\"rust.cool.\""), get_mailbox_local_part("\"rust.cool.\""));
    assert_eq!(Some("\"rust\\\\\\b\\;.c\\\"ool\""), get_mailbox_local_part("\"rust\\\\\\b\\;.c\\\"ool\""));
}

/// Represents the foreign part of an email address, aka the host.
#[derive(PartialEq, Eq, Clone, Show)]
pub enum MailboxForeignPart {
    /// The foreign part is a domain name.
    Domain(String),
    /// The foreign part is an ip address.
    IpAddr(IpAddr)
}

#[test]
fn test_foreign_part() {
    let domain_text = "rustastic.org";
    let domain = MailboxForeignPart::Domain(domain_text.to_owned());
    let ipv4 = MailboxForeignPart::IpAddr(IpAddr::Ipv4Addr(127, 0, 0, 1));
    let ipv6 = MailboxForeignPart::IpAddr(IpAddr::Ipv6Addr(1, 1, 1, 1, 1, 1, 1, 1));

    assert!(domain == domain);
    assert!(domain != MailboxForeignPart::Domain(domain_text.to_owned() + "bullshit"));
    assert!(domain != ipv4);
    assert!(domain != ipv6);
}

/// Represents an email address, aka "mailbox" in the SMTP spec.
///
/// It is composed of a local part and a foreign part. If the address is sent to the `Postmaster`
/// address for a domain, then the local part will always be converted `postmaster`, all lowercase.
/// Since the `Postmaster` address must be handled without regard for case, this makes things simpler.
#[derive(PartialEq, Eq, Clone, Show)]
pub struct Mailbox {
    local_part: String,
    foreign_part: MailboxForeignPart
}

/// Represents an error that occured while trying to parse an email address.
#[derive(PartialEq, Eq, Clone, Show, Copy)]
pub enum MailboxParseError {
    /// The maximum length of 64 octets [as per RFC 5321](http://tools.ietf.org/html/rfc5321#section-4.5.3.1.1) is exceeded.
    LocalPartTooLong,
    /// The local part was neither a atom, nor a quoted string.
    LocalPartUnrecognized,
    /// The foreign part was neither a domain, nor an IP.
    ForeignPartUnrecognized,
    /// The maximum length of 255 octets [as per RFC 5321](http://tools.ietf.org/html/rfc5321#section-4.5.3.1.2) is exceeded.
    DomainTooLong,
    /// The maximum length of 254 octets (256 - 2 for punctuaction) [as per RFC 5321](http://tools.ietf.org/html/rfc5321#section-4.5.3.1.3) is exceeded.
    TooLong,
    /// If no @ was present.
    AtNotFound
}

impl Mailbox {
    /// Creates a `Mailbox` from a string if the string contains a valid email
    /// address. Otherwise, returns a `MailboxParseError`.
    ///
    /// The argument should be of the form:
    /// `hello@world.com`
    /// This function does *not* expect anything to wrap the passed email
    /// address. For example, this will result in an error:
    /// `<hello@world.com>`
    pub fn parse(s: &str) -> Result<Mailbox, MailboxParseError> {
        let mut local_part: String;
        let mut foreign_part: MailboxForeignPart;

        // Skip the source routes as specified in RFC 5321.
        let mut offset = utils::get_source_route(s).map_or(0, |s| s.len());

        // Get the local part.
        match get_mailbox_local_part(s.slice_from(offset)) {
            Some(lp) => {
                if lp.len() > MAX_MAILBOX_LOCAL_PART_LEN {
                    return Err(MailboxParseError::LocalPartTooLong);
                }
                local_part = lp.to_owned();
                offset += lp.len();
            },
            None => {
                return Err(MailboxParseError::LocalPartUnrecognized);
            }
        }

        // Check if the email address continues to find an @.
        if offset >= s.len() {
            return Err(MailboxParseError::AtNotFound);
        }
        // If no @ is found, it means we're still in what should be the local
        // part but it is invalid, ie "rust is@rustastic.org".
        if s.char_at(offset) != '@' {
            return Err(MailboxParseError::LocalPartUnrecognized);
        }
        offset += 1;

        match utils::get_domain(s.slice_from(offset)) {
            Some(d) => {
                // Is the domain is too long ?
                if d.len() > MAX_DOMAIN_LEN {
                    return Err(MailboxParseError::DomainTooLong);
                }
                // Save the domain.
                foreign_part = MailboxForeignPart::Domain(
                    s.slice(offset, offset + d.len()).to_owned()
                );
                offset += d.len();
            },
            None => {
                match utils::get_mailbox_ip(s.slice_from(offset)) {
                    Some((ip, addr)) => {
                        foreign_part = MailboxForeignPart::IpAddr(addr);
                        offset += ip.len();
                    },
                    None => {
                        return Err(MailboxParseError::ForeignPartUnrecognized);
                    }
                }
            }
        }

        // Example would be "rust.is@rustastic.org{}" where "rustastic.org{}"
        // would be considered an invalid domain name.
        if offset != s.len() {
            Err(MailboxParseError::ForeignPartUnrecognized)
        // Overall, is the email address to long? We could test this at the
        // beginning of the function to potentially save processing power, but
        // this shouldn't happen too often and this error doesn't give much
        // information whereas LocalPartTooLong is more precise which allows
        // for more understandable debug messages.
        } else if offset > MAX_MAILBOX_LEN {
            Err(MailboxParseError::TooLong)
        } else {
            // The special "Postmaster" address must be handled differently.
            // It is ASCII for sure, and since `into_ascii_lower` may panic for
            // non ascii strings, we make this check first.
            if local_part.as_slice().is_ascii() {
                // We make this special address lowercase so the server can
                // avoid to check this again. Basically, we're saying that if
                // the email is sent by or to Postmaster, we know that the email
                // will be lowercase.
                //
                // We don't do this for other addresses though. Here's why:
                // Imagine you want to build an email hosting service. You may
                // want to allow your members to see the case that the person on
                // the other end chose to give you. Also, handling low/up case
                // with UTF8 strings is non trivial. Since SMTP allows non-ASCII
                // mailboxes with RFC 5336, we'll let the case conversion up to
                // the individual commands that a server wishes to implement.
                //
                // RFC 5336: https://tools.ietf.org/html/rfc5336
                local_part = local_part.into_ascii_lowercase();
                if local_part.as_slice() == "postmaster" {
                    local_part = "postmaster".to_owned();
                }
            }
            Ok(Mailbox {
                local_part: local_part,
                foreign_part: foreign_part
            })
        }
    }
}

#[test]
fn test_mailbox() {
    let mut s = String::from_char(MAX_MAILBOX_LOCAL_PART_LEN, 'a');
    s.push_str("@t.com");
    assert!(Mailbox::parse(s.as_slice()).is_ok());
    let mut s = String::from_char(MAX_MAILBOX_LOCAL_PART_LEN + 1, 'a');
    s.push_str("@t.com");
    assert_eq!(Err(MailboxParseError::LocalPartTooLong), Mailbox::parse(s.as_slice()));
    assert_eq!(Err(MailboxParseError::LocalPartUnrecognized), Mailbox::parse("t @t.com{"));
    assert_eq!(Err(MailboxParseError::LocalPartUnrecognized), Mailbox::parse("t "));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("t@{}"));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("t@t.com{"));

    // The check here is to expect something else than DomainTooLong.
    assert_eq!(Err(MailboxParseError::TooLong), Mailbox::parse(
        ("rust@".to_owned() + String::from_char(MAX_DOMAIN_LEN, 'a'))
            .as_slice()
    ));
    assert_eq!(Err(MailboxParseError::DomainTooLong), Mailbox::parse(
        ("rust@".to_owned() + String::from_char(MAX_DOMAIN_LEN + 1, 'a'))
            .as_slice()
    ));
    assert!(Mailbox::parse(
        ("rust@".to_owned() + String::from_char(MAX_MAILBOX_LEN - 5, 'a'))
            .as_slice()
    ).is_ok());
    assert_eq!(Err(MailboxParseError::TooLong), Mailbox::parse(
        ("rust@".to_owned() + String::from_char(MAX_MAILBOX_LEN - 4, 'a'))
            .as_slice()
    ));

    // Check some common error cases.
    assert_eq!(Err(MailboxParseError::AtNotFound), Mailbox::parse("t"));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("rust.is@[127.0.0.1"));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("rust.is@[00.0.1]"));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("rust.is@[::1]"));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("rust.is@[Ipv6: ::1]"));
    assert_eq!(Err(MailboxParseError::ForeignPartUnrecognized), Mailbox::parse("rust.is@[Ipv6:::1"));

    // Check that we can compare mailboxes
    let path_1 = Mailbox::parse("rust.is@rustastic.org").unwrap();
    let path_2 = Mailbox::parse("rust.is.not@rustastic.org").unwrap();
    let path_3 = Mailbox::parse("\"hello\"@rust").unwrap();

    assert!(path_1 == path_1.clone());
    assert!(path_2 == path_2.clone());
    assert!(path_1 != path_2);
    assert_eq!(path_3.local_part.as_slice(), "\"hello\"");
    assert_eq!(path_3.foreign_part, MailboxForeignPart::Domain("rust".to_owned()));

    // Check that parsing of IP addresses is done right.
    let path_4 = Mailbox::parse("rust.is@[127.0.0.1]").unwrap();
    assert_eq!(path_4.foreign_part, MailboxForeignPart::IpAddr(
        IpAddr::Ipv4Addr(127, 0, 0, 1)
    ));

    let path_5 = Mailbox::parse("rust.is@[Ipv6:::1]").unwrap();
    assert_eq!(path_5.foreign_part, MailboxForeignPart::IpAddr(
        IpAddr::Ipv6Addr(0, 0, 0, 0, 0, 0, 0, 1)
    ));

    let path_6 = Mailbox::parse("rust.is@[Ipv6:2001:db8::ff00:42:8329]").unwrap();
    assert_eq!(path_6.foreign_part, MailboxForeignPart::IpAddr(
        IpAddr::Ipv6Addr(0x2001, 0xdb8, 0x0, 0x0, 0x0, 0xff00, 0x42, 0x8329)
    ));

    // Make sure that the special postmaster address is always lowercase.
    let path_7 = Mailbox::parse("PosTMAster@ok").unwrap();
    assert_eq!("postmaster", path_7.local_part.as_slice());

    let path_8 = Mailbox::parse("postmaster@ok").unwrap();
    assert_eq!("postmaster", path_8.local_part.as_slice());
}
