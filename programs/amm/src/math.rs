use anchor_lang::prelude::*;

use crate::constants::FEE_DENOMINATOR;
use crate::error::AmmError;

fn div_ceil(numerator: u128, denominator: u128) -> Result<u128> {
    require!(denominator > 0, AmmError::Arithmetic);

    let quotient = numerator / denominator;

    if numerator % denominator == 0 {
        Ok(quotient)
    } else {
        quotient
            .checked_add(1)
            .ok_or_else(|| error!(AmmError::Arithmetic))
    }
}

pub fn integer_sqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }

    let mut x = n;
    let mut y = x / 2 + 1;

    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }

    x
}

pub fn calculate_fee(amount: u64, fee_bps: u16) -> Result<u64> {
    require!((fee_bps as u128) < FEE_DENOMINATOR, AmmError::InvalidFee);
    let fee = (amount as u128)
        .checked_mul(fee_bps as u128)
        .ok_or_else(|| error!(AmmError::Arithmetic))?
        .checked_div(FEE_DENOMINATOR)
        .ok_or_else(|| error!(AmmError::Arithmetic))?;

    u64::try_from(fee).map_err(|_| error!(AmmError::Arithmetic))
}

pub fn calculate_amount_out(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    fee_bps: u16,
) -> Result<u64> {
    require!(amount_in > 0, AmmError::ZeroAmount);

    require!(
        reserve_in > 0 && reserve_out > 0,
        AmmError::InsufficientLiquidity
    );

    let fee = calculate_fee(amount_in, fee_bps)?;

    let amount_after_fee = amount_in.checked_sub(fee).ok_or(AmmError::Arithmetic)?;

    require!(amount_after_fee > 0, AmmError::ZeroAmount);

    let numerator = (amount_after_fee as u128)
        .checked_mul(reserve_out as u128)
        .ok_or(AmmError::Arithmetic)?;

    let denominator = (reserve_in as u128)
        .checked_add(amount_after_fee as u128)
        .ok_or(AmmError::Arithmetic)?;

    let amount_out = numerator
        .checked_div(denominator)
        .ok_or(AmmError::Arithmetic)?;

    let amount_out = u64::try_from(amount_out).map_err(|_| error!(AmmError::Arithmetic))?;

    require!(
        amount_out > 0 && amount_out < reserve_out,
        AmmError::InsufficientLiquidity
    );

    Ok(amount_out)
}

pub fn calculate_deposit(
    max_x: u64,
    max_y: u64,
    reserve_x: u64,
    reserve_y: u64,
    total_lp_supply: u64,
) -> Result<(u64, u64, u64)> {
    require!(max_x > 0 && max_y > 0, AmmError::ZeroAmount);

    // First liquidity provider establishes the initial pool ratio.
    if total_lp_supply == 0 {
        require!(reserve_x == 0 && reserve_y == 0, AmmError::InvalidPoolState);

        let product = (max_x as u128)
            .checked_mul(max_y as u128)
            .ok_or(AmmError::Arithmetic)?;

        let shares = integer_sqrt(product);

        let shares = u64::try_from(shares).map_err(|_| error!(AmmError::Arithmetic))?;

        require!(shares > 0, AmmError::InsufficientLiquidity);

        return Ok((max_x, max_y, shares));
    }

    require!(reserve_x > 0 && reserve_y > 0, AmmError::InvalidPoolState);

    // Determine how many LP shares each side can support.
    let shares_x = (max_x as u128)
        .checked_mul(total_lp_supply as u128)
        .ok_or(AmmError::Arithmetic)?
        / reserve_x as u128;

    let shares_y = (max_y as u128)
        .checked_mul(total_lp_supply as u128)
        .ok_or(AmmError::Arithmetic)?
        / reserve_y as u128;

    let shares = core::cmp::min(shares_x, shares_y);

    require!(shares > 0, AmmError::InsufficientLiquidity);

    // Calculate the exact token amounts required for those shares.
    let used_x = div_ceil(
        shares
            .checked_mul(reserve_x as u128)
            .ok_or(AmmError::Arithmetic)?,
        total_lp_supply as u128,
    )?;

    let used_y = div_ceil(
        shares
            .checked_mul(reserve_y as u128)
            .ok_or(AmmError::Arithmetic)?,
        total_lp_supply as u128,
    )?;

    require!(
        used_x <= max_x as u128 && used_y <= max_y as u128,
        AmmError::InvalidPoolState
    );

    Ok((
        u64::try_from(used_x).map_err(|_| error!(AmmError::Arithmetic))?,
        u64::try_from(used_y).map_err(|_| error!(AmmError::Arithmetic))?,
        u64::try_from(shares).map_err(|_| error!(AmmError::Arithmetic))?,
    ))
}

pub fn calculate_withdrawal_amounts(
    lp_to_burn: u64,
    total_lp_supply: u64,
    reserve_x: u64,
    reserve_y: u64,
) -> Result<(u64, u64)> {
    require!(lp_to_burn > 0, AmmError::ZeroAmount);

    require!(
        total_lp_supply > 0 && lp_to_burn <= total_lp_supply,
        AmmError::InsufficientLiquidity
    );

    let amount_x = (lp_to_burn as u128)
        .checked_mul(reserve_x as u128)
        .ok_or(AmmError::Arithmetic)?
        / total_lp_supply as u128;

    let amount_y = (lp_to_burn as u128)
        .checked_mul(reserve_y as u128)
        .ok_or(AmmError::Arithmetic)?
        / total_lp_supply as u128;

    Ok((
        u64::try_from(amount_x).map_err(|_| error!(AmmError::Arithmetic))?,
        u64::try_from(amount_y).map_err(|_| error!(AmmError::Arithmetic))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_calculation() {
        assert_eq!(calculate_fee(100_000, 30).unwrap(), 300);
    }

    #[test]
    fn test_swap_output() {
        let output = calculate_amount_out(100_000, 1_000_000, 1_000_000, 30).unwrap();

        assert_eq!(output, 90_661);
    }

    #[test]
    fn test_fee_reduces_swap_output() {
        let without_fee = calculate_amount_out(100_000, 1_000_000, 1_000_000, 0).unwrap();

        let with_fee = calculate_amount_out(100_000, 1_000_000, 1_000_000, 30).unwrap();

        assert!(with_fee < without_fee);
    }

    #[test]
    fn test_initial_liquidity_deposit() {
        let result = calculate_deposit(1_000, 1_000, 0, 0, 0).unwrap();

        assert_eq!(result, (1_000, 1_000, 1_000));
    }

    #[test]
    fn test_proportional_liquidity_deposit() {
        let result = calculate_deposit(200, 300, 1_000, 1_000, 1_000).unwrap();

        assert_eq!(result, (200, 200, 200));
    }

    #[test]
    fn test_proportional_withdrawal() {
        let result = calculate_withdrawal_amounts(100, 1_000, 5_000, 8_000).unwrap();

        assert_eq!(result, (500, 800));
    }

    #[test]
    fn test_zero_input_rejected() {
        assert!(calculate_amount_out(0, 1_000, 1_000, 30).is_err());
    }

    #[test]
    fn test_empty_reserves_rejected() {
        assert!(calculate_amount_out(100, 0, 1_000, 30).is_err());
    }

    #[test]
    fn test_integer_square_root() {
        assert_eq!(integer_sqrt(15), 3);
        assert_eq!(integer_sqrt(16), 4);
    }
}
