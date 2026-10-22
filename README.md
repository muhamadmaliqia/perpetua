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
- Each market's oracle identity is a **Pyth** feed. The program reads Pyth's on-chain
  `PriceUpdateV2` accounts itself (owner, full Wormhole verification, feed id, confidence,
  freshness) — anyone can push a verified price, and every action settles on-chain.
- **Prediction markets are the same engine with an expiry**: when it expires, *anyone* can
  resolve it and the contract locks a verified Pyth price published within two hours after expiry. Nobody
  types the result in — not even the market's creator.

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
   Pyth PriceUpdateV2 ──▶ update_mark_from_pyth (anyone) ──▶ mark_price
                     └──▶ resolve_market (anyone, after expiry) ──▶ resolution_price (locked)
   set_mark_price (market creator, perps with no Pyth price yet) ── demo pump/crash ──▶ mark_price
```

**On-chain accounts (PDAs)**
| Account | Seeds | Holds |
|---|---|---|
| `Market` | `["market", feed, expiry]` | authority, oracle feed, mark price, open interest, skew/funding, kind + expiry, resolution price, last Pyth publish time. A perp uses expiry 0 (one perp per feed); every prediction round has its own expiry, so a feed can host new rounds forever |
| `Position` | `["position", market, owner]` | side, size, collateral, entry price |
| `Vault` | `["vault", market]` | the liquidity pool (native SOL) |

**Instructions**
| Instruction | What it does |
|---|---|
| `create_market` | Ignite a perp (expiry 0) *or* a new prediction round (with expiry) for a feed + seed initial liquidity — *permissionless* |
| `open_position` | Long/short with leverage; collateral locked into the vault |
| `close_position` | Settle PnL vs mark price (or the locked resolution price); payout = `max(0, equity)` |
| `liquidate_position` | **Permissionless** force-close when equity ≤ 5% of notional; liquidator earns a 1% bounty |
| `update_mark_from_pyth` | **Permissionless** — pushes a Pyth price the program verifies itself: owner = Pyth Receiver, full verification, matching feed, ≤ 120 s old, confidence ≤ 1%, never older than the last one |
| `resolve_market` | **Permissionless**, after expiry — locks a verified Pyth price published in `[expiry, expiry + 2h]` as the result |
| `set_mark_price` | Market-creator override that drives the demo pump/crash controls — **perps only, and only until the market's first Pyth price**, after which it is retired for good. Always rejected on prediction markets. Never used to resolve; the UI labels it "Creator-set" |

Pyth accounts are decoded by hand in `programs/perpetua/src/pyth.rs` (layout mirrored from
`pyth-solana-receiver-sdk` 2.0.0 and checked against a real devnet account in the tests) —
the SDK pulls in the monolithic `solana-program` crate, which clashes with Agave 4.x.

The app's trading math — fill price with skew premium, funding accrual, equity after funding,
liquidation price — lives in `app/src/engine.ts`, a line-for-line mirror of the program (same
integer rounding). `app/e2e-engine.mjs` checks it against the chain to the lamport.

Collateral is **native SOL** (no SPL setup) for a fast, frictionless demo.

---

## Status

| Piece | State |
|---|---|
| On-chain program (7 instructions) | ✅ `cargo test` **13 passing** — trade loop, +25% close, liquidation, skew fill, funding, prediction resolve from Pyth, creator can't fake the result (override rejected on predictions and retired after the first Pyth price), bad-oracle rejections, permissionless Pyth push, parser vs real devnet Pyth bytes, many prediction rounds per feed |
| Client wiring, end-to-end | ✅ `bash e2e-local.sh` **4/4** on a local validator — perp loop; prediction vs a real Pyth account cloned from devnet (correctly waits); prediction vs a synthetic test fixture (resolves, settles); engine check: the numbers the UI shows (fill, funding, payout, liquidation) equal the chain's to the lamport |
| Frontend (React) | ✅ builds clean; markets start at the latest Pyth price read from Pyth's on-chain accounts; perp + prediction rounds (countdown, resolve from Pyth, locked price, start a new round any time), live skew + funding strip, estimated fill and price impact before you trade, equity net of funding, honest price-source label, deep links (`?feed=SOL/USD&view=prediction`) |
| Fresh Pyth prices on localnet | ⏳ needs a Pyth Hermes key (signed updates are key-gated). Without one, Pyth's sponsored accounts on devnet/mainnet update about hourly |
| Devnet deployment | ✅ live — program [`GTaf7icvPHNzDMfd1gt8eN5p3fKERRwPaenfWKjGhaGP`](https://explorer.solana.com/address/GTaf7icvPHNzDMfd1gt8eN5p3fKERRwPaenfWKjGhaGP?cluster=devnet) (deployed from this repo's code; the upgrade to per-round prediction markets is pending) |

**Proof on devnet — a prediction market decided by Pyth, not by a person** (JUP/USD, 1-minute expiry, Oct 22 2026):
the market's creator tried to set the price → rejected on-chain (`CreatorPriceOnPrediction`); a resolve right at
expiry → rejected (`OraclePriceBeforeExpiry`: Pyth hadn't published since expiry); Pyth published 99 s after expiry →
[resolve](https://explorer.solana.com/tx/2hPm1mM83JTATKgM9kBwyn1RXZXuvmkaXtLNVJ1AupZg3XoKyEVXieDBjAEBxzBLg7W4uK65fRE4YKkzWefMVark?cluster=devnet)
locked **$0.311346** from the real Pyth account →
[close](https://explorer.solana.com/tx/2QTL3suEK1hZxHQtbwHNAMwKykVea7AvPQnM5vUmUWieEubjuWQ138geDH66AVXHzL2BEjoMsR41yn4bwfFzac1k?cluster=devnet)
settled the position at exactly that price.

---

## Run it

**Prerequisites:** Rust + Solana CLI (Agave 4.x) + Anchor 1.2 + Node 20+.

```bash
# 1. Program: build + test
anchor build
cargo test -p perpetua              # 13 passing
bash e2e-local.sh                   # optional: full client E2E on a local validator (needs internet)

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

- Post fresh Pyth updates from the app (Hermes pull) so prediction markets resolve seconds after expiry
- Pin resolution to the first Pyth update after expiry (`prev_publish_time < expiry <= publish_time`),
  so exactly one price can settle a market instead of any verified price in the 2h window
- Free-form event markets (the full "market on anything" vision)
