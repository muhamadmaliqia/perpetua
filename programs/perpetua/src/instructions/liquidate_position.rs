use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::PerpetuaError,
    state::{Market, Position, Vault},
};

/// Likuidasi permissionless: siapa pun (keeper) boleh menutup paksa posisi yang
/// ekuitasnya jatuh ≤ maintenance margin. Liquidator dapat bounty dari kolam;
/// sisa jaminan trader jadi milik kolam. Rent akun posisi balik ke owner.
#[derive(Accounts)]
pub struct LiquidatePosition<'info> {
    #[account(mut)]
    pub liquidator: Signer<'info>,
    #[account(
        mut,
        seeds = [MARKET_SEED, market.oracle_feed.as_ref()],
        bump = market.bump,
    )]
    pub market: Account<'info, Market>,
    #[account(
        mut,
        seeds = [VAULT_SEED, market.key().as_ref()],
        bump = market.vault_bump,
    )]
    pub vault: Account<'info, Vault>,
    /// Owner posisi — nerima rent balik pas akun ditutup.
    #[account(mut)]
    pub owner: SystemAccount<'info>,
    #[account(
        mut,
        close = owner,
        seeds = [POSITION_SEED, market.key().as_ref(), owner.key().as_ref()],
        bump = position.bump,
        constraint = position.owner == owner.key() @ PerpetuaError::Unauthorized,
    )]
    pub position: Account<'info, Position>,
}

pub fn handle_liquidate_position(ctx: Context<LiquidatePosition>) -> Result<()> {
    let mark = ctx.accounts.market.mark_price;
    require!(mark > 0, PerpetuaError::InvalidPrice);

    let is_long = ctx.accounts.position.is_long;
    let size = ctx.accounts.position.size;
    let equity = ctx.accounts.position.equity(mark);

    // Maintenance margin = mmr% dari notional. Sehat kalau ekuitas > maintenance.
    let maintenance = (size as u128)
        .checked_mul(MAINTENANCE_MARGIN_BPS as u128)
        .ok_or(PerpetuaError::MathOverflow)?
        / BPS_DENOMINATOR as u128;
    require!(
        equity <= maintenance as i128,
        PerpetuaError::PositionHealthy
    );

    // Kurangi exposure pasar.
    {
        let market = &mut ctx.accounts.market;
        if is_long {
            market.total_long = market.total_long.saturating_sub(size);
        } else {
            market.total_short = market.total_short.saturating_sub(size);
        }
    }

    // Bounty ke liquidator dari kolam (jaga vault rent-exempt).
    let bounty = ((size as u128)
        .checked_mul(LIQUIDATION_BOUNTY_BPS as u128)
        .ok_or(PerpetuaError::MathOverflow)?
        / BPS_DENOMINATOR as u128) as u64;

    if bounty > 0 {
        let vault_ai = ctx.accounts.vault.to_account_info();
        let rent_min = Rent::get()?.minimum_balance(vault_ai.data_len());
        let vault_lamports = vault_ai.lamports();
        require!(
            vault_lamports.saturating_sub(bounty) >= rent_min,
            PerpetuaError::InsufficientLiquidity
        );
        **vault_ai.try_borrow_mut_lamports()? = vault_lamports
            .checked_sub(bounty)
            .ok_or(PerpetuaError::MathOverflow)?;

        let liq_ai = ctx.accounts.liquidator.to_account_info();
        let liq_lamports = liq_ai.lamports();
        **liq_ai.try_borrow_mut_lamports()? = liq_lamports
            .checked_add(bounty)
            .ok_or(PerpetuaError::MathOverflow)?;
    }

    msg!(
        "LIQUIDATION: size={} equity={} maintenance={} bounty={}",
        size,
        equity,
        maintenance,
        bounty
    );
    Ok(())
}
