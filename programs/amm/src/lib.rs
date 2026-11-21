use anchor_lang::prelude::*;

use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Burn, Mint, MintTo, Token, TokenAccount, Transfer},
};

pub mod constants;
pub mod error;
pub mod math;
pub mod state;

use constants::*;
use error::AmmError;
use state::PoolConfig;

declare_id!("EJj4J33tg8xQUyHdDMfn2J1Y2iCjUBURPYwg2Wm7eUzs");

#[program]
pub mod amm {
    use super::*;


    pub fn initialize(ctx: Context<Initialize>, fee_bps: u16) -> Result<()> {
        require!(
            fee_bps >= PROTOCOL_FEE_BPS && fee_bps <= MAX_FEE_BPS,
            AmmError::InvalidFee
        );

        // Require canonical mint order so one token pair maps
        // to exactly one pool PDA.
        require!(
            ctx.accounts.mint_x.key().to_bytes() < ctx.accounts.mint_y.key().to_bytes(),
            AmmError::InvalidMintOrder
        );

        let pool = &mut ctx.accounts.pool;

        pool.authority = ctx.accounts.authority.key();

        pool.mint_x = ctx.accounts.mint_x.key();
        pool.mint_y = ctx.accounts.mint_y.key();

        pool.vault_x = ctx.accounts.vault_x.key();
        pool.vault_y = ctx.accounts.vault_y.key();

        pool.lp_mint = ctx.accounts.lp_mint.key();

        pool.treasury_authority = ctx.accounts.treasury_authority.key();

        pool.treasury_x = ctx.accounts.treasury_x.key();
        pool.treasury_y = ctx.accounts.treasury_y.key();

        pool.fee_bps = fee_bps;
        pool.locked = false;

        pool.bump = ctx.bumps.pool;
        pool.treasury_bump = ctx.bumps.treasury_authority;

        msg!("AMM pool initialized");

        Ok(())
    }


    pub fn deposit(
        ctx: Context<Deposit>,
        max_x: u64,
        max_y: u64,
        min_lp_tokens: u64,
    ) -> Result<()> {
        require!(!ctx.accounts.pool.locked, AmmError::PoolLocked);

        // Read the current pool reserves and LP supply.
        let reserve_x = ctx.accounts.vault_x.amount;
        let reserve_y = ctx.accounts.vault_y.amount;

        let lp_supply = ctx.accounts.lp_mint.supply;

        // Calculate exact deposit amounts and LP shares.
        let (used_x, used_y, lp_tokens) =
            math::calculate_deposit(max_x, max_y, reserve_x, reserve_y, lp_supply)?;

        require!(lp_tokens >= min_lp_tokens, AmmError::SlippageExceeded);

        // Transfer Token X from user to pool.
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.user_token_x.to_account_info(),
                    to: ctx.accounts.vault_x.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            used_x,
        )?;

