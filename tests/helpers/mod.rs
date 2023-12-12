pub mod bufwrapper;
pub mod client;
pub mod relay;
pub mod server;

#[derive(Debug, PartialEq, Eq)]
pub enum TestOp {
    Read,
    Write,
}
