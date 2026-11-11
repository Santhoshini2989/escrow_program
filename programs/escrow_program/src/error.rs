use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("The maker and taker must be different accounts.")]
    MakerCannotBeTaker,
    #[msg("The mint accounts must be different.")]
    SameMint,
    #[msg("The deposit and requested amount must be greater than zero.")]
    InvalidAmount,
}