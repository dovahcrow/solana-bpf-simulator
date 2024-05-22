#[cfg(feature = "instruction")]
mod instruction_executor;

#[cfg(feature = "instruction")]
pub use instruction_executor::*;