        // Transfer Token Y from user to pool.
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.user_token_y.to_account_info(),
                    to: ctx.accounts.vault_y.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            used_y,
        )?;

        // The pool PDA is the authority of the LP mint.
        let mint_x = ctx.accounts.pool.mint_x;
        let mint_y = ctx.accounts.pool.mint_y;
        let bump_seed = [ctx.accounts.pool.bump];

        let pool_seeds: &[&[u8]] = &[POOL_SEED, mint_x.as_ref(), mint_y.as_ref(), &bump_seed];

        let signer_seeds = &[pool_seeds];

        // Mint LP tokens to the user.
        token::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                MintTo {
                    mint: ctx.accounts.lp_mint.to_account_info(),
                    to: ctx.accounts.user_lp_token.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                signer_seeds,
            ),
            lp_tokens,
        )?;

        msg!(
            "Deposited X: {}, Y: {}, LP tokens: {}",
            used_x,
            used_y,
            lp_tokens
        );

        Ok(())
    }



    pub fn swap(
        ctx: Context<Swap>,
        amount_in: u64,
        min_amount_out: u64,
        x_to_y: bool,
    ) -> Result<()> {
        require!(!ctx.accounts.pool.locked, AmmError::PoolLocked);

        require!(amount_in > 0, AmmError::ZeroAmount);

        let fee_bps = ctx.accounts.pool.fee_bps;

        // This is the protocol's share of the swap fee.
        let protocol_fee = math::calculate_fee(amount_in, PROTOCOL_FEE_BPS)?;

        let mint_x = ctx.accounts.pool.mint_x;
        let mint_y = ctx.accounts.pool.mint_y;
        let bump_seed = [ctx.accounts.pool.bump];

        let pool_seeds: &[&[u8]] = &[POOL_SEED, mint_x.as_ref(), mint_y.as_ref(), &bump_seed];

        let signer_seeds = &[pool_seeds];

        if x_to_y {
            // X enters the pool; Y leaves the pool.

            let amount_out = math::calculate_amount_out(
                amount_in,
                ctx.accounts.vault_x.amount,
                ctx.accounts.vault_y.amount,
                fee_bps,
            )?;

            require!(amount_out >= min_amount_out, AmmError::SlippageExceeded);

            // User sends X into the pool.
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.user_token_x.to_account_info(),
                        to: ctx.accounts.vault_x.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    },
                ),
                amount_in,
            )?;

            // Move the protocol fee into the treasury.
            if protocol_fee > 0 {
                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.key(),
                        Transfer {
                            from: ctx.accounts.vault_x.to_account_info(),
                            to: ctx.accounts.treasury_x.to_account_info(),
                            authority: ctx.accounts.pool.to_account_info(),
                        },
                        signer_seeds,
                    ),
                    protocol_fee,
                )?;
            }

            // Pool sends Y to the user.
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.vault_y.to_account_info(),
                        to: ctx.accounts.user_token_y.to_account_info(),
                        authority: ctx.accounts.pool.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount_out,
            )?;

            msg!("Swap X -> Y: input {}, output {}", amount_in, amount_out);
        } else {
            // Y enters the pool; X leaves the pool.

            let amount_out = math::calculate_amount_out(
                amount_in,
                ctx.accounts.vault_y.amount,
                ctx.accounts.vault_x.amount,
                fee_bps,
            )?;

            require!(amount_out >= min_amount_out, AmmError::SlippageExceeded);

            // User sends Y into the pool.
            token::transfer(
                CpiContext::new(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.user_token_y.to_account_info(),
                        to: ctx.accounts.vault_y.to_account_info(),
                        authority: ctx.accounts.user.to_account_info(),
                    },
                ),
                amount_in,
            )?;

            // Move the protocol fee into the treasury.
            if protocol_fee > 0 {
                token::transfer(
                    CpiContext::new_with_signer(
                        ctx.accounts.token_program.key(),
                        Transfer {
                            from: ctx.accounts.vault_y.to_account_info(),
                            to: ctx.accounts.treasury_y.to_account_info(),
                            authority: ctx.accounts.pool.to_account_info(),
                        },
                        signer_seeds,
                    ),
                    protocol_fee,
                )?;
            }

            // Pool sends X to the user.
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.vault_x.to_account_info(),
                        to: ctx.accounts.user_token_x.to_account_info(),
                        authority: ctx.accounts.pool.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount_out,
            )?;

            msg!("Swap Y -> X: input {}, output {}", amount_in, amount_out);
        }

        Ok(())
    }



    pub fn withdraw(
        ctx: Context<Withdraw>,
        lp_tokens: u64,
        min_amount_x: u64,
        min_amount_y: u64,
    ) -> Result<()> {
        // Calculate the user's proportional claim.
        let (amount_x, amount_y) = math::calculate_withdrawal_amounts(
            lp_tokens,
            ctx.accounts.lp_mint.supply,
            ctx.accounts.vault_x.amount,
            ctx.accounts.vault_y.amount,
        )?;

        require!(
            amount_x >= min_amount_x && amount_y >= min_amount_y,
            AmmError::SlippageExceeded
        );

        // Burn LP tokens from the user.
        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.key(),
                Burn {
                    mint: ctx.accounts.lp_mint.to_account_info(),
                    from: ctx.accounts.user_lp_token.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            lp_tokens,
        )?;

        // The pool PDA signs transfers from its vaults.
        let mint_x = ctx.accounts.pool.mint_x;
        let mint_y = ctx.accounts.pool.mint_y;
        let bump_seed = [ctx.accounts.pool.bump];

        let pool_seeds: &[&[u8]] = &[POOL_SEED, mint_x.as_ref(), mint_y.as_ref(), &bump_seed];

        let signer_seeds = &[pool_seeds];

        // Send Token X to the user.
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_x.to_account_info(),
                    to: ctx.accounts.user_token_x.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                signer_seeds,
            ),
            amount_x,
        )?;

        // Send Token Y to the user.
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.key(),
                Transfer {
                    from: ctx.accounts.vault_y.to_account_info(),
                    to: ctx.accounts.user_token_y.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                signer_seeds,
            ),
            amount_y,
        )?;

        msg!("Withdrawn X: {}, Y: {}", amount_x, amount_y);

        Ok(())
    }

    // ---------------------------------------------------------
    // 5. WITHDRAW FEES
    // ---------------------------------------------------------

    pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount_x: u64, amount_y: u64) -> Result<()> {
        require!(amount_x > 0 || amount_y > 0, AmmError::ZeroAmount);

        require!(
            amount_x <= ctx.accounts.treasury_x.amount
                && amount_y <= ctx.accounts.treasury_y.amount,
            AmmError::InsufficientLiquidity
        );

        let pool_key = ctx.accounts.pool.key();
        let bump_seed = [ctx.accounts.pool.treasury_bump];

        let treasury_seeds: &[&[u8]] = &[TREASURY_SEED, pool_key.as_ref(), &bump_seed];

        let signer_seeds = &[treasury_seeds];

        if amount_x > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.treasury_x.to_account_info(),
                        to: ctx.accounts.destination_x.to_account_info(),
                        authority: ctx.accounts.treasury_authority.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount_x,
            )?;
        }

        if amount_y > 0 {
            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.key(),
                    Transfer {
                        from: ctx.accounts.treasury_y.to_account_info(),
                        to: ctx.accounts.destination_y.to_account_info(),
                        authority: ctx.accounts.treasury_authority.to_account_info(),
                    },
                    signer_seeds,
                ),
                amount_y,
            )?;
        }

        msg!("Treasury fees withdrawn");

        Ok(())
    }



    pub fn update_config(ctx: Context<UpdateConfig>, new_fee_bps: u16, locked: bool) -> Result<()> {
        require!(
            new_fee_bps >= PROTOCOL_FEE_BPS && new_fee_bps <= MAX_FEE_BPS,
            AmmError::InvalidFee
        );

        let pool = &mut ctx.accounts.pool;

        pool.fee_bps = new_fee_bps;
        pool.locked = locked;

        msg!(
            "Pool configuration updated. Fee: {}, locked: {}",
            new_fee_bps,
            locked
        );

        Ok(())
    }
}

