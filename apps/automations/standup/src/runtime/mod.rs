//! Bootstrap and host integration. [`app`] is the `App`/`Instance` descriptor
//! the console registers; [`run`] builds the Lark bot and serves the WS command
//! handler + scheduler, shared by both the embedded instance and the standalone
//! binary.

pub mod app;
pub mod run;
