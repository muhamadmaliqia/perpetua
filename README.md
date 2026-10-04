# Perpetua — the market never closes

**Spin up a leveraged market on *any* number in ~10 seconds. Permissionless.**

Perpetua turns any [Pyth](https://pyth.network) price feed into a live, leveraged,
on-chain market — no orderbook, no listing committee, no waiting. It unifies
**perps and prediction markets into one primitive**: if a number has a feed, you can
trade leverage on it, right now.

Built on Solana (Anchor) for the *Perps & Prediction Markets* hackathon.

---

## The wedge

Every perps DEX ships the same 10 markets (SOL, BTC, ETH…). The hard part was never
*trading* a market — it's **creating** one. Perpetua makes market creation itself
permissionless and instant:

- **Anyone** calls `create_market(feed)` and a leveraged market exists ~10s later.
- The counterparty is an **automated liquidity pool** (a simple vAMM) — no orderbook,
  no matching engine, no market makers to court.
- Each market's oracle identity is a **Pyth** feed; PnL updates live from the on-chain
  mark, and every action settles on-chain.

Perps on predictions already exists. *Permissionless, instant, any-feed market
**creation*** is the wedge.

---

## How it works

```
            ┌─────────────┐   create_market(feed)     ┌──────────────────────┐
   anyone ─▶│  Perpetua    │──────────────────────────▶│ Market PDA (per feed)│
            │  program     │                            │  mark_price, OI …    │
            │  (Anchor)    │   open / close / liquidate ├──────────────────────┤
   trader ─▶│              │◀──────────────────────────▶│ Position PDA (/user) │
            └──────┬───────┘                            ├──────────────────────┤
                   │  SOL in/out                        │ Vault PDA (liquidity)│
                   ▼                                     └──────────────────────┘
            set_mark_price ──(Pyth keeper in prod · pump/crash in demo)──▶ mark_price
```

**On-chain accounts (PDAs)**
| Account | Seeds | Holds |
|---|---|---|
| `Market` | `["market", feed]` | authority, oracle feed, mark price, open interest |
| `Position` | `["position", market, owner]` | side, size, collateral, entry price |
| `Vault` | `["vault", market]` | the liquidity pool (native SOL) |

**Instructions**
| Instruction | What it does |
|---|---|
| `create_market` | Ignite a market for a feed + seed initial liquidity — *permissionless* |
| `open_position` | Long/short with leverage; collateral locked into the vault |
| `close_position` | Settle PnL vs mark price; payout = `max(0, equity)` |
| `liquidate_position` | **Permissionless** force-close when equity ≤ 5% of notional; liquidator earns a 1% bounty |
| `set_mark_price` | Price ingress — a Pyth keeper in production; driven by demo pump/crash controls here |

Collateral is **native SOL** (no SPL setup) for a fast, frictionless demo.

---

## Status

| Piece | State |
|---|---|
| On-chain program (5 instructions) | ✅ complete — `cargo test` **4 passing** (trade loop, +25% close, liquidation, self-liquidation) |
| Deploy path | ✅ verified on a validator (~1.53 SOL, 219 KB program) |
| Client/frontend wiring | ✅ verified end-to-end via smoke test against a live program |
| Frontend (React, dark-terminal UI) | ✅ builds clean; wired to all 5 instructions, live PnL from the on-chain mark |
| Devnet deployment | ⏳ pending (awaiting devnet SOL) |

---

## Run it

**Prerequisites:** Rust + Solana CLI (Agave 4.x) + Anchor 1.2 + Node 20+.

```bash
# 1. Program: build + test
anchor build
cargo test -p perpetua              # 4 passing: trade loop, +25% close, liquidation, self-liquidation

# 2. Deploy (devnet)
solana airdrop 2                    # fund the wallet (or use faucet.solana.com)
anchor deploy --provider.cluster devnet

# 3. Frontend
cd app
npm install
npm run dev                         # http://localhost:5173  (connect a devnet wallet)
```

### Or run the whole thing locally — no faucet needed

Everything is verified on a local validator, so you can drive the full product without
any devnet SOL:

```bash
bash demo-local.sh                  # starts a validator, deploys, funds — leave it running
# in another terminal:
cd app && npm run dev
```

Open **http://localhost:5173?rpc=http://localhost:8899** — on localnet the app spins up an
auto-funded burner wallet, so there's **no wallet popup and no faucet**. Just hit
**Ignite market** and trade.

---

## Demo flow (the 60 seconds that matter)

1. Pick a Pyth feed (SOL / BTC / ETH) → **⚡ Ignite market** — live in ~10s.
2. Open a **5× long**. PnL **ticks live** from the on-chain mark.
3. Price drops → margin breaches → **anyone** can liquidate it (and earn the bounty).
4. Close a winner → **payout on-chain**, straight to the wallet.

---

## Why Solana

Sub-second finality + cheap transactions make **live-ticking, per-second PnL** feel
real instead of laggy, and **Pyth** provides first-party, high-frequency price feeds
native to the chain — exactly what a "market on any number, updating constantly"
needs. On a slower or pricier chain, the core experience simply doesn't land.

---

## Roadmap (post-hackathon)

- On-chain Pyth verification (pull-oracle price updates) replacing the keeper ingress
- Real vAMM depth / price impact + funding rates
- Free-form event markets (the full "market on anything" vision)
