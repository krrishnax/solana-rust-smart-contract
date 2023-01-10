use solana_program::program_error::ProgramError;
use thiserror::Error;

// We'll need errors that we can use in the following situations:
// The update instruction has been invoked on an account that hasn't been initialized yet
// The provided PDA doesn't match the expected or derived PDA
// The input data is larger than the program allows
// The rating provided does not fall in the 1-5 range

#[derive(Debug, Error)]
pub enum ReviewError{
    // Error 0
    #[error("Account not initialized yet")]
    UninitializedAccount,
    // Error 1
    #[error("PDA derived does not equal PDA passed in")]
    InvalidPDA,
    // Error 2
    #[error("Input data exceeds max length")]
    InvalidDataLength,
    // Error 3
    #[error("Rating greater than 5 or less than 1")]
    InvalidRating,
}

impl From<ReviewError> for ProgramError {
    fn from(e: ReviewError) -> Self {
        ProgramError::Custom(e as u32)
    }
}