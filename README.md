## infotainer

![Test](https://github.com/joppich/infotainer/workflows/Rust/badge.svg)
[![codecov](https://codecov.io/gh/joppich/infotainer/branch/master/graph/badge.svg)](https://codecov.io/gh/joppich/infotainer)

Distributed messaging. Very much WIP.

### Components
* Tabloid (message broker component):
    - Create message topics and watch the list of current topics
    - Subscribe and publish messages to topics
    - Archive, replay and delete topics
* Chronicle (storage component):
    - store received messages
    - maintain indices of topics
* Editorial (metadata tracking component):
    - save metadata on topics and messages
* infotainer-common (shared library for package)

### Development
Design decisions and detailed development directions are documented by simple architecture design records.  
They can be found <a href="https://github.com/joppich/infotainer/docs/adr/">here</a>.

### Is it any good?
[yes](https://news.ycombinator.com/item?id=3067434)

### License

Licensed under either of <a href="docs/LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="docs/LICENSE-MIT">MIT license</a> at your option.


Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Serde by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

