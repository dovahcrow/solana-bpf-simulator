use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FasterSBPFExecutorError {
    #[error("Instruction is too long")]
    InvalidInstruction,

    #[error("Account data is too long")]
    InvalidAccount,
}
