#!/usr/bin/env bash
# Perpetua: E2E LOKAL satu perintah (versi CLI, jalur client SAMA kayak frontend):
#   1) smoke.mjs            perp: create -> LONG 5x -> +10% -> close            (delta ~ +0.75 SOL)
#   2) e2e-predict.mjs BTC  prediction, akun Pyth ASLI di-clone dari devnet (beku) -> resolve DITOLAK
#                           (harga terakhir terbit SEBELUM expiry = persis yg dilihat UI di localnet)
#   3) e2e-predict.mjs ETH  prediction, akun Pyth SINTETIS (fixture e2e, BUKAN data asli) terbit SETELAH
#                           expiry -> resolve LOLOS, close settle di harga terkunci
#      (2 & 3 juga: pembuat pasar nyoba set harga prediksi -> DITOLAK CreatorPriceOnPrediction)
#   4) e2e-engine.mjs       angka UI (src/engine.ts) == angka on-chain ke lamport: fill premium, funding,
#                           payout close, equity + bounty likuidasi, batas harga likuidasi (babak panggung DEMO.md)
# Butuh internet (clone akun dari devnet). Validator dimatiin otomatis di akhir.
# Jalanin:  bash e2e-local.sh
set -euo pipefail
cd "$(dirname "$0")"

RPC=http://127.0.0.1:8899
BTC=e62df6c8b4a85fe1a67db44dc12de5db330f7ac66b72dc658afedf0f4a415b43
ETH=ff61491a931112ddf1bd8147cd1b641375f79f5825126d665480874634fd0ace
BTC_PYTH=4cSM2e6rvbGQUFiJbqytoVMi5GgghSMr8LwVrT9VPSPo # akun sponsored BTC/USD di devnet (owner Pyth Receiver)
XAU_PYTH=2uPQGpm8X4ZkxMHxrAW1QuhXcse1AHEgPih6Xp9NuEWW # akun sponsored XAU/USD di devnet -> harga awal e2e-engine

echo "▶ Fixture Pyth SINTETIS buat ETH (terbit 15 menit dari sekarang)…"
ETH_PYTH=$(cd app && node e2e-pyth-fixture.mjs "$ETH" 3500 900 /tmp/perpetua-e2e-eth.json | cut -d' ' -f1)

echo "▶ Validator lokal (clone BTC asli dari devnet + fixture ETH)…"
solana-test-validator --ledger /tmp/perpetua-e2e-ledger --reset \
  --url https://api.devnet.solana.com --clone "$BTC_PYTH" --clone "$XAU_PYTH" \
  --account "$ETH_PYTH" /tmp/perpetua-e2e-eth.json \
  >/tmp/perpetua-e2e-validator.log 2>&1 &
VPID=$!
trap 'kill $VPID 2>/dev/null || true' EXIT
until solana cluster-version --url "$RPC" >/dev/null 2>&1; do sleep 1; done

echo "▶ Danai wallet + deploy program…"
solana airdrop 100 --url "$RPC" >/dev/null
anchor deploy --provider.cluster localnet

cd app
echo ""; echo "━━ 1/4 PERP (smoke.mjs)"
node smoke.mjs
echo ""; echo "━━ 2/4 PREDICTION · akun Pyth ASLI (clone) -> mesti nunggu"
node e2e-predict.mjs "$BTC" 95000 awaiting
echo ""; echo "━━ 3/4 PREDICTION · akun Pyth SINTETIS terbit setelah expiry -> mesti resolved"
node e2e-predict.mjs "$ETH" 3400 resolved 3500
echo ""; echo "━━ 4/4 ENGINE · angka UI == angka on-chain (fill, funding, payout, likuidasi)"
node e2e-engine.mjs

echo ""
echo "✅ E2E LOKAL: 4/4 OK"
