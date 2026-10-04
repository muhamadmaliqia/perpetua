use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::{
    constants::*,
    error::PerpetuaError,
    state::{Market, Position, Vault},
};

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,
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
    #[account(
        init,
        payer = trader,
        space = 8 + Position::INIT_SPACE,
        seeds = [POSITION_SEED, market.key().as_ref(), trader.key().as_ref()],
        bump
    )]
    pub position: Account<'info, Position>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_position(
    ctx: Context<OpenPosition>,
    collateral: u64,
    leverage: u64,
    is_long: bool,
) -> Result<()> {
    require!(collateral > 0, PerpetuaError::ZeroCollateral);
    require!(
        (1..=MAX_LEVERAGE).contains(&leverage),
        PerpetuaError::InvalidLeverage
    );
    require!(ctx.accounts.market.mark_price > 0, PerpetuaError::InvalidPrice);

    let size = collateral
        .checked_mul(leverage)
        .ok_or(PerpetuaError::MathOverflow)?;
    let entry_price = ctx.accounts.market.mark_price;
    let market_key = ctx.accounts.market.key();

    // Kunci jaminan ke kolam lawan-dagang.
    let cpi_accounts = system_program::Transfer {
        from: ctx.accounts.trader.to_account_info(),
        to: ctx.accounts.vault.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(anchor_lang::system_program::ID, cpi_accounts);
    system_program::transfer(cpi_ctx, collateral)?;

    let position = &mut ctx.accounts.position;
    position.owner = ctx.accounts.trader.key();
    position.market = market_key;
    position.is_long = is_long;
    position.size = size;
    position.collateral = collateral;
    position.entry_price = entry_price;
    position.bump = ctx.bumps.position;

    let market = &mut ctx.accounts.market;
    if is_long {
        market.total_long = market
            .total_long
            .checked_add(size)
            .ok_or(PerpetuaError::MathOverflow)?;
    } else {
        market.total_short = market
            .total_short
            .checked_add(size)
            .ok_or(PerpetuaError::MathOverflow)?;
    }

    msg!(
        "Position opened: {} size={} entry={} collateral={}",
        if is_long { "LONG" } else { "SHORT" },
        size,
        entry_price,
        collateral
    );
    Ok(())
}
