use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstructionExecutorError {
    #[error("Instruction is too long")]
    InvalidInstruction,

    #[error("Account data is too long")]
    InvalidAccount,

    #[error("Program has not been loaded")]
    MissingProgram,
}
