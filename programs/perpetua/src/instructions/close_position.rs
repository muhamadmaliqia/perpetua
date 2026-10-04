use anchor_lang::prelude::*;

use crate::{
    constants::*,
    error::PerpetuaError,
    state::{Market, Position, Vault},
};

#[derive(Accounts)]
pub struct ClosePosition<'info> {
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
        mut,
        close = trader,
        seeds = [POSITION_SEED, market.key().as_ref(), trader.key().as_ref()],
        bump = position.bump,
        constraint = position.owner == trader.key() @ PerpetuaError::Unauthorized,
    )]
    pub position: Account<'info, Position>,
}

pub fn handle_close_position(ctx: Context<ClosePosition>) -> Result<()> {
    let mark = ctx.accounts.market.mark_price;
    require!(mark > 0, PerpetuaError::InvalidPrice);

    let is_long = ctx.accounts.position.is_long;
    let position_size = ctx.accounts.position.size;
    let pnl = ctx.accounts.position.unrealized_pnl(mark);
    let equity = ctx.accounts.position.equity(mark);

    // Payout = ekuitas kalau positif; kalau nggak, 0 (rugi max = seluruh jaminan).
    let payout: u64 = if equity > 0 { equity as u64 } else { 0 };

    // Kurangi exposure pasar.
    {
        let market = &mut ctx.accounts.market;
        if is_long {
            market.total_long = market.total_long.saturating_sub(position_size);
        } else {
            market.total_short = market.total_short.saturating_sub(position_size);
        }
    }

    // Bayar payout dari kolam, jaga vault tetap rent-exempt.
    if payout > 0 {
        let vault_ai = ctx.accounts.vault.to_account_info();
        let rent_min = Rent::get()?.minimum_balance(vault_ai.data_len());
        let vault_lamports = vault_ai.lamports();
        require!(
            vault_lamports.saturating_sub(payout) >= rent_min,
            PerpetuaError::InsufficientLiquidity
        );
        **vault_ai.try_borrow_mut_lamports()? = vault_lamports
            .checked_sub(payout)
            .ok_or(PerpetuaError::MathOverflow)?;

        let trader_ai = ctx.accounts.trader.to_account_info();
        let trader_lamports = trader_ai.lamports();
        **trader_ai.try_borrow_mut_lamports()? = trader_lamports
            .checked_add(payout)
            .ok_or(PerpetuaError::MathOverflow)?;
    }

    msg!(
        "Position closed: mark={} pnl={} payout={}",
        mark,
        pnl,
        payout
    );
    Ok(())
}
