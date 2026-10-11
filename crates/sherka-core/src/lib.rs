pub mod acoustics;
pub mod audio;
pub mod catalogs;
pub mod connections;
pub mod conversation;
pub mod error;
pub mod memory;
pub mod model;
pub mod models;
pub mod providers;
pub mod runtime;
pub mod session;
mod speech_detector;
pub mod storage;
pub mod voice;
pub mod window;

pub use error::{Error, Result};
pub use runtime::Companion;
