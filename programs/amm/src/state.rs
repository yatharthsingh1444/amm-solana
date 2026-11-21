use anchor_lang::prelude::*;

#[account]
pub struct PoolConfig {
    pub authority: Pubkey,

    pub mint_x: Pubkey,
    pub mint_y: Pubkey,

    pub vault_x: Pubkey,
    pub vault_y: Pubkey,

    pub lp_mint: Pubkey,

    pub treasury_authority: Pubkey,
    pub treasury_x: Pubkey,
    pub treasury_y: Pubkey,

    pub fee_bps: u16,
    pub locked: bool,

    pub bump: u8,
    pub treasury_bump: u8,
}

impl PoolConfig {
    pub const SPACE: usize = 8 + (9 * 32) + 2 + 1 + 2;
}
