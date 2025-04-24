#[cfg(all(feature = "cxx", feature = "native"))]
compile_error!("Feature cxx and native are mutually exclusive and cannot be enabled together");
#[cfg(feature = "cxx")]
mod aasdk;
#[cfg(feature = "cxx")]
pub use aasdk::*;
