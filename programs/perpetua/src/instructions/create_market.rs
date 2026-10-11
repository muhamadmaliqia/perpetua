use anchor_lang::prelude::*;
use anchor_lang::system_program;

use crate::{
    constants::*,
    error::PerpetuaError,
    state::{Market, Vault},
};

#[derive(Accounts)]
#[instruction(oracle_feed: Pubkey)]
pub struct CreateMarket<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,
    #[account(
        init,
        payer = authority,
        space = 8 + Market::INIT_SPACE,
        seeds = [MARKET_SEED, oracle_feed.as_ref()],
        bump
    )]
    pub market: Account<'info, Market>,
    #[account(
        init,
        payer = authority,
        space = 8 + Vault::INIT_SPACE,
        seeds = [VAULT_SEED, market.key().as_ref()],
        bump
    )]
    pub vault: Account<'info, Vault>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_market(
    ctx: Context<CreateMarket>,
    oracle_feed: Pubkey,
    initial_price: u64,
    initial_liquidity: u64,
    skew_scale: u64,
    market_kind: u8,
    expiry_ts: i64,
) -> Result<()> {
    require!(initial_price > 0, PerpetuaError::InvalidPrice);
    require!(skew_scale > 0, PerpetuaError::InvalidSkewScale);
    require!(
        market_kind == MARKET_KIND_PERP || market_kind == MARKET_KIND_PREDICTION,
        PerpetuaError::InvalidMarketKind
    );

    let now = Clock::get()?.unix_timestamp;
    // Perp: expiry dipaksa 0 (perpetual). Prediction: wajib jatuh tempo di masa depan.
    let expiry = if market_kind == MARKET_KIND_PREDICTION {
        require!(expiry_ts > now, PerpetuaError::InvalidExpiry);
        expiry_ts
    } else {
        0
    };

    let vault_key = ctx.accounts.vault.key();

    let vault = &mut ctx.accounts.vault;
    vault.market = ctx.accounts.market.key();
    vault.bump = ctx.bumps.vault;

    let market = &mut ctx.accounts.market;
    market.authority = ctx.accounts.authority.key();
    market.oracle_feed = oracle_feed;
    market.mark_price = initial_price;
    market.total_long = 0;
    market.total_short = 0;
    market.skew_scale = skew_scale;
    market.cumulative_funding = 0;
    market.funding_rate = 0;
    market.last_funding_ts = now;
    market.market_kind = market_kind;
    market.expiry_ts = expiry;
    market.resolved = false;
    market.resolution_price = 0;
    market.vault_bump = ctx.bumps.vault;
    market.bump = ctx.bumps.market;

    // Setor likuiditas awal ke kolam lawan-dagang.
    if initial_liquidity > 0 {
        let cpi_accounts = system_program::Transfer {
            from: ctx.accounts.authority.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(anchor_lang::system_program::ID, cpi_accounts);
        system_program::transfer(cpi_ctx, initial_liquidity)?;
    }

    msg!(
        "Market ignited: vault={} price={} liq={} kind={} expiry={}",
        vault_key,
        initial_price,
        initial_liquidity,
        market_kind,
        expiry
    );
    Ok(())
}
