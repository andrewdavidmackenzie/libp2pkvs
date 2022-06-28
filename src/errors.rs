#![allow(missing_docs)]

pub use error_chain::bail;
use error_chain::error_chain;

error_chain! {
    types {
        Error, ErrorKind, ResultExt, Result;
    }

    foreign_links {
        Io(std::io::Error);
        P2P(libp2p::kad::store::Error);
    }
}
