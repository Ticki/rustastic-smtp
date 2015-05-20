# Rustastic SMTP

Rustastic SMTP is meant to provide SMTP tools such as email address parsing
utilities as well as a configurable SMTP server and client. The goal is to eventually comply with the
[SMTP spec from RFC 5321](http://tools.ietf.org/html/rfc5321).

Here's a public roadmap: https://trello.com/b/nC5JR22d/rsmtp. Open an issue or

**THIS PACKAGE IS CURRENTLY UNSTABLE.**

# Example

To help you get started and showcase `rsmtp` in action, we have built an
[example SMTP server](https://github.com/conradkleinespel/rustastic-smtp-test-server).

# Documentation

Rustastic SMTP uses Rust's built-in documentation system.

You can build the latest documentation using [Cargo](http://crates.io/) like so:

```shell
git clone https://github.com/conradkleinespel/rustastic-smtp.git
cd rustastic-smtp
cargo doc
```

Then, open the file `target/doc/rsmtp/index.html` in your browser of choice.

# Running tests

This project is linked with [rust-ci](http://rust-ci.org/conradkleinespel/rustastic-smtp)
where you can see the latest build status.

If you would like to run the tests yourself, here's how to do that, using
[Cargo](http://crates.io/):

```shell
git clone https://github.com/conradkleinespel/rustastic-smtp.git
cd rustastic-smtp
cargo test
```

# License

Rustastic SMTP is distributed under the terms of the Apache License (Version 2.0).
See [LICENSE.txt](LICENSE.txt) for more information.
