# PERPETUA — Design Brief (hand this to the design pass)

## 0. The job, in one line
Restyle an existing **React + Vite + TypeScript** dApp into a **premium, shippable Solana-native perps UI** — something that looks like it belongs next to Drift, Jupiter, or Hyperliquid. **Not** a template, not a generic SaaS landing, not "purple-gradient-on-everything." Dark, precise, financial, confident. **UI only — do not change app logic.**

## 1. Why it matters
Hackathon: Solana "Perps & Prediction Markets" ($100K, deadline Oct 9, 2026). Judges ask *"could this be a real app people actually use?"* and they **watch a ≤3-minute video first** — the UI is the first thing they feel, before any code. Current UI reads cheap/amateur ("Windows XP"); it must read as a real product.

## 2. Product context (so the design serves the story)
- **Perpetua** = spin up a leveraged, perpetual-style market on **ANY Pyth price feed in ~10 seconds, permissionless.** Counterparty is an automated liquidity pool (vAMM, no orderbook). Settlement on-chain. It **unifies perps + prediction markets into one primitive.**
- Tagline: **"The market never closes."**
- Collateral is SOL. Live PnL ticks against the oracle price. Liquidation is **permissionless** (anyone can liquidate an unhealthy position for a bounty).

## 3. The golden flow = the screens that MUST shine (in demo order)
This exact sequence is the video. Beats 5–7 must read **instantly on camera** (big numbers, obvious color shift):
1. **Empty / connect state** — first impression. Brand + tagline + one CTA ("connect wallet").
2. **Feed select + price hero** — pick SOL/BTC/ETH; a big, confident price number.
3. **IGNITE** — the "⚡ NYALAKAN PASAR" (Ignite Market) button — the permissionless *wow*; a live market appears in ~10s.
4. **Open position** — LONG/SHORT toggle, collateral + leverage inputs, "BUKA" (Open).
5. **Live PnL** — position card, PnL ticking green/red (~1.5s). **This is the "why Solana" moment** (sub-second finality → per-second PnL).
6. **Pump +25% → payout** — the *win* beat: PnL goes big green (+125% on 5x), close → payout.
7. **Crash -25% → liquidation** — the *risk* beat: PnL red, permissionless liquidate → bounty.

> Note: there are two authority-only "demo" buttons — **pump +25%** and **crash -25%** — used to stage beats 6 & 7 on camera (real oracle won't move on cue). Style them as clearly secondary/utility, visually distinct from real actions.

## 4. Visual direction
- **Mood:** premium DeFi **trading terminal** — precise, high-contrast, data-dense but legible. Think "Bloomberg terminal meets crypto-native." Confident and quiet, not playful, not a marketing page. (The team likes a terminal feel — keep that *energy*, but make it premium, not retro.)
- **Study these for the bar:** drift.trade · app.hyperliquid.xyz · jup.ag · phantom.app · solana.com.
- **Solana identity:** the purple→green signature is the accent, used with restraint on 1–2 hero elements — **not** as wallpaper on every button.

## 5. Tokens (starting point — refine with real taste; don't treat as gospel)
- **Background:** near-black, layered surfaces (e.g. `#08080e` base, slightly lighter cards). Consider a *very* subtle brand glow, or drop it if it muddies.
- **Accents:** Solana purple `#9945FF`, green `#14F195`. Loss/red ~`#FF5C72`. Text `#EDEDF2`, dim `#8B8BA3`.
- **Gradient:** `104deg, #9945FF → #14F195` — reserve for the brand mark and/or the single primary CTA. Not every button.
- **Type:** numbers need **`font-variant-numeric: tabular-nums`** and a precise grotesk; UI needs a clean sans. Inter + Space Grotesk is a *safe* baseline — feel free to pick type with more character (a distinctive grotesk, or a refined mono for figures) if it lifts it. Bundle via `@fontsource/*` (preferred, offline-safe) or Google Fonts.
- Define a **consistent spacing scale, radius scale, hairline borders (rgba white ~.07), and one soft low shadow** — consistency is what separates "product" from "slop."

## 6. Component inventory (current CSS class names = the contract)
Keep these class names so no logic breaks — or list any you change and I'll update the JSX. All styling lives in `src/index.css`; markup in `src/App.tsx`.

| Class / element | What it is |
|---|---|
| `.wrap` | page container (~760px, single column) |
| `.topbar`, `.brand` (+`.tick`), `.tagline` | header: "PERPETUA▮" + "THE MARKET NEVER CLOSES" |
| `.net` (+`.dot`, `.bal`) | network pill: "● devnet · 2.00 SOL" |
| `.wallet-adapter-button-trigger` | wallet connect / address button (3rd-party; override via CSS) |
| `.center` | empty state ("connect wallet to start") |
| `.panel` (+ `h3`) | the card used for every section |
| `.feedrow`, `.chip` (+`.active`) | feed selector pills (SOL/BTC/ETH) |
| `.price-hero` (`.sym`, `.px`, `.px.live`), `.src` (`.pyth`) | big price + source line |
| `.btn.ignite` | the "⚡ NYALAKAN PASAR" primary CTA |
| `.keeper` (+`.on`, `.live-dot`) | "oracle · Pyth feed" indicator + pulse dot |
| `.grid2`, `.stat` (`.k`, `.v`) | stat tiles (mark price, open interest, size, entry, PnL, equity) |
| `.controls`, `.seg` (long/short), `input` | order form |
| `.btn` (BUKA / TUTUP POSISI) | primary actions |
| `.btn.ghost` (pump/crash demo) · `.btn.danger` (liquidate) | secondary / destructive |
| `.up` / `.down` / `.cyan` | semantic colors (profit / loss / highlight) |
| `.log` | status line (shows tx result / errors) |
| `.row`, `.mut` | layout + muted-text helpers |

## 7. Technical constraints
- **Deliverable:** a rewritten `src/index.css` (+ any *minimal* JSX notes if structure must change). `npm run build` must stay clean (Vite + `tsc`).
- **Dark theme only.** Keep every element (don't delete features). Must stay responsive-ish down to a narrow window.
- **No icon libraries that need network at runtime;** inline SVG is fine. Replace the emoji (⚡, ◎) with real glyphs/SVG if it looks cleaner.
- Don't touch the Solana program or the CLI scripts — **UI layer only.**
- **Price is action-driven, not a live external ticker** (Pyth's public API is now key-gated): the on-chain mark moves on open / close / pump / crash. Don't design UI that implies a streaming external price feed or an auto-updating chart. A static "last price" / sparkline is fine; a fake live ticker is not. The on-chain oracle identity stays a **Pyth feed** — credit Pyth there, not as a live quote.

## 8. AVOID (the "AI-slop" traps — this is the current failure)
- Purple gradient slapped on every button + glows everywhere.
- Default shadcn/Bootstrap/system-font look. Everything centered. Even, lifeless spacing.
- Emoji used as product icons. Low-contrast gray-on-gray. Over-animation.
- Generic "rounded dark card + one gradient button" that could be any project.

## 9. The bar
It should look like a product a16z or the Solana Foundation would screenshot and share. **If it looks like a hackathon template, it failed.** Density, typographic precision, and restraint > decoration.