// =============================================================
// ACCOUNT CONTEXTS
// =============================================================

// -------------------------------------------------------------
// INITIALIZE CONTEXT
// -------------------------------------------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = PoolConfig::SPACE,
        seeds = [
            POOL_SEED,
            mint_x.key().as_ref(),
            mint_y.key().as_ref()
        ],
        bump
    )]
    pub pool: Account<'info, PoolConfig>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = mint_x,
        associated_token::authority = pool,
        associated_token::token_program = token_program
    )]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = mint_y,
        associated_token::authority = pool,
        associated_token::token_program = token_program
    )]
    pub vault_y: Account<'info, TokenAccount>,

    /// CHECK: PDA used only as the authority of treasury token accounts.
    #[account(
        seeds = [TREASURY_SEED, pool.key().as_ref()],
        bump
    )]
    pub treasury_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = mint_x,
        associated_token::authority = treasury_authority,
        associated_token::token_program = token_program
    )]
    pub treasury_x: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = mint_y,
        associated_token::authority = treasury_authority,
        associated_token::token_program = token_program
    )]
    pub treasury_y: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        seeds = [LP_MINT_SEED, pool.key().as_ref()],
        bump,
        mint::decimals = LP_DECIMALS,
        mint::authority = pool
    )]
    pub lp_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}



