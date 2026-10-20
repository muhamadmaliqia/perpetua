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
   set_mark_price (market creator only) ── demo pump/crash override ──▶ mark_price
```

**On-chain accounts (PDAs)**
| Account | Seeds | Holds |
|---|---|---|
| `Market` | `["market", feed]` | authority, oracle feed, mark price, open interest, skew/funding, kind + expiry, resolution price, last Pyth publish time |
| `Position` | `["position", market, owner]` | side, size, collateral, entry price |
| `Vault` | `["vault", market]` | the liquidity pool (native SOL) |

**Instructions**
| Instruction | What it does |
|---|---|
| `create_market` | Ignite a perp *or* prediction market (with expiry) for a feed + seed initial liquidity — *permissionless* |
| `open_position` | Long/short with leverage; collateral locked into the vault |
| `close_position` | Settle PnL vs mark price (or the locked resolution price); payout = `max(0, equity)` |
| `liquidate_position` | **Permissionless** force-close when equity ≤ 5% of notional; liquidator earns a 1% bounty |
| `update_mark_from_pyth` | **Permissionless** — pushes a Pyth price the program verifies itself: owner = Pyth Receiver, full verification, matching feed, ≤ 120 s old, confidence ≤ 1%, never older than the last one |
| `resolve_market` | **Permissionless**, after expiry — locks a verified Pyth price published in `[expiry, expiry + 2h]` as the result |
| `set_mark_price` | Market-creator override that drives the demo pump/crash controls. Never used to resolve; the UI labels it "Creator-set" |

Pyth accounts are decoded by hand in `programs/perpetua/src/pyth.rs` (layout mirrored from
`pyth-solana-receiver-sdk` 2.0.0 and checked against a real devnet account in the tests) —
the SDK pulls in the monolithic `solana-program` crate, which clashes with Agave 4.x.

Collateral is **native SOL** (no SPL setup) for a fast, frictionless demo.

---

## Status

| Piece | State |
|---|---|
| On-chain program (7 instructions) | ✅ `cargo test` **12 passing** — trade loop, +25% close, liquidation, skew fill, funding, prediction resolve from Pyth, creator can't fake the result, bad-oracle rejections, permissionless Pyth push, parser vs real devnet Pyth bytes |
| Client wiring, end-to-end | ✅ `bash e2e-local.sh` **3/3** on a local validator — perp loop; prediction vs a real Pyth account cloned from devnet (correctly waits); prediction vs a synthetic test fixture (resolves, settles) |
| Frontend (React) | ✅ builds clean; perp + prediction (countdown, resolve from Pyth, locked price), honest price-source label |
| Fresh Pyth prices on localnet | ⏳ needs a Pyth Hermes key (signed updates are key-gated). Without one, Pyth's sponsored accounts on devnet/mainnet update about hourly |
| Devnet deployment | ⏳ pending (awaiting devnet SOL) |

---

## Run it

**Prerequisites:** Rust + Solana CLI (Agave 4.x) + Anchor 1.2 + Node 20+.

```bash
# 1. Program: build + test
anchor build
cargo test -p perpetua              # 12 passing
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
- Retire the creator price override on markets that have live Pyth prices
- Free-form event markets (the full "market on anything" vision)
