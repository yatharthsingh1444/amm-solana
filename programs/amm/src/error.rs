use anchor_lang::prelude::*;

#[error_code]
pub enum AmmError {
    #[msg("Invalid fee configuration")]
    InvalidFee,

    #[msg("Invalid token mint order")]
    InvalidMintOrder,

    #[msg("Amount must be greater than zero")]
    ZeroAmount,

    #[msg("Insufficient liquidity")]
    InsufficientLiquidity,

    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,

    #[msg("Pool is currently locked")]
    PoolLocked,

    #[msg("Unauthorized authority")]
    Unauthorized,

    #[msg("Arithmetic overflow or underflow")]
    Arithmetic,

    #[msg("Invalid pool state")]
    InvalidPoolState,
}