#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [
            POOL_SEED,
            pool.mint_x.as_ref(),
            pool.mint_y.as_ref()
        ],
        bump = pool.bump
    )]
    pub pool: Account<'info, PoolConfig>,

    #[account(
        mut,
        token::mint = pool.mint_x,
        token::authority = user
    )]
    pub user_token_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = pool.mint_y,
        token::authority = user
    )]
    pub user_token_y: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.vault_x,
        token::mint = pool.mint_x,
        token::authority = pool
    )]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.vault_y,
        token::mint = pool.mint_y,
        token::authority = pool
    )]
    pub vault_y: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.lp_mint
    )]
    pub lp_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = lp_mint,
        token::authority = user
    )]
    pub user_lp_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}



#[derive(Accounts)]
pub struct Swap<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [
            POOL_SEED,
            pool.mint_x.as_ref(),
            pool.mint_y.as_ref()
        ],
        bump = pool.bump
    )]
    pub pool: Account<'info, PoolConfig>,

    #[account(
        mut,
        token::mint = pool.mint_x,
        token::authority = user
    )]
    pub user_token_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = pool.mint_y,
        token::authority = user
    )]
    pub user_token_y: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.vault_x,
        token::mint = pool.mint_x,
        token::authority = pool
    )]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.vault_y,
        token::mint = pool.mint_y,
        token::authority = pool
    )]
    pub vault_y: Account<'info, TokenAccount>,

    /// CHECK: Checked against the configured treasury PDA and bump.
    #[account(
        seeds = [TREASURY_SEED, pool.key().as_ref()],
        bump = pool.treasury_bump,
        address = pool.treasury_authority
    )]
    pub treasury_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        address = pool.treasury_x,
        token::mint = pool.mint_x,
        token::authority = treasury_authority
    )]
    pub treasury_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.treasury_y,
        token::mint = pool.mint_y,
        token::authority = treasury_authority
    )]
    pub treasury_y: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}



#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [
            POOL_SEED,
            pool.mint_x.as_ref(),
            pool.mint_y.as_ref()
        ],
        bump = pool.bump
    )]
    pub pool: Account<'info, PoolConfig>,

    #[account(mut, address = pool.lp_mint)]
    pub lp_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = lp_mint,
        token::authority = user
    )]
    pub user_lp_token: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.vault_x,
        token::mint = pool.mint_x,
        token::authority = pool
    )]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.vault_y,
        token::mint = pool.mint_y,
        token::authority = pool
    )]
    pub vault_y: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = pool.mint_x,
        token::authority = user
    )]
    pub user_token_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = pool.mint_y,
        token::authority = user
    )]
    pub user_token_y: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}


#[derive(Accounts)]
pub struct WithdrawFees<'info> {
    pub authority: Signer<'info>,

    #[account(
        address = pool.authority,
        seeds = [
            POOL_SEED,
            pool.mint_x.as_ref(),
            pool.mint_y.as_ref()
        ],
        bump = pool.bump
    )]
    pub pool: Account<'info, PoolConfig>,

    /// CHECK: Checked against the configured treasury PDA and bump.
    #[account(
        seeds = [TREASURY_SEED, pool.key().as_ref()],
        bump = pool.treasury_bump,
        address = pool.treasury_authority
    )]
    pub treasury_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        address = pool.treasury_x,
        token::mint = pool.mint_x,
        token::authority = treasury_authority
    )]
    pub treasury_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        address = pool.treasury_y,
        token::mint = pool.mint_y,
        token::authority = treasury_authority
    )]
    pub treasury_y: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = pool.mint_x,
        token::authority = authority
    )]
    pub destination_x: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = pool.mint_y,
        token::authority = authority
    )]
    pub destination_y: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}


#[derive(Accounts)]
pub struct UpdateConfig<'info> {
    pub authority: Signer<'info>,

    #[account(
        mut,
        address = pool.authority,
        seeds = [
            POOL_SEED,
            pool.mint_x.as_ref(),
            pool.mint_y.as_ref()
        ],
        bump = pool.bump
    )]
    pub pool: Account<'info, PoolConfig>,
}
